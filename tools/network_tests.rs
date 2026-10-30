#![allow(dead_code)]
#[path = "../src/net_model.rs"]
mod net_model;
use net_model::*;
const MAC: Mac = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
const ROUTER: Mac = [0x52, 0x55, 10, 0, 2, 2];
const IP: Ip = [10, 0, 2, 15];
const GW: Ip = [10, 0, 2, 2];
fn put(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_be_bytes());
}
fn ethernet(kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 14];
    b[..6].copy_from_slice(&MAC);
    b[6..12].copy_from_slice(&ROUTER);
    put(&mut b, 12, kind);
    b.extend_from_slice(payload);
    b
}
fn ipv4(protocol: u8, source: Ip, dest: Ip, payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 20];
    b[0] = 0x45;
    b[8] = 64;
    b[9] = protocol;
    put(&mut b, 2, (20 + payload.len()) as u16);
    b[12..16].copy_from_slice(&source);
    b[16..20].copy_from_slice(&dest);
    let sum = checksum(&b);
    put(&mut b, 10, sum);
    b.extend_from_slice(payload);
    ethernet(0x800, &b)
}
fn udp(payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 8];
    put(&mut b, 0, 67);
    put(&mut b, 2, 68);
    put(&mut b, 4, (8 + payload.len()) as u16);
    b.extend_from_slice(payload);
    b
}
fn xid(stack: &mut Stack, now: u32) -> u32 {
    stack.restart(now);
    stack.tick(now);
    let n = u32::from_be_bytes(stack.outgoing().unwrap().bytes[46..50].try_into().unwrap());
    stack.sent();
    n
}
fn dhcp(xid: u32, kind: u8, lease: u32) -> Vec<u8> {
    let mut b = vec![0; 240];
    b[0] = 2;
    b[1] = 1;
    b[2] = 6;
    b[4..8].copy_from_slice(&xid.to_be_bytes());
    b[16..20].copy_from_slice(&IP);
    b[28..34].copy_from_slice(&MAC);
    b[236..240].copy_from_slice(&[99, 130, 83, 99]);
    b.extend_from_slice(&[
        53, 1, kind, 54, 4, 10, 0, 2, 2, 1, 4, 255, 255, 255, 0, 3, 4, 10, 0, 2, 2, 6, 4, 10, 0, 2,
        3, 51, 4,
    ]);
    b.extend_from_slice(&lease.to_be_bytes());
    b.push(255);
    b
}
fn bound(now: u32, lease: u32) -> Stack {
    let mut s = Stack::new(MAC);
    let id = xid(&mut s, now);
    s.receive(&ipv4(17, GW, BROADCAST, &udp(&dhcp(id, 2, lease))), now + 1);
    assert_eq!(s.dhcp, Dhcp::Requesting);
    s.tick(now + 1);
    s.sent();
    s.receive(&ipv4(17, GW, IP, &udp(&dhcp(id, 5, lease))), now + 2);
    assert_eq!(s.dhcp, Dhcp::Bound);
    s
}
fn arp() -> Vec<u8> {
    let mut b = vec![0; 28];
    put(&mut b, 0, 1);
    put(&mut b, 2, 0x800);
    b[4] = 6;
    b[5] = 4;
    put(&mut b, 6, 2);
    b[8..14].copy_from_slice(&ROUTER);
    b[14..18].copy_from_slice(&GW);
    b[18..24].copy_from_slice(&MAC);
    b[24..28].copy_from_slice(&IP);
    ethernet(0x806, &b)
}
#[test]
fn addresses_and_checksum_vectors() {
    assert_eq!(parse_ip("10.0.2.15"), Some(IP));
    for text in [
        "1.2.3",
        "1.2.3.4.5",
        "1.2.3.256",
        "1.2.-3.4",
        "1..3.4",
        " 1.2.3.4",
        "1.2.3.4x",
        "+1.2.3.4",
    ] {
        assert!(parse_ip(text).is_none());
    }
    assert_eq!(
        checksum(&[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]),
        0x220d
    );
    assert_eq!(checksum(&[0x12, 0x34, 0x56]), 0x97cb);
}
#[test]
fn dhcp_identity_binding_and_lease_wraparound() {
    let now = u32::MAX - 100;
    let mut s = bound(now, 10);
    let c = s.config.unwrap();
    assert_eq!(c.ip, IP);
    assert_eq!(c.gateway, GW);
    assert_eq!(c.dns, [10, 0, 2, 3]);
    let before = s.dropped;
    s.receive(&ipv4(17, GW, IP, &udp(&dhcp(42, 5, 10))), now + 3);
    assert_eq!(s.dropped, before + 1);
    s.tick(now.wrapping_add(873));
    assert_eq!(s.dhcp, Dhcp::Rebinding);
    let request = s.outgoing().unwrap();
    assert_eq!(&request.bytes[54..58], &IP);
    assert_eq!(&request.bytes[26..30], &IP);
    s.tick(now.wrapping_add(1003));
    assert_eq!(s.dhcp, Dhcp::Selecting);
    assert!(s.config.is_none());
}
#[test]
fn malformed_dhcp_options_cannot_bind_or_change_a_lease() {
    for variant in 0..10 {
        let mut s = Stack::new(MAC);
        let id = xid(&mut s, 0);
        let mut b = dhcp(id, 2, 100);
        match variant {
            0 => b[0] = 1,
            1 => b[28] ^= 1,
            2 => b[236] = 0,
            3 => {
                b.pop();
            }
            4 => b.extend_from_slice(&[53, 1, 2]),
            5 => {
                let at = b.len() - 1;
                b.splice(at..at, [53, 1, 2]);
            }
            6 => b[244] = 250,
            7 => b[16] = 224,
            8 => {
                let at = b.len() - 1;
                b.splice(at..at, [52, 1, 1]);
            }
            _ => b[252] = 0,
        };
        s.receive(&ipv4(17, GW, BROADCAST, &udp(&b)), 1);
        // Bytes after END are ignored by RFC DHCP; all other corruptions fail closed.
        if variant != 4 {
            assert_eq!(s.dhcp, Dhcp::Selecting, "variant {variant}");
        }
    }
    let mut s = bound(0, 100);
    let old = s.config.unwrap().ip;
    let mut nak = dhcp(0, 6, 100);
    nak[16..20].fill(0);
    s.receive(&ipv4(17, GW, BROADCAST, &udp(&nak)), 10);
    assert_eq!(s.config.unwrap().ip, old);
}
#[test]
fn arp_routing_echo_matching_duplicates_and_checksums() {
    let mut s = bound(0, 100);
    assert!(s.start_ping(GW, 3));
    s.tick(3);
    let request = *s.outgoing().unwrap();
    assert_eq!(&request.bytes[38..42], &GW);
    s.sent();
    s.receive(&arp(), 4);
    s.tick(4);
    let request = *s.outgoing().unwrap();
    assert_eq!(request.bytes[34], 8);
    assert_eq!(&request.bytes[..6], &ROUTER);
    assert_eq!(checksum(&request.bytes[14..34]), 0);
    s.sent();
    let mut echo = request.bytes[34..request.len].to_vec();
    echo[0] = 0;
    put(&mut echo, 2, 0);
    let sum = checksum(&echo);
    put(&mut echo, 2, sum);
    let reply = ipv4(1, GW, IP, &echo);
    let mut bad = reply.clone();
    bad[45] ^= 1;
    s.receive(&bad, 5);
    assert_eq!(s.ping.received, 0);
    s.receive(&reply, 6);
    assert_eq!(s.ping.received, 1);
    s.receive(&reply, 7);
    assert_eq!(s.ping.received, 1);
    s.tick(104);
    assert_eq!(s.ping.sent, 2);
    s.sent();
    s.tick(204);
    assert_eq!(s.ping.lost, 1);
    let mut outside = bound(0, 100);
    assert!(outside.start_ping([192, 0, 2, 1], 3));
    outside.tick(3);
    assert_eq!(&outside.outgoing().unwrap().bytes[38..42], &GW);
}
#[test]
fn fragments_udp_corruption_and_truncations_are_rejected() {
    let mut s = Stack::new(MAC);
    let id = xid(&mut s, 0);
    let valid = ipv4(17, GW, BROADCAST, &udp(&dhcp(id, 2, 100)));
    for n in 0..valid.len() {
        s.receive(&valid[..n], 1);
        assert_eq!(s.dhcp, Dhcp::Selecting, "prefix {n}");
    }
    for flags in [0x2000, 1, 0x8000] {
        let mut p = valid.clone();
        put(&mut p, 20, flags);
        put(&mut p, 24, 0);
        let sum = checksum(&p[14..34]);
        put(&mut p, 24, sum);
        s.receive(&p, 1);
        assert_eq!(s.dhcp, Dhcp::Selecting);
    }
    let mut bad = valid.clone();
    put(&mut bad, 40, 0x1234);
    s.receive(&bad, 1);
    assert_eq!(s.dhcp, Dhcp::Selecting);
    s.receive(&valid, 1);
    assert_eq!(s.dhcp, Dhcp::Requesting);
}
#[test]
fn retries_arp_timeouts_and_packet_queues_are_bounded() {
    let mut s = Stack::new(MAC);
    xid(&mut s, 0);
    for now in [400, 1200, 2800, 4400] {
        s.tick(now);
        s.sent();
    }
    assert_eq!(s.dhcp, Dhcp::Failed);
    let mut s = bound(0, 100);
    assert!(s.start_ping(GW, 3));
    for now in [3, 103, 203, 303] {
        s.tick(now);
        s.sent();
    }
    assert!(!s.ping.active);
    assert_eq!(s.ping.sent, 0);
    assert!(!s.start_ping(BROADCAST, 304));
    assert!(!s.start_ping([10, 0, 2, 255], 304));
    assert!(!s.start_ping([10, 0, 2, 0], 304));
    let mut packet = arp();
    put(&mut packet, 20, 1);
    packet[32..38].fill(0);
    for _ in 0..100 {
        s.receive(&packet, 400);
    }
    assert!(s.queue_drops > 0);
    let mut count = 0;
    while s.outgoing().is_some() {
        s.sent();
        count += 1;
    }
    assert_eq!(count, 4);
}
#[test]
fn lease_renews_in_place_and_recovers_from_a_nak() {
    let mut s = bound(0, 100);
    s.receive(&arp(), 10);
    s.tick(5002);
    assert_eq!(s.dhcp, Dhcp::Renewing);
    let p = *s.outgoing().unwrap();
    s.sent();
    assert_eq!(&p.bytes[..6], &ROUTER);
    assert_eq!(&p.bytes[30..34], &GW);
    assert_eq!(&p.bytes[54..58], &IP);
    let id = u32::from_be_bytes(p.bytes[46..50].try_into().unwrap());
    s.receive(&ipv4(17, GW, IP, &udp(&dhcp(id, 5, 200))), 5003);
    assert_eq!(s.dhcp, Dhcp::Bound);
    assert_eq!(s.config.unwrap().lease, 200);
    s.tick(15003);
    assert_eq!(s.dhcp, Dhcp::Renewing);
    let mut nak = dhcp(id, 6, 200);
    nak[16..20].fill(0);
    s.receive(&ipv4(17, GW, BROADCAST, &udp(&nak)), 15004);
    assert_eq!(s.dhcp, Dhcp::Selecting);
    assert!(s.config.is_none());
}
#[test]
fn renewal_continues_as_soon_as_arp_resolves() {
    let mut s = bound(0, 10);
    s.tick(502);
    assert_eq!(s.dhcp, Dhcp::Renewing);
    assert_eq!(&s.outgoing().unwrap().bytes[12..14], &[8, 6]);
    s.sent();
    s.receive(&arp(), 503);
    s.tick(503);
    let p = s.outgoing().unwrap();
    assert_eq!(&p.bytes[12..14], &[8, 0]);
    assert_eq!(&p.bytes[30..34], &GW);
}
#[test]
fn mutated_network_input_never_panics_or_grows_state() {
    let mut seed = 0x12345678u32;
    let mut s = bound(0, 100);
    let base = ipv4(17, GW, IP, &udp(&dhcp(10, 5, 100)));
    for i in 0..20000 {
        let mut b = base.clone();
        for _ in 0..i % 7 + 1 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = seed as usize % b.len();
            b[n] ^= (seed >> 24) as u8;
        }
        if i % 3 == 0 {
            let n = seed as usize % b.len();
            b.truncate(n);
        }
        s.receive(&b, i);
        while s.outgoing().is_some() {
            s.sent();
        }
    }
    assert_eq!(s.config.unwrap().ip, IP);
}
