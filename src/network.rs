//! Host-context network service. No parsing, allocation or printing from NIC IRQs.
use crate::net_model::{Dhcp, Ip, Stack};
struct Network {
    device: crate::rtl8139::Device,
    stack: Stack,
    last: u32,
    link: bool,
    notified: bool,
    origin: Option<u32>,
}
static mut NETWORK: Option<Network> = None;
static mut ENABLED: bool = false;
fn address(ip: Ip) -> impl core::fmt::Display {
    struct Address(Ip);
    impl core::fmt::Display for Address {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
        }
    }
    Address(ip)
}
pub fn init() {
    crate::rtl8139::disable_all();
    crate::debug::write_str("Net: disabled by default\n");
}
fn enable() {
    let Some(device) = crate::rtl8139::Device::init() else {
        crate::debug::write_str("Net: no supported RTL8139 adapter\n");
        return;
    };
    let now = crate::interrupts::timer_ticks();
    let link = device.link();
    let mut stack = Stack::new(device.mac);
    if link {
        stack.restart(now);
    }
    unsafe {
        core::ptr::addr_of_mut!(NETWORK).write(Some(Network {
            device,
            stack,
            last: now.wrapping_sub(1),
            link,
            notified: true,
            origin: None,
        }));
    }
    unsafe {
        ENABLED = true;
    }
    poll();
}
pub fn poll() {
    if !unsafe { ENABLED } {
        return;
    }
    let now = crate::interrupts::timer_ticks();
    let Some(n) = (unsafe { (&mut *core::ptr::addr_of_mut!(NETWORK)).as_mut() }) else {
        return;
    };
    if now == n.last {
        return;
    }
    n.last = now;
    let link = n.device.link();
    if link != n.link {
        n.link = link;
        if link {
            n.stack.restart(now);
        } else {
            n.stack.offline();
        }
        crate::debug::write_fmt(format_args!(
            "Net: link {}\n",
            if link { "up" } else { "down" }
        ));
    }
    if !link {
        return;
    }
    let before = n.stack.dhcp;
    let replies = n.stack.ping.received;
    let mut packet = [0; crate::net_model::FRAME];
    for _ in 0..8 {
        let Some(length) = n.device.receive(&mut packet) else {
            break;
        };
        n.stack.receive(&packet[..length], now);
    }
    n.stack.tick(now);
    for _ in 0..4 {
        let Some(frame) = n.stack.outgoing() else {
            break;
        };
        if !n.device.transmit(&frame.bytes[..frame.len]) {
            break;
        }
        n.stack.sent();
    }
    if before != n.stack.dhcp {
        crate::debug::write_fmt(format_args!("Net: DHCP {:?}\n", n.stack.dhcp));
        if n.stack.dhcp == Dhcp::Bound {
            let c = n.stack.config.unwrap();
            crate::debug::write_fmt(format_args!(
                "Net: bound ip={} mask={} gateway={} dns={} lease={}\n",
                address(c.ip),
                address(c.mask),
                address(c.gateway),
                address(c.dns),
                c.lease
            ));
        }
    }
    if replies != n.stack.ping.received {
        crate::debug::write_fmt(format_args!(
            "Net: ping reply target={} seq={} ms={}\n",
            address(n.stack.ping.target),
            n.stack.ping.sent,
            n.stack.ping.last_ms
        ));
    }
    if !n.stack.ping.active && !n.notified {
        n.notified = true;
        let p = n.stack.ping;
        crate::debug::write_fmt(format_args!(
            "Net: ping complete sent={} received={} lost={}\n",
            p.sent, p.received, p.lost
        ));
        let current = crate::pty::current();
        if n.origin.is_some_and(crate::pty::exists)
            || n.origin.is_none() && !crate::multitask::active()
        {
            crate::pty::set_current(n.origin);
            crate::println!(
                "\nPing {}: {} sent, {} received, {} lost.",
                address(p.target),
                p.sent,
                p.received,
                p.lost
            );
            crate::pty::set_current(current);
        }
    }
}
pub fn command(argument: &str) {
    if argument == "experimental-enable" {
        if unsafe { ENABLED } {
            crate::println!("Experimental networking is already enabled.");
            return;
        }
        enable();
        crate::println!(
            "{}",
            if unsafe { ENABLED } {
                "Experimental networking enabled for this boot only."
            } else {
                "Network unavailable: no supported RTL8139 Ethernet adapter."
            }
        );
        return;
    }
    if argument == "off" {
        shutdown();
        crate::println!("Networking disabled.");
        return;
    }
    if !unsafe { ENABLED } {
        crate::println!("Networking disabled (experimental).");
        return;
    }

    poll();
    let Some(n) = (unsafe { (&mut *core::ptr::addr_of_mut!(NETWORK)).as_mut() }) else {
        crate::println!("Network unavailable: no supported RTL8139 Ethernet adapter.");
        return;
    };
    if argument == "dhcp" {
        if !n.link {
            crate::println!("Network cable is disconnected.");
            return;
        }
        n.stack.restart(crate::interrupts::timer_ticks());
        n.last = n.last.wrapping_sub(1);
        crate::println!("DHCP address request started. Type net to check progress.");
        return;
    }
    if !matches!(argument, "" | "status") {
        crate::println!("Usage: net [status|dhcp]");
        return;
    }
    let mac = n.stack.mac;
    crate::println!(
        "Ethernet: RTL8139, link {}",
        if n.link { "up" } else { "down" }
    );
    crate::println!(
        "MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );
    if let Some(c) = n.stack.config {
        crate::println!("IPv4: {}  mask: {}", address(c.ip), address(c.mask));
        crate::println!("Gateway: {}  DNS: {}", address(c.gateway), address(c.dns));
        crate::println!("DHCP lease: {} seconds", c.lease);
    } else {
        crate::println!("DHCP: {:?}", n.stack.dhcp);
    }
    crate::println!(
        "Packets: {} received, {} transmitted; {} rejected, {} driver errors",
        n.device.received,
        n.device.transmitted,
        n.stack.dropped,
        n.device.errors
    );
}
pub fn ping(argument: &str) {
    if !unsafe { ENABLED } {
        crate::println!("Networking disabled (experimental).");
        return;
    }
    poll();
    let Some(n) = (unsafe { (&mut *core::ptr::addr_of_mut!(NETWORK)).as_mut() }) else {
        crate::println!("Network unavailable: no supported RTL8139 Ethernet adapter.");
        return;
    };
    if argument.is_empty() {
        let p = n.stack.ping;
        crate::println!(
            "Ping {}: {} sent, {} received, {} lost; {}",
            address(p.target),
            p.sent,
            p.received,
            p.lost,
            if p.active { "running" } else { "finished" }
        );
        return;
    }
    let Some(ip) = crate::net_model::parse_ip(argument) else {
        crate::println!("Usage: ping A.B.C.D (numeric IPv4 address)");
        return;
    };
    if !n.stack.start_ping(ip, crate::interrupts::timer_ticks()) {
        crate::println!("Ping requires a DHCP address, an idle ping, and a unicast destination.");
        return;
    }
    n.notified = false;
    n.origin = crate::pty::current();
    n.last = n.last.wrapping_sub(1);
    crate::println!(
        "Pinging {} with 4 packets. Type ping to see progress.",
        address(ip)
    );
}
pub fn shutdown() {
    unsafe {
        ENABLED = false;
        if let Some(n) = (&mut *core::ptr::addr_of_mut!(NETWORK)).as_mut() {
            n.stack.offline();
            n.device.stop();
            n.notified = true;
            n.origin = None;
        }
    }
}
