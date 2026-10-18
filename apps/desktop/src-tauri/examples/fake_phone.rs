//! Pretends to be the Android app on the Wi‑Fi path: answers discovery
//! wanting the first PC it hears, accepts the PC's connection, sends HELLO
//! and streams a 440 Hz tone (PCM 48 kHz, 10 ms frames) with pings.
//!
//!   cargo run --example fake_phone -- [seconds] [mute-at-second]
//!
//! Prints the PC's INFO/WELCOME/STATS and the measured RTT.

use std::{
    io::Write,
    net::{Ipv4Addr, TcpListener, UdpSocket},
    time::{Duration, Instant},
};

use micnexus_desktop_lib::protocol::{self as proto, kind};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let mute_at: Option<u64> = args.get(2).and_then(|s| s.parse().ok());
    let phone_id = "fake-phone-0001";

    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, proto::PHONE_PORT)).expect("bind tcp");
    let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, proto::DISCOVERY_PORT)).expect("bind udp");
    println!("listening on tcp {} / udp {}", proto::PHONE_PORT, proto::DISCOVERY_PORT);

    // Discovery: wait for a PC and answer "I want you".
    let mut buf = [0u8; 2048];
    let pc_id = loop {
        let (n, from) = udp.recv_from(&mut buf).expect("recv");
        let Some(body) = buf[..n].strip_prefix(proto::DISCOVER_REQUEST.as_slice()) else {
            continue;
        };
        let info: serde_json::Value = serde_json::from_slice(body).expect("json");
        let pc_id = info["id"].as_str().unwrap_or_default().to_string();
        println!("discovery from {from}: {info}");
        let mut reply = proto::DISCOVER_REPLY.to_vec();
        reply.extend(
            serde_json::to_vec(&serde_json::json!({
                "v": 1, "id": phone_id, "name": "Fake Phone", "model": "Test", "port": proto::PHONE_PORT, "want": pc_id
            }))
            .unwrap(),
        );
        udp.send_to(&reply, from).expect("reply");
        break pc_id;
    };
    // Keep answering while we wait for the TCP connection.
    let udp2 = udp.try_clone().unwrap();
    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = udp2.recv_from(&mut buf) {
            if buf[..n].starts_with(proto::DISCOVER_REQUEST) {
                let mut reply = proto::DISCOVER_REPLY.to_vec();
                reply.extend(
                    serde_json::to_vec(&serde_json::json!({
                        "v": 1, "id": phone_id, "name": "Fake Phone", "model": "Test", "port": proto::PHONE_PORT, "want": pc_id
                    }))
                    .unwrap(),
                );
                let _ = udp2.send_to(&reply, from);
            }
        }
    });

    let (mut stream, from) = listener.accept().expect("accept");
    let connected_at = Instant::now();
    println!("PC connected from {from}");
    stream.set_nodelay(true).unwrap();
    let mut reader = stream.try_clone().unwrap();
    let info = proto::read_frame(&mut reader).expect("info");
    assert_eq!(info.kind, kind::INFO, "first frame must be INFO");
    println!("INFO {}", String::from_utf8_lossy(&info.payload));
    let hello = serde_json::json!({
        "v": 1, "id": phone_id, "name": "Fake Phone", "model": "Test", "version": "0.1.0",
        "codecs": ["pcm16"], "muted": false
    });
    proto::write_frame(&mut stream, kind::HELLO, &serde_json::to_vec(&hello).unwrap()).unwrap();
    loop {
        let f = proto::read_frame(&mut reader).expect("welcome");
        match f.kind {
            kind::PENDING => println!("PENDING (approve on the PC)"),
            kind::WELCOME => {
                println!(
                    "WELCOME {} after {} ms",
                    String::from_utf8_lossy(&f.payload),
                    connected_at.elapsed().as_millis()
                );
                break;
            }
            kind::BYE => {
                println!("BYE {}", String::from_utf8_lossy(&f.payload));
                return;
            }
            other => println!("frame {other}"),
        }
    }

    // Reader: PONG → RTT, STATS, MUTE requests.
    let start = Instant::now();
    std::thread::spawn(move || {
        while let Ok(f) = proto::read_frame(&mut reader) {
            match f.kind {
                kind::PONG => {
                    let sent = u64::from_le_bytes(f.payload[..8].try_into().unwrap());
                    let now = start.elapsed().as_millis() as u64;
                    println!("RTT {} ms", now - sent);
                }
                kind::STATS => println!("STATS {}", String::from_utf8_lossy(&f.payload)),
                kind::MUTE => println!("PC asks MUTE={}", f.payload[0]),
                kind::BYE => {
                    println!("BYE {}", String::from_utf8_lossy(&f.payload));
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    });

    let mut seq = 0u32;
    let mut phase = 0f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / 48_000.0;
    let mut muted = false;
    let t0 = Instant::now();
    let mut next = Instant::now();
    let mut last_ping = Instant::now() - Duration::from_secs(2);
    while t0.elapsed() < Duration::from_secs(seconds) {
        if let Some(m) = mute_at
            && !muted
            && t0.elapsed() >= Duration::from_secs(m)
        {
            muted = true;
            proto::write_frame(&mut stream, kind::MUTE, &[1]).unwrap();
            println!("muted");
        }
        if last_ping.elapsed() >= Duration::from_secs(1) {
            last_ping = Instant::now();
            let mut p = (start.elapsed().as_millis() as u64).to_le_bytes().to_vec();
            p.extend(12u32.to_le_bytes());
            proto::write_frame(&mut stream, kind::PING, &p).unwrap();
        }
        if !muted {
            let mut payload = seq.to_le_bytes().to_vec();
            for _ in 0..480 {
                let s = (phase.sin() * 0.5 * 32767.0) as i16;
                payload.extend(s.to_le_bytes());
                phase = (phase + step) % (2.0 * std::f32::consts::PI);
            }
            stream.write_all(&proto::encode_frame(kind::AUDIO, &payload)).unwrap();
            seq += 1;
        }
        next += Duration::from_millis(10);
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    let _ = proto::write_frame(&mut stream, kind::BYE, br#"{"reason":"user"}"#);
    println!("sent {seq} frames");
}
