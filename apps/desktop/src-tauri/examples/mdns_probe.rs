//! Checks that mDNS legacy unicast answers reach us through Windows Firewall:
//! asks the LAN which Bonjour services exist (or for one service type).
//!
//!   cargo run --example mdns_probe -- [_services._dns-sd._udp.local]
use std::time::{Duration, Instant};

use micnexus_desktop_lib::{lan, mdns};

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "_services._dns-sd._udp.local".into());
    let nets = lan::ipv4_networks();
    println!("interfaces: {nets:?}");
    let socks = mdns::sockets(&nets);
    let mdns_addr = "224.0.0.251:5353";
    for (i, s) in socks.iter().enumerate() {
        let _ = s.send_to(&mdns::query(0x1234 + i as u16, &name, 12), mdns_addr);
    }
    let start = Instant::now();
    let mut buf = [0u8; 4096];
    let mut answers = 0;
    while start.elapsed() < Duration::from_secs(3) {
        for s in &socks {
            while let Ok((n, from)) = s.recv_from(&mut buf) {
                answers += 1;
                for r in mdns::parse(&buf[..n]) {
                    println!("{from}  {}  {:?}", r.name, r.data);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
        if start.elapsed() > Duration::from_millis(1000) && start.elapsed() < Duration::from_millis(1030) {
            for (i, s) in socks.iter().enumerate() {
                let _ = s.send_to(&mdns::query(0x2234 + i as u16, &name, 12), mdns_addr);
            }
        }
    }
    println!("{answers} unicast answers");
}
