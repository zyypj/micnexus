//! Pretends to be the iOS app on Wi‑Fi: advertises `_micnexus._tcp` with a
//! tiny Bonjour responder (answers legacy unicast queries), takes the PC's
//! "presence" connection with `BYE idle` (that is how the iPhone learns about
//! PCs), then sets `want` to that PC — as if the user tapped it — and streams
//! a 440 Hz tone on the next connection.
//!
//!   cargo run --example fake_iphone -- [seconds]

use std::{
    io::Write,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use micnexus_desktop_lib::{
    lan, mdns,
    protocol::{self as proto, kind},
};

const PHONE_ID: &str = "fake-iphone-0001";
const INSTANCE: &str = "Fake iPhone._micnexus._tcp.local";

fn name(out: &mut Vec<u8>, n: &str) {
    for label in n.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

fn record(out: &mut Vec<u8>, owner: &str, rtype: u16, rdata: &[u8]) {
    name(out, owner);
    out.extend_from_slice(&rtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&120u32.to_be_bytes());
    out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
    out.extend_from_slice(rdata);
}

fn response(id: u16, qname: &str, qtype: u16, want: &str) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&[0x84, 0x00, 0, 1, 0, 1, 0, 0, 0, 2]);
    name(&mut p, qname);
    p.extend_from_slice(&qtype.to_be_bytes());
    p.extend_from_slice(&1u16.to_be_bytes());
    let mut inst = Vec::new();
    name(&mut inst, INSTANCE);
    let mut txt = Vec::new();
    for kv in [
        "v=1".to_string(),
        format!("id={PHONE_ID}"),
        "name=Fake iPhone".into(),
        format!("want={want}"),
    ] {
        txt.push(kv.len() as u8);
        txt.extend_from_slice(kv.as_bytes());
    }
    let mut srv = vec![0, 0, 0, 0];
    srv.extend_from_slice(&proto::PHONE_PORT.to_be_bytes());
    name(&mut srv, "fake-iphone.local");
    if qtype == 12 {
        record(&mut p, qname, 12, &inst);
    } else {
        record(&mut p, INSTANCE, 16, &txt);
    }
    record(&mut p, INSTANCE, 16, &txt);
    record(&mut p, INSTANCE, 33, &srv);
    p
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let want = Arc::new(Mutex::new(String::new()));

    // Bonjour responder on 5353 (shared with the Windows mDNS service).
    let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
    sock.set_reuse_address(true).unwrap();
    sock.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, 5353)).into())
        .expect("bind 5353");
    for (ip, _) in lan::ipv4_networks() {
        let _ = sock.join_multicast_v4(&Ipv4Addr::new(224, 0, 0, 251), &ip);
    }
    let udp: std::net::UdpSocket = sock.into();
    {
        let want = want.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok((n, from)) = udp.recv_from(&mut buf) {
                let Some((q, t)) = mdns::question(&buf[..n]) else {
                    continue;
                };
                if std::env::var("MDNS_DEBUG").is_ok() {
                    println!("mdns question {q} type {t} from {from}");
                }
                let ours = (t == 12 && q.eq_ignore_ascii_case(proto::BONJOUR_SERVICE)) || (t == 16 && q == INSTANCE);
                if !ours || from.port() == 5353 {
                    continue; // only legacy unicast questions, like the PC sends
                }
                let id = u16::from_be_bytes([buf[0], buf[1]]);
                let w = want.lock().unwrap().clone();
                let _ = udp.send_to(&response(id, &q, t, &w), from);
            }
        });
    }

    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, proto::PHONE_PORT)).expect("bind tcp");
    println!("advertising {INSTANCE}, listening on {}", proto::PHONE_PORT);
    for conn in listener.incoming() {
        let Ok(mut stream) = conn else { continue };
        let mut reader = stream.try_clone().unwrap();
        let Ok(info) = proto::read_frame(&mut reader) else {
            continue;
        };
        assert_eq!(info.kind, kind::INFO);
        let pc: serde_json::Value = serde_json::from_slice(&info.payload).unwrap();
        let pc_id = pc["id"].as_str().unwrap_or_default().to_string();
        if *want.lock().unwrap() != pc_id {
            println!(
                "presence from {} ({}): BYE idle, now wanting it",
                pc["name"],
                stream.peer_addr().unwrap()
            );
            let _ = proto::write_frame(&mut stream, kind::BYE, br#"{"reason":"idle"}"#);
            *want.lock().unwrap() = pc_id;
            continue;
        }
        let asked = Instant::now();
        let hello = serde_json::json!({"v":1,"id":PHONE_ID,"name":"Fake iPhone","model":"iPhone","version":"0.1.0","codecs":["opus","pcm16"],"muted":false});
        proto::write_frame(&mut stream, kind::HELLO, &serde_json::to_vec(&hello).unwrap()).unwrap();
        loop {
            let f = proto::read_frame(&mut reader).expect("frame");
            match f.kind {
                kind::PENDING => println!("PENDING (approve on the PC)"),
                kind::WELCOME => {
                    println!(
                        "WELCOME {} after {} ms",
                        String::from_utf8_lossy(&f.payload),
                        asked.elapsed().as_millis()
                    );
                    break;
                }
                kind::BYE => return println!("BYE {}", String::from_utf8_lossy(&f.payload)),
                _ => {}
            }
        }
        std::thread::spawn(move || while proto::read_frame(&mut reader).is_ok() {});
        let (mut phase, step) = (0f32, 2.0 * std::f32::consts::PI * 440.0 / 48_000.0);
        let start = Instant::now();
        let mut next = Instant::now();
        let mut seq = 0u32;
        let mut last_ping = Instant::now() - Duration::from_secs(2);
        while start.elapsed() < Duration::from_secs(seconds) {
            if last_ping.elapsed() >= Duration::from_secs(1) {
                last_ping = Instant::now();
                let _ = proto::write_frame(
                    &mut stream,
                    kind::PING,
                    &(start.elapsed().as_millis() as u64).to_le_bytes(),
                );
            }
            let mut payload = seq.to_le_bytes().to_vec();
            for _ in 0..480 {
                payload.extend(((phase.sin() * 0.5 * 32767.0) as i16).to_le_bytes());
                phase = (phase + step) % (2.0 * std::f32::consts::PI);
            }
            stream.write_all(&proto::encode_frame(kind::AUDIO, &payload)).unwrap();
            seq += 1;
            next += Duration::from_millis(10);
            if let Some(w) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(w);
            }
        }
        let _ = proto::write_frame(&mut stream, kind::BYE, br#"{"reason":"user"}"#);
        println!("sent {seq} frames");
        return;
    }
}
