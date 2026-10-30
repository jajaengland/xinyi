//! Bounded Ethernet/ARP/IPv4/UDP/DHCP/ICMP. No allocation or hardware access.
pub const FRAME: usize = 1536;
pub type Ip = [u8; 4];
pub type Mac = [u8; 6];
pub const ZERO: Ip = [0; 4];
pub const BROADCAST: Ip = [255; 4];
fn word(b: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([b[i], b[i + 1]])
}
fn long(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(b[i..i + 4].try_into().unwrap())
}
fn put(b: &mut [u8], i: usize, n: u16) {
    b[i..i + 2].copy_from_slice(&n.to_be_bytes());
}
pub fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in bytes.chunks(2) {
        sum += ((chunk[0] as u32) << 8) | chunk.get(1).copied().unwrap_or(0) as u32;
    }
    while sum >> 16 != 0 {
        sum = (sum & 65535) + (sum >> 16);
    }
    !(sum as u16)
}
pub fn parse_ip(text: &str) -> Option<Ip> {
    let mut ip = [0; 4];
    let mut parts = text.split('.');
    for byte in &mut ip {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *byte = p.parse().ok()?;
    }
    if parts.next().is_some() {
        None
    } else {
        Some(ip)
    }
}
pub fn unicast(ip: Ip) -> bool {
    ip[0] != 0 && ip[0] != 127 && ip[0] < 224 && ip != BROADCAST
}
fn due(now: u32, when: u32) -> bool {
    now.wrapping_sub(when) < 0x80000000
}
#[derive(Clone, Copy)]
pub struct Config {
    pub ip: Ip,
    pub mask: Ip,
    pub gateway: Ip,
    pub dns: Ip,
    pub server: Ip,
    pub lease: u32,
    renew: u32,
    rebind: u32,
}
#[derive(Clone, Copy)]
pub struct Packet {
    pub bytes: [u8; FRAME],
    pub len: usize,
}
impl Packet {
    fn ethernet(mac: Mac, to: Mac, kind: u16) -> Self {
        let mut p = Self {
            bytes: [0; FRAME],
            len: 14,
        };
        p.bytes[..6].copy_from_slice(&to);
        p.bytes[6..12].copy_from_slice(&mac);
        put(&mut p.bytes, 12, kind);
        p
    }
    fn ip(mac: Mac, to: Mac, source: Ip, dest: Ip, protocol: u8, payload: &[u8], id: u16) -> Self {
        let mut p = Self::ethernet(mac, to, 0x800);
        p.len = 34 + payload.len();
        let b = &mut p.bytes[14..34];
        b[0] = 0x45;
        put(b, 2, (20 + payload.len()) as u16);
        put(b, 4, id);
        put(b, 6, 0x4000);
        b[8] = 64;
        b[9] = protocol;
        b[12..16].copy_from_slice(&source);
        b[16..20].copy_from_slice(&dest);
        let c = checksum(b);
        put(b, 10, c);
        p.bytes[34..p.len].copy_from_slice(payload);
        p
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dhcp {
    Off,
    Selecting,
    Requesting,
    Bound,
    Renewing,
    Rebinding,
    Failed,
}
#[derive(Clone, Copy)]
struct Neighbor {
    ip: Ip,
    mac: Mac,
    until: u32,
}
#[derive(Clone, Copy)]
pub struct Ping {
    pub target: Ip,
    pub sent: u16,
    pub received: u16,
    pub lost: u16,
    pub last_ms: u32,
    pub active: bool,
    id: u16,
    waiting: bool,
    since: u32,
    next: u32,
}
impl Ping {
    const EMPTY: Self = Self {
        target: ZERO,
        sent: 0,
        received: 0,
        lost: 0,
        last_ms: 0,
        active: false,
        id: 0,
        waiting: false,
        since: 0,
        next: 0,
    };
}
pub struct Stack {
    pub mac: Mac,
    pub config: Option<Config>,
    pub dhcp: Dhcp,
    pub ping: Ping,
    pub received: u32,
    pub dropped: u32,
    pub queue_drops: u32,
    xid: u32,
    offer: Option<Config>,
    deadline: u32,
    attempts: u8,
    expires: u32,
    renew: u32,
    rebind: u32,
    neighbors: [Option<Neighbor>; 4],
    arp_next: u32,
    arp_attempts: u8,
    queue: [Packet; 4],
    head: usize,
    count: usize,
    serial: u16,
}
impl Stack {
    pub fn new(mac: Mac) -> Self {
        Self {
            mac,
            config: None,
            dhcp: Dhcp::Off,
            ping: Ping::EMPTY,
            received: 0,
            dropped: 0,
            queue_drops: 0,
            xid: 0,
            offer: None,
            deadline: 0,
            attempts: 0,
            expires: 0,
            renew: 0,
            rebind: 0,
            neighbors: [None; 4],
            arp_next: 0,
            arp_attempts: 0,
            queue: [Packet {
                bytes: [0; FRAME],
                len: 0,
            }; 4],
            head: 0,
            count: 0,
            serial: 0,
        }
    }
    pub fn restart(&mut self, now: u32) {
        self.config = None;
        self.offer = None;
        self.dhcp = Dhcp::Selecting;
        self.attempts = 0;
        self.deadline = now;
        self.xid = now.rotate_left(11)
            ^ u32::from_be_bytes(self.mac[2..6].try_into().unwrap())
            ^ 0x58494e59;
        self.neighbors = [None; 4];
        self.ping.active = false;
        self.count = 0;
        self.head = 0;
    }
    pub fn offline(&mut self) {
        self.config = None;
        self.dhcp = Dhcp::Off;
        self.ping.active = false;
        self.neighbors = [None; 4];
        self.count = 0;
    }
    fn queue(&mut self, p: Packet) {
        if self.count == self.queue.len() {
            self.queue_drops = self.queue_drops.saturating_add(1);
            return;
        }
        self.queue[(self.head + self.count) % 4] = p;
        self.count += 1;
    }
    pub fn outgoing(&self) -> Option<&Packet> {
        if self.count == 0 {
            None
        } else {
            Some(&self.queue[self.head])
        }
    }
    pub fn sent(&mut self) {
        if self.count > 0 {
            self.head = (self.head + 1) % 4;
            self.count -= 1;
        }
    }
    fn dhcp_request(&mut self, now: u32) {
        let mut dest = BROADCAST;
        let mut to = [255; 6];
        if self.dhcp == Dhcp::Renewing {
            let c = self.config.unwrap();
            dest = c.server;
            let mask = u32::from_be_bytes(c.mask);
            let next = if u32::from_be_bytes(dest) & mask == u32::from_be_bytes(c.ip) & mask {
                dest
            } else {
                c.gateway
            };
            if let Some(mac) = self
                .neighbors
                .iter()
                .flatten()
                .find(|v| v.ip == next && !due(now, v.until))
                .map(|v| v.mac)
            {
                to = mac;
            } else {
                if unicast(next) {
                    self.arp(next);
                }
                return;
            }
        }
        let mut bytes = [0u8; 308];
        bytes[0] = 1;
        bytes[1] = 1;
        bytes[2] = 6;
        bytes[4..8].copy_from_slice(&self.xid.to_be_bytes());
        put(&mut bytes, 10, 0x8000);
        bytes[28..34].copy_from_slice(&self.mac);
        bytes[236..240].copy_from_slice(&[99, 130, 83, 99]);
        let kind = if self.dhcp == Dhcp::Selecting { 1 } else { 3 };
        let mut at = 240;
        bytes[at..at + 3].copy_from_slice(&[53, 1, kind]);
        at += 3;
        bytes[at..at + 3].copy_from_slice(&[61, 7, 1]);
        at += 3;
        bytes[at..at + 6].copy_from_slice(&self.mac);
        at += 6;
        let source = if matches!(self.dhcp, Dhcp::Renewing | Dhcp::Rebinding) {
            let ip = self.config.unwrap().ip;
            bytes[12..16].copy_from_slice(&ip);
            if self.dhcp == Dhcp::Renewing {
                put(&mut bytes, 10, 0);
            }
            ip
        } else {
            ZERO
        };
        if self.dhcp == Dhcp::Requesting {
            let c = self.offer.unwrap();
            for (code, ip) in [(50, c.ip), (54, c.server)] {
                bytes[at] = code;
                bytes[at + 1] = 4;
                bytes[at + 2..at + 6].copy_from_slice(&ip);
                at += 6;
            }
        }
        bytes[at..at + 6].copy_from_slice(&[55, 4, 1, 3, 6, 51]);
        at += 6;
        bytes[at] = 255;
        let mut udp = [0; 316];
        put(&mut udp, 0, 68);
        put(&mut udp, 2, 67);
        put(&mut udp, 4, 316);
        udp[8..].copy_from_slice(&bytes);
        self.queue(Packet::ip(
            self.mac,
            to,
            source,
            dest,
            17,
            &udp,
            self.serial,
        ));
        self.serial = self.serial.wrapping_add(1);
    }
    pub fn tick(&mut self, now: u32) {
        if self.config.is_some() && due(now, self.expires) {
            self.restart(now);
        }
        if self.dhcp == Dhcp::Bound && due(now, self.renew) {
            self.dhcp = Dhcp::Renewing;
            self.attempts = 0;
            self.deadline = now;
        }
        if self.dhcp == Dhcp::Renewing && due(now, self.rebind) {
            self.dhcp = Dhcp::Rebinding;
            self.attempts = 0;
            self.deadline = now;
        }
        if matches!(
            self.dhcp,
            Dhcp::Selecting | Dhcp::Requesting | Dhcp::Renewing | Dhcp::Rebinding
        ) && due(now, self.deadline)
        {
            if self.attempts >= 4 && matches!(self.dhcp, Dhcp::Selecting | Dhcp::Requesting) {
                self.dhcp = Dhcp::Failed;
                self.offer = None;
            } else {
                self.dhcp_request(now);
                self.attempts = self.attempts.saturating_add(1);
                self.deadline = now.wrapping_add(400u32 << self.attempts.min(3).saturating_sub(1));
            }
        }
        if !self.ping.active {
            return;
        }
        let Some(c) = self.config else {
            self.ping.active = false;
            return;
        };
        if self.ping.waiting {
            if !due(now, self.ping.since.wrapping_add(100)) {
                return;
            }
            self.ping.waiting = false;
            self.ping.lost += 1;
        }
        if self.ping.sent == 4 {
            self.ping.active = false;
            return;
        }
        if !due(now, self.ping.next) {
            return;
        }
        let on_link = self
            .ping
            .target
            .iter()
            .zip(c.ip.iter())
            .zip(c.mask.iter())
            .all(|((&a, &b), &m)| a & m == b & m);
        let next = if on_link { self.ping.target } else { c.gateway };
        if !unicast(next) {
            self.ping.active = false;
            return;
        }
        let mac = self
            .neighbors
            .iter()
            .flatten()
            .find(|v| v.ip == next && !due(now, v.until))
            .map(|v| v.mac);
        if let Some(mac) = mac {
            let mut payload = [0; 40];
            payload[0] = 8;
            put(&mut payload, 4, self.ping.id);
            put(&mut payload, 6, self.ping.sent + 1);
            for (i, b) in payload[8..].iter_mut().enumerate() {
                *b = i as u8;
            }
            let sum = checksum(&payload);
            put(&mut payload, 2, sum);
            self.queue(Packet::ip(
                self.mac,
                mac,
                c.ip,
                self.ping.target,
                1,
                &payload,
                self.serial,
            ));
            self.serial = self.serial.wrapping_add(1);
            self.ping.sent += 1;
            self.ping.waiting = true;
            self.ping.since = now;
            self.ping.next = now.wrapping_add(100);
        } else if due(now, self.arp_next) {
            if self.arp_attempts >= 3 {
                self.ping.active = false;
                return;
            }
            self.arp(next);
            self.arp_attempts += 1;
            self.arp_next = now.wrapping_add(100);
        }
    }
    pub fn start_ping(&mut self, ip: Ip, now: u32) -> bool {
        if self.config.is_none()
            || self.ping.active
            || !unicast(ip)
            || self.config.is_some_and(|c| ip == c.ip)
        {
            return false;
        }
        let c = self.config.unwrap();
        let bits = u32::from_be_bytes(ip);
        let mask = u32::from_be_bytes(c.mask);
        if bits & mask == u32::from_be_bytes(c.ip) & mask
            && mask != u32::MAX
            && (bits & !mask == 0 || bits & !mask == !mask)
        {
            return false;
        }
        self.ping = Ping {
            target: ip,
            id: (self.xid as u16)
                .wrapping_add(self.serial)
                .wrapping_add(now as u16),
            active: true,
            next: now,
            ..Ping::EMPTY
        };
        self.arp_next = now;
        self.arp_attempts = 0;
        true
    }
    fn arp(&mut self, ip: Ip) {
        let c = self.config.unwrap();
        let mut p = Packet::ethernet(self.mac, [255; 6], 0x806);
        p.len = 42;
        let b = &mut p.bytes[14..42];
        put(b, 0, 1);
        put(b, 2, 0x800);
        b[4] = 6;
        b[5] = 4;
        put(b, 6, 1);
        b[8..14].copy_from_slice(&self.mac);
        b[14..18].copy_from_slice(&c.ip);
        b[24..28].copy_from_slice(&ip);
        self.queue(p);
    }
    pub fn receive(&mut self, frame: &[u8], now: u32) {
        self.received = self.received.saturating_add(1);
        if !self.accept(frame, now) {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
    fn accept(&mut self, frame: &[u8], now: u32) -> bool {
        if frame.len() < 14
            || frame.len() > FRAME
            || !(frame[..6] == self.mac || frame[..6] == [255; 6])
        {
            return false;
        }
        let source: Mac = frame[6..12].try_into().unwrap();
        if source[0] & 1 != 0 || source == [0; 6] {
            return false;
        }
        match word(frame, 12) {
            0x806 => self.receive_arp(&frame[14..], source, now),
            0x800 => self.receive_ip(&frame[14..], source, now),
            _ => false,
        }
    }
    fn receive_arp(&mut self, b: &[u8], source: Mac, now: u32) -> bool {
        let Some(c) = self.config else {
            return false;
        };
        if b.len() < 28
            || word(b, 0) != 1
            || word(b, 2) != 0x800
            || b[4] != 6
            || b[5] != 4
            || b[8..14] != source
            || !matches!(word(b, 6), 1 | 2)
        {
            return false;
        }
        let ip: Ip = b[14..18].try_into().unwrap();
        let target: Ip = b[24..28].try_into().unwrap();
        if target != c.ip || !unicast(ip) || ip == c.ip {
            return false;
        }
        if word(b, 6) == 2 && b[18..24] != self.mac {
            return false;
        }
        let slot = self
            .neighbors
            .iter()
            .position(|v| v.is_none_or(|v| v.ip == ip || due(now, v.until)))
            .unwrap_or(0);
        self.neighbors[slot] = Some(Neighbor {
            ip,
            mac: source,
            until: now.wrapping_add(6000),
        });
        if self.dhcp == Dhcp::Renewing && (ip == c.server || ip == c.gateway) {
            self.deadline = now;
        }
        if word(b, 6) == 1 {
            let mut p = Packet::ethernet(self.mac, source, 0x806);
            p.len = 42;
            let a = &mut p.bytes[14..42];
            a.copy_from_slice(&b[..28]);
            put(a, 6, 2);
            a[8..14].copy_from_slice(&self.mac);
            a[14..18].copy_from_slice(&c.ip);
            a[18..24].copy_from_slice(&source);
            a[24..28].copy_from_slice(&ip);
            self.queue(p);
        }
        true
    }
    fn receive_ip(&mut self, b: &[u8], mac: Mac, now: u32) -> bool {
        if b.len() < 20 || b[0] >> 4 != 4 {
            return false;
        }
        let header = (b[0] & 15) as usize * 4;
        let total = word(b, 2) as usize;
        if header < 20
            || header > b.len()
            || total < header
            || total > b.len()
            || b[8] == 0
            || word(b, 6) & 0xbfff != 0
            || checksum(&b[..header]) != 0
        {
            return false;
        }
        let source: Ip = b[12..16].try_into().unwrap();
        let dest: Ip = b[16..20].try_into().unwrap();
        let payload = &b[header..total];
        if !unicast(source) {
            return false;
        }
        if b[9] == 17 {
            if payload.len() < 8
                || word(payload, 0) != 67
                || word(payload, 2) != 68
                || word(payload, 4) as usize != payload.len()
            {
                return false;
            }
            if word(payload, 6) != 0 {
                let mut pseudo = [0; FRAME];
                pseudo[..4].copy_from_slice(&source);
                pseudo[4..8].copy_from_slice(&dest);
                pseudo[9] = 17;
                put(&mut pseudo, 10, payload.len() as u16);
                pseudo[12..12 + payload.len()].copy_from_slice(payload);
                if checksum(&pseudo[..12 + payload.len()]) != 0 {
                    return false;
                }
            }
            if dest != BROADCAST
                && !self.config.is_some_and(|c| dest == c.ip)
                && !self.offer.is_some_and(|c| dest == c.ip)
            {
                return false;
            }
            return self.receive_dhcp(&payload[8..], now);
        }
        let Some(c) = self.config else {
            return false;
        };
        if dest != c.ip
            || b[9] != 1
            || payload.len() < 8
            || checksum(payload) != 0
            || payload[1] != 0
        {
            return false;
        }
        if payload[0] == 8 && payload.len() <= 512 {
            let mut reply = [0; 512];
            reply[..payload.len()].copy_from_slice(payload);
            reply[0] = 0;
            put(&mut reply, 2, 0);
            let sum = checksum(&reply[..payload.len()]);
            put(&mut reply, 2, sum);
            self.queue(Packet::ip(
                self.mac,
                mac,
                c.ip,
                source,
                1,
                &reply[..payload.len()],
                self.serial,
            ));
            self.serial = self.serial.wrapping_add(1);
            return true;
        }
        if payload[0] != 0
            || !self.ping.active
            || !self.ping.waiting
            || source != self.ping.target
            || word(payload, 4) != self.ping.id
            || word(payload, 6) != self.ping.sent
            || payload.len() != 40
            || !payload[8..].iter().enumerate().all(|(i, &b)| b == i as u8)
        {
            return false;
        }
        self.ping.received += 1;
        self.ping.last_ms = now.wrapping_sub(self.ping.since).saturating_mul(10);
        self.ping.waiting = false;
        if self.ping.sent == 4 {
            self.ping.active = false;
        }
        true
    }
    fn receive_dhcp(&mut self, b: &[u8], now: u32) -> bool {
        if b.len() < 241
            || b[0] != 2
            || b[1] != 1
            || b[2] != 6
            || long(b, 4) != self.xid
            || b[28..34] != self.mac
            || b[236..240] != [99, 130, 83, 99]
        {
            return false;
        }
        let mut c = Config {
            ip: b[16..20].try_into().unwrap(),
            mask: ZERO,
            gateway: ZERO,
            dns: ZERO,
            server: ZERO,
            lease: 0,
            renew: 0,
            rebind: 0,
        };
        let (mut at, mut kind, mut seen, mut end) = (240, 0, 0u8, false);
        while at < b.len() {
            let code = b[at];
            at += 1;
            if code == 255 {
                end = true;
                break;
            }
            if code == 0 {
                continue;
            }
            if at >= b.len() {
                return false;
            }
            let n = b[at] as usize;
            at += 1;
            if at + n > b.len() {
                return false;
            }
            let value = &b[at..at + n];
            at += n;
            let bit = match code {
                53 => 1,
                54 => 2,
                1 => 4,
                3 => 8,
                6 => 16,
                51 => 32,
                58 => 64,
                59 => 128,
                52 => return false,
                _ => 0,
            };
            if bit != 0 && seen & bit != 0 {
                return false;
            }
            seen |= bit;
            match code {
                53 => {
                    if n != 1 {
                        return false;
                    }
                    kind = value[0];
                }
                54 | 1 | 51 | 58 | 59 => {
                    if n != 4 {
                        return false;
                    }
                    match code {
                        54 => c.server = value.try_into().unwrap(),
                        1 => c.mask = value.try_into().unwrap(),
                        51 => c.lease = u32::from_be_bytes(value.try_into().unwrap()),
                        58 => c.renew = u32::from_be_bytes(value.try_into().unwrap()),
                        _ => c.rebind = u32::from_be_bytes(value.try_into().unwrap()),
                    }
                }
                3 | 6 => {
                    if n < 4 || n % 4 != 0 {
                        return false;
                    }
                    if code == 3 {
                        c.gateway = value[..4].try_into().unwrap();
                    } else {
                        c.dns = value[..4].try_into().unwrap();
                    }
                }
                _ => (),
            }
        }
        if !end || seen & 3 != 3 || !unicast(c.server) {
            return false;
        }
        if kind == 6
            && matches!(
                self.dhcp,
                Dhcp::Requesting | Dhcp::Renewing | Dhcp::Rebinding
            )
            && self
                .offer
                .or(self.config)
                .is_some_and(|old| self.dhcp == Dhcp::Rebinding || old.server == c.server)
        {
            self.restart(now);
            return true;
        }
        let mask = u32::from_be_bytes(c.mask);
        let host = u32::from_be_bytes(c.ip) & !mask;
        if !unicast(c.ip)
            || mask == 0
            || (!mask) & (!mask).wrapping_add(1) != 0
            || mask != u32::MAX && (host == 0 || host == !mask)
            || c.lease < 8
        {
            return false;
        }
        if c.gateway != ZERO
            && (!unicast(c.gateway)
                || u32::from_be_bytes(c.gateway) & mask != u32::from_be_bytes(c.ip) & mask)
        {
            return false;
        }
        if c.dns != ZERO && !unicast(c.dns) {
            return false;
        }
        c.lease = c.lease.min(604800);
        if c.renew == 0 || c.renew >= c.lease {
            c.renew = c.lease / 2;
        }
        if c.rebind <= c.renew || c.rebind >= c.lease {
            c.rebind = c.lease * 7 / 8;
        }
        if c.renew >= c.rebind {
            c.renew = c.lease / 2;
            c.rebind = c.lease * 7 / 8;
        }
        if kind == 2 && self.dhcp == Dhcp::Selecting {
            self.offer = Some(c);
            self.dhcp = Dhcp::Requesting;
            self.attempts = 0;
            self.deadline = now;
            return true;
        }
        if kind == 5
            && matches!(
                self.dhcp,
                Dhcp::Requesting | Dhcp::Renewing | Dhcp::Rebinding
            )
        {
            let Some(old) = self.offer.or(self.config) else {
                return false;
            };
            if self.dhcp != Dhcp::Rebinding && c.server != old.server || c.ip != old.ip {
                return false;
            }
            self.config = Some(c);
            self.offer = None;
            self.dhcp = Dhcp::Bound;
            self.expires = now.wrapping_add(c.lease * 100);
            self.renew = now.wrapping_add(c.renew * 100);
            self.rebind = now.wrapping_add(c.rebind * 100);
            self.neighbors = [None; 4];
            return true;
        }
        false
    }
}
