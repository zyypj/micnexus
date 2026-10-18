//! Wi‑Fi for iPhones: the iOS app advertises `_micnexus._tcp` over Bonjour
//! (it cannot hear our UDP broadcast). We ask with mDNS *legacy unicast*
//! queries — sent from an ephemeral port, so responders must answer by
//! unicast straight to that port (RFC 6762 §6.7), which Windows Firewall lets
//! through like any reply. Then the PC connects to the phone, as on Android.

use std::{
    collections::{HashMap, HashSet},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    core::{Core, LanPhone},
    lan,
    protocol::{self as proto, Transport},
};

const MDNS: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);
const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_SRV: u16 = 33;
/// Gone from the list after this long without an answer.
const TTL: Duration = Duration::from_secs(6);
/// Even when the phone does not ask for us, introduce ourselves this often
/// (that is how the iPhone builds its list of PCs).
const PRESENCE_EVERY: Duration = Duration::from_secs(3);

// ------------------------------------------------------------- DNS wire --

pub fn query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0]); // flags, QD=1, AN/NS/AR=0
    for label in name.trim_end_matches('.').split('.') {
        let l = label.as_bytes();
        out.push(l.len().min(63) as u8);
        out.extend_from_slice(&l[..l.len().min(63)]);
    }
    out.push(0);
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // IN
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    Ptr(String),
    Txt(Vec<String>),
    Srv { port: u16, target: String },
    A(Ipv4Addr),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub name: String,
    pub data: Data,
}

fn read_name(p: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *p.get(at)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(at + 1)));
        }
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | *p.get(at + 1)? as usize;
            end.get_or_insert(at + 2);
            at = ptr;
            continue;
        }
        labels.push(String::from_utf8_lossy(p.get(at + 1..at + 1 + len)?).into_owned());
        at += 1 + len;
    }
    None
}

/// (name, type) of the first question of a query (used by test responders).
pub fn question(p: &[u8]) -> Option<(String, u16)> {
    if p.len() < 12 || p[2] & 0x80 != 0 || u16::from_be_bytes([p[4], p[5]]) == 0 {
        return None;
    }
    let (name, next) = read_name(p, 12)?;
    let t = p.get(next..next + 2)?;
    Some((name, u16::from_be_bytes([t[0], t[1]])))
}

/// Every resource record of a response (answers, authority, additional).
pub fn parse(p: &[u8]) -> Vec<Record> {
    let mut out = Vec::new();
    if p.len() < 12 || p[2] & 0x80 == 0 {
        return out; // not a response
    }
    let count = |i: usize| u16::from_be_bytes([p[i], p[i + 1]]) as usize;
    let (qd, records) = (count(4), count(6) + count(8) + count(10));
    let mut at = 12;
    for _ in 0..qd {
        let Some((_, next)) = read_name(p, at) else { return out };
        at = next + 4;
    }
    for _ in 0..records {
        let Some((name, next)) = read_name(p, at) else {
            return out;
        };
        let Some(h) = p.get(next..next + 10) else { return out };
        let rtype = u16::from_be_bytes([h[0], h[1]]);
        let len = u16::from_be_bytes([h[8], h[9]]) as usize;
        let start = next + 10;
        let Some(rdata) = p.get(start..start + len) else {
            return out;
        };
        let data = match rtype {
            TYPE_PTR => read_name(p, start).map(|(n, _)| Data::Ptr(n)).unwrap_or(Data::Other),
            TYPE_A if len == 4 => Data::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])),
            TYPE_SRV if len >= 7 => Data::Srv {
                port: u16::from_be_bytes([rdata[4], rdata[5]]),
                target: read_name(p, start + 6).map(|(n, _)| n).unwrap_or_default(),
            },
            TYPE_TXT => {
                let mut strings = Vec::new();
                let mut i = 0;
                while i < rdata.len() {
                    let l = rdata[i] as usize;
                    if let Some(s) = rdata.get(i + 1..i + 1 + l) {
                        strings.push(String::from_utf8_lossy(s).into_owned());
                    }
                    i += 1 + l;
                }
                Data::Txt(strings)
            }
            _ => Data::Other,
        };
        out.push(Record { name, data });
        at = start + len;
    }
    out
}

fn txt_get<'a>(txt: &'a [String], key: &str) -> Option<&'a str> {
    txt.iter().find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

// --------------------------------------------------------------- browse --

#[derive(Debug, Clone)]
struct Phone {
    ip: Ipv4Addr,
    port: u16,
    id: String,
    name: String,
    want: String,
    seen: Instant,
}

pub fn start(core: Arc<Core>) {
    let _ = std::thread::Builder::new()
        .name("bonjour".into())
        .spawn(move || run(core));
}

/// One socket per interface, so the query leaves on every network.
pub fn sockets(networks: &[(Ipv4Addr, u8)]) -> Vec<UdpSocket> {
    networks
        .iter()
        .filter_map(|(ip, _)| {
            let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).ok()?;
            s.set_multicast_if_v4(ip).ok()?;
            s.set_multicast_ttl_v4(255).ok()?;
            s.bind(&SocketAddr::from((*ip, 0)).into()).ok()?;
            let s: UdpSocket = s.into();
            s.set_nonblocking(true).ok()?;
            Some(s)
        })
        .collect()
}

fn run(core: Arc<Core>) {
    let mut networks: Vec<(Ipv4Addr, u8)> = Vec::new();
    let mut socks: Vec<UdpSocket> = Vec::new();
    let mut networks_at = Instant::now() - Duration::from_secs(60);
    let mut last_query = Instant::now() - Duration::from_secs(60);
    let mut phones: HashMap<String, Phone> = HashMap::new();
    let mut presence: HashMap<String, Instant> = HashMap::new();
    let connecting: Arc<Mutex<HashSet<String>>> = Arc::default();
    let mut query_id: u16 = 0x4d4e;
    let mut buf = [0u8; 4096];

    loop {
        std::thread::sleep(Duration::from_millis(50));
        if !core.settings().wifi_enabled {
            phones.clear();
            core.set_bonjour_phones(Vec::new());
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        if networks_at.elapsed() > Duration::from_secs(5) {
            let now = lan::ipv4_networks();
            if now != networks {
                socks = sockets(&now);
                networks = now;
            }
            networks_at = Instant::now();
        }
        if last_query.elapsed() >= Duration::from_millis(1500) {
            last_query = Instant::now();
            query_id = query_id.wrapping_add(1);
            let q = query(query_id, proto::BONJOUR_SERVICE, TYPE_PTR);
            for s in &socks {
                let _ = s.send_to(&q, MDNS);
            }
        }

        // Answers (unicast from each phone's mDNS responder).
        let mut need_txt: Vec<(usize, String)> = Vec::new();
        for (i, s) in socks.iter().enumerate() {
            while let Ok((n, from)) = s.recv_from(&mut buf) {
                let SocketAddr::V4(from) = from else { continue };
                let records = parse(&buf[..n]);
                let instances: Vec<String> = records
                    .iter()
                    .filter_map(|r| match &r.data {
                        Data::Ptr(target) if r.name.eq_ignore_ascii_case(proto::BONJOUR_SERVICE) => {
                            Some(target.clone())
                        }
                        _ => None,
                    })
                    .collect();
                for r in &records {
                    if let Data::Txt(txt) = &r.data
                        && txt_get(txt, "v") == Some("1")
                        && let Some(id) = txt_get(txt, "id").filter(|id| !id.is_empty())
                    {
                        let port = records
                            .iter()
                            .find_map(|x| match &x.data {
                                Data::Srv { port, .. } if x.name == r.name => Some(*port),
                                _ => None,
                            })
                            .unwrap_or(proto::PHONE_PORT);
                        phones.insert(
                            r.name.clone(),
                            Phone {
                                ip: *from.ip(),
                                port,
                                id: id.to_string(),
                                name: txt_get(txt, "name").unwrap_or("iPhone").to_string(),
                                want: txt_get(txt, "want").unwrap_or("").to_string(),
                                seen: Instant::now(),
                            },
                        );
                    }
                }
                for inst in instances {
                    let has_txt = records.iter().any(|r| r.name == inst && matches!(r.data, Data::Txt(_)));
                    if !has_txt {
                        need_txt.push((i, inst));
                    }
                }
            }
        }
        for (i, inst) in need_txt {
            query_id = query_id.wrapping_add(1);
            let _ = socks[i].send_to(&query(query_id, &inst, TYPE_TXT), MDNS);
        }

        phones.retain(|_, p| p.seen.elapsed() < TTL);
        let mut list: Vec<LanPhone> = phones
            .values()
            .map(|p| LanPhone {
                id: p.id.clone(),
                name: p.name.clone(),
                model: "iPhone".into(),
                address: p.ip.to_string(),
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list.dedup_by(|a, b| a.id == b.id);
        core.set_bonjour_phones(list);

        // Connect: right away when the phone wants us, otherwise now and then
        // just to introduce ourselves (the iPhone lists PCs from our INFO).
        let my_id = core.pc_id();
        let session = core.current_device();
        for p in phones.values() {
            if session.as_deref() == Some(p.id.as_str()) {
                continue;
            }
            let wants_us = p.want == my_id;
            let due = presence.get(&p.id).is_none_or(|t| t.elapsed() >= PRESENCE_EVERY);
            if !(due
                || wants_us
                    && presence
                        .get(&p.id)
                        .is_none_or(|t| t.elapsed() >= Duration::from_millis(1500)))
            {
                continue;
            }
            if !connecting.lock().unwrap().insert(p.id.clone()) {
                continue;
            }
            presence.insert(p.id.clone(), Instant::now());
            let (core, connecting, target, id) = (
                core.clone(),
                connecting.clone(),
                SocketAddr::from((p.ip, p.port)),
                p.id.clone(),
            );
            let _ = std::thread::Builder::new()
                .name("bonjour-connect".into())
                .spawn(move || {
                    if let Ok(stream) = TcpStream::connect_timeout(&target, Duration::from_secs(3)) {
                        let _ = stream.set_nodelay(true);
                        core.serve(Box::new(stream), Transport::Wifi, target.ip().to_string(), false);
                    }
                    connecting.lock().unwrap().remove(&id);
                });
        }
        presence.retain(|id, _| phones.values().any(|p| &p.id == id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A response the way mDNSResponder answers a legacy unicast PTR query:
    /// question echoed, PTR answer, TXT/SRV/A as additional records, with
    /// name compression.
    fn sample() -> Vec<u8> {
        let mut p = vec![0x4d, 0x4e, 0x84, 0x00, 0, 1, 0, 1, 0, 0, 0, 3];
        // question at 12: _micnexus._tcp.local PTR IN
        let q = query(0, "_micnexus._tcp.local", TYPE_PTR);
        p.extend_from_slice(&q[12..]);
        // answer: name = pointer to 12, PTR → "iPhone de Ana" + pointer
        p.extend_from_slice(&[0xC0, 12, 0, 12, 0, 1, 0, 0, 0x11, 0x94]);
        let inst_label = b"iPhone de Ana";
        let rdlen = 1 + inst_label.len() + 2;
        p.extend_from_slice(&(rdlen as u16).to_be_bytes());
        let inst_at = p.len();
        p.push(inst_label.len() as u8);
        p.extend_from_slice(inst_label);
        p.extend_from_slice(&[0xC0, 12]);
        // TXT for the instance
        p.extend_from_slice(&[0xC0, inst_at as u8, 0, 16, 0x80, 1, 0, 0, 0x11, 0x94]);
        let kvs: [&[u8]; 4] = [b"v=1", b"id=ph-1", b"name=iPhone de Ana", b"want=pc-9"];
        let len: usize = kvs.iter().map(|k| 1 + k.len()).sum();
        p.extend_from_slice(&(len as u16).to_be_bytes());
        for k in kvs {
            p.push(k.len() as u8);
            p.extend_from_slice(k);
        }
        // SRV port 47820 target "ana.local"
        p.extend_from_slice(&[0xC0, inst_at as u8, 0, 33, 0x80, 1, 0, 0, 0, 120]);
        let target = [3, b'a', b'n', b'a', 5, b'l', b'o', b'c', b'a', b'l', 0];
        p.extend_from_slice(&((6 + target.len()) as u16).to_be_bytes());
        p.extend_from_slice(&[0, 0, 0, 0]);
        p.extend_from_slice(&47820u16.to_be_bytes());
        p.extend_from_slice(&target);
        // A
        p.extend_from_slice(&[0xC0, 12, 0, 1, 0x80, 1, 0, 0, 0, 120, 0, 4, 192, 168, 0, 20]);
        p
    }

    #[test]
    fn query_layout() {
        let q = query(7, "_micnexus._tcp.local", TYPE_PTR);
        assert_eq!(&q[..6], &[0, 7, 0, 0, 0, 1]);
        assert_eq!(q[12], 9);
        assert_eq!(&q[13..22], b"_micnexus");
        assert_eq!(&q[q.len() - 4..], &[0, 12, 0, 1]);
    }

    #[test]
    fn parses_a_compressed_response() {
        let r = parse(&sample());
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].name, "_micnexus._tcp.local");
        assert_eq!(r[0].data, Data::Ptr("iPhone de Ana._micnexus._tcp.local".into()));
        let Data::Txt(txt) = &r[1].data else { panic!("txt") };
        assert_eq!(txt_get(txt, "id"), Some("ph-1"));
        assert_eq!(txt_get(txt, "want"), Some("pc-9"));
        assert_eq!(
            r[2].data,
            Data::Srv {
                port: 47820,
                target: "ana.local".into()
            }
        );
        assert_eq!(r[3].data, Data::A(Ipv4Addr::new(192, 168, 0, 20)));
    }

    #[test]
    fn rejects_queries_and_garbage() {
        assert!(parse(&query(1, "x.local", TYPE_PTR)).is_empty());
        assert!(parse(&[0xff; 5]).is_empty());
        let mut bad = sample();
        bad.truncate(40);
        let _ = parse(&bad); // no panic on truncation
        let mut looped = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        looped.extend_from_slice(&[0xC0, 12]); // name pointing at itself
        assert!(parse(&looped).is_empty());
    }
}
