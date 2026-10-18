//! Wi‑Fi: the PC asks "who is there?" by broadcast, phones answer, and the PC
//! opens the TCP connection to the phone that wants it. Everything goes out
//! from the PC, so Windows Firewall never needs a rule (docs/PROTOCOL.md).

use std::{
    collections::{HashMap, HashSet},
    net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    core::{Core, LanPhone, WifiStatus},
    protocol::{self as proto, PhoneReply, Transport},
};

/// A phone not heard from for this long leaves the list.
const PHONE_TTL: Duration = Duration::from_secs(5);

pub fn start(core: Arc<Core>) {
    let _ = std::thread::Builder::new().name("lan".into()).spawn(move || run(core));
}

fn run(core: Arc<Core>) {
    let socket = loop {
        match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
            Ok(s) => break s,
            Err(e) => {
                core.set_wifi(WifiStatus {
                    ok: false,
                    addresses: Vec::new(),
                    phones: Vec::new(),
                    error: Some(e.to_string()),
                });
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    };
    let _ = socket.set_broadcast(true);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));

    let connecting: Arc<Mutex<HashSet<String>>> = Arc::default();
    let mut retry_after: HashMap<String, Instant> = HashMap::new();
    let mut phones: HashMap<String, (LanPhone, Instant)> = HashMap::new();
    let mut networks: Vec<(Ipv4Addr, u8)> = Vec::new();
    let mut networks_at = Instant::now() - Duration::from_secs(60);
    let mut last_send = Instant::now() - Duration::from_secs(60);
    let mut buf = [0u8; 2048];

    loop {
        if !core.settings().wifi_enabled {
            phones.clear();
            core.set_wifi(WifiStatus {
                ok: false,
                addresses: Vec::new(),
                phones: Vec::new(),
                error: None,
            });
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        if networks_at.elapsed() > Duration::from_secs(5) {
            networks = ipv4_networks();
            networks_at = Instant::now();
        }
        let interval = if core.is_connected() {
            Duration::from_secs(3)
        } else {
            Duration::from_secs(1)
        };
        if last_send.elapsed() >= interval {
            last_send = Instant::now();
            let mut packet = proto::DISCOVER_REQUEST.to_vec();
            packet.extend(serde_json::to_vec(&core.pc_info()).unwrap_or_default());
            let mut targets: Vec<Ipv4Addr> = networks.iter().map(|(ip, prefix)| broadcast(*ip, *prefix)).collect();
            targets.push(Ipv4Addr::BROADCAST);
            targets.dedup();
            for t in targets {
                let _ = socket.send_to(&packet, (t, proto::DISCOVERY_PORT));
            }
        }

        if let Ok((n, from)) = socket.recv_from(&mut buf)
            && let Some(reply) = parse_reply(&buf[..n])
            && let SocketAddr::V4(from) = from
        {
            let address = from.ip().to_string();
            phones.insert(
                reply.id.clone(),
                (
                    LanPhone {
                        id: reply.id.clone(),
                        name: reply.name.clone(),
                        model: reply.model.clone(),
                        address,
                    },
                    Instant::now(),
                ),
            );
            let wants_us = reply.want.as_deref() == Some(core.pc_id().as_str());
            let free = core.current_device().is_none_or(|d| d == reply.id);
            let waited = retry_after.get(&reply.id).is_none_or(|t| Instant::now() >= *t);
            if wants_us && free && waited && connecting.lock().unwrap().insert(reply.id.clone()) {
                retry_after.insert(reply.id.clone(), Instant::now() + Duration::from_secs(2));
                let (core, connecting) = (core.clone(), connecting.clone());
                let target = SocketAddr::from((*from.ip(), reply.port));
                let id = reply.id.clone();
                let _ = std::thread::Builder::new().name("lan-connect".into()).spawn(move || {
                    if let Ok(stream) = TcpStream::connect_timeout(&target, Duration::from_secs(3)) {
                        let _ = stream.set_nodelay(true);
                        core.serve(Box::new(stream), Transport::Wifi, target.ip().to_string());
                    }
                    connecting.lock().unwrap().remove(&id);
                });
            }
        }

        phones.retain(|_, (_, seen)| seen.elapsed() < PHONE_TTL);
        let mut list: Vec<LanPhone> = phones.values().map(|(p, _)| p.clone()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        core.set_wifi(WifiStatus {
            ok: !networks.is_empty(),
            addresses: networks.iter().map(|(ip, _)| ip.to_string()).collect(),
            phones: list,
            error: None,
        });
    }
}

fn parse_reply(packet: &[u8]) -> Option<PhoneReply> {
    let body = packet.strip_prefix(proto::DISCOVER_REPLY.as_slice())?;
    let reply: PhoneReply = serde_json::from_slice(body).ok()?;
    (reply.v == proto::PROTOCOL_VERSION && reply.port != 0).then_some(reply)
}

pub fn broadcast(ip: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32) as u32)
    };
    Ipv4Addr::from(u32::from(ip) | !mask)
}

/// IPv4 addresses (with prefix length) of the adapters that are up, loopback aside.
#[cfg(windows)]
pub fn ipv4_networks() -> Vec<(Ipv4Addr, u8)> {
    use windows::Win32::{
        NetworkManagement::{
            IpHelper::{
                GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
                IP_ADAPTER_ADDRESSES_LH,
            },
            Ndis::IfOperStatusUp,
        },
        Networking::WinSock::{AF_INET, SOCKADDR_IN},
    };
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    let mut out = Vec::new();
    let mut size = 32 * 1024u32;
    for _ in 0..3 {
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let first = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let rc = unsafe { GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(first), &mut size) };
        if rc == 111 {
            continue; // ERROR_BUFFER_OVERFLOW: `size` now holds what is needed
        }
        if rc != 0 {
            return out;
        }
        let mut adapter = first;
        while !adapter.is_null() {
            let a = unsafe { &*adapter };
            if a.OperStatus == IfOperStatusUp && a.IfType != IF_TYPE_SOFTWARE_LOOPBACK {
                let mut unicast = a.FirstUnicastAddress;
                while !unicast.is_null() {
                    let u = unsafe { &*unicast };
                    let sa = u.Address.lpSockaddr;
                    if !sa.is_null() && unsafe { (*sa).sa_family } == AF_INET {
                        let sin = unsafe { &*(sa as *const SOCKADDR_IN) };
                        let ip = Ipv4Addr::from(u32::from_be(unsafe { sin.sin_addr.S_un.S_addr }));
                        if !ip.is_loopback() && !ip.is_link_local() {
                            out.push((ip, u.OnLinkPrefixLength));
                        }
                    }
                    unicast = u.Next;
                }
            }
            adapter = a.Next;
        }
        break;
    }
    out
}

#[cfg(not(windows))]
pub fn ipv4_networks() -> Vec<(Ipv4Addr, u8)> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_addresses() {
        assert_eq!(
            broadcast(Ipv4Addr::new(192, 168, 0, 17), 24),
            Ipv4Addr::new(192, 168, 0, 255)
        );
        assert_eq!(
            broadcast(Ipv4Addr::new(10, 1, 2, 3), 16),
            Ipv4Addr::new(10, 1, 255, 255)
        );
        assert_eq!(broadcast(Ipv4Addr::new(10, 1, 2, 3), 32), Ipv4Addr::new(10, 1, 2, 3));
    }

    #[test]
    fn replies_are_validated() {
        let ok = br#"MNX!{"v":1,"id":"p","name":"Pixel","port":47820,"want":"pc"}"#;
        let r = parse_reply(ok).unwrap();
        assert_eq!(r.want.as_deref(), Some("pc"));
        assert!(parse_reply(br#"MNX?{"v":1}"#).is_none());
        assert!(parse_reply(br#"MNX!{"v":2,"id":"p","name":"x","port":1}"#).is_none());
    }
}
