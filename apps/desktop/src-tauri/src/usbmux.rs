//! USB cable for iPhones: the Apple Mobile Device Service (installed with the
//! "Apple Devices" app or iTunes — Sideloadly needs it too) runs usbmuxd on
//! 127.0.0.1:27015. Its `Connect` request turns the socket into a tunnel to a
//! TCP port on the iPhone, where MicNexus listens while it is open.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use plist::{Dictionary, Value};

use crate::{
    core::{Core, UsbDevice},
    protocol::{self as proto, Transport},
};

pub const USBMUXD_PORT: u16 = 27015;
const PLIST_MESSAGE: u32 = 8;

fn request(kind: &str) -> Dictionary {
    let mut d = Dictionary::new();
    d.insert("MessageType".into(), kind.into());
    d.insert("ClientVersionString".into(), "micnexus".into());
    d.insert("ProgName".into(), "MicNexus".into());
    d.insert("kLibUSBMuxVersion".into(), 3.into());
    d
}

pub fn encode(tag: u32, body: &Dictionary) -> Vec<u8> {
    let mut xml = Vec::new();
    plist::to_writer_xml(&mut xml, body).expect("plist");
    let mut out = Vec::with_capacity(16 + xml.len());
    out.extend_from_slice(&((16 + xml.len()) as u32).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // version: plist
    out.extend_from_slice(&PLIST_MESSAGE.to_le_bytes());
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&xml);
    out
}

pub fn read_message(r: &mut impl Read) -> io::Result<Dictionary> {
    let mut head = [0u8; 16];
    r.read_exact(&mut head)?;
    let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
    if !(16..=1 << 20).contains(&len) {
        return Err(io::Error::other("bad usbmux length"));
    }
    let mut body = vec![0u8; len - 16];
    r.read_exact(&mut body)?;
    plist::from_bytes::<Dictionary>(&body).map_err(io::Error::other)
}

fn connect_daemon() -> io::Result<TcpStream> {
    let s = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, USBMUXD_PORT)),
        Duration::from_secs(1),
    )?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    Ok(s)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppleDevice {
    pub id: u64,
    pub udid: String,
}

/// iPhones attached by USB (network-paired ones are ignored).
pub fn list_devices() -> io::Result<Vec<AppleDevice>> {
    let mut s = connect_daemon()?;
    s.write_all(&encode(1, &request("ListDevices")))?;
    Ok(parse_device_list(&read_message(&mut s)?))
}

pub fn parse_device_list(reply: &Dictionary) -> Vec<AppleDevice> {
    let Some(list) = reply.get("DeviceList").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|d| {
            let d = d.as_dictionary()?;
            let props = d.get("Properties")?.as_dictionary()?;
            if props.get("ConnectionType").and_then(Value::as_string) != Some("USB") {
                return None;
            }
            let id = d
                .get("DeviceID")
                .or_else(|| props.get("DeviceID"))
                .and_then(Value::as_unsigned_integer)?;
            let udid = props
                .get("SerialNumber")
                .and_then(Value::as_string)
                .unwrap_or_default()
                .to_string();
            Some(AppleDevice { id, udid })
        })
        .collect()
}

pub enum ConnectError {
    /// Nothing listens on the iPhone (MicNexus is not open).
    Refused,
    Other(io::Error),
}

/// Opens a tunnel to `port` on the iPhone.
pub fn connect(device: u64, port: u16) -> Result<TcpStream, ConnectError> {
    let mut s = connect_daemon().map_err(ConnectError::Other)?;
    let mut req = request("Connect");
    req.insert("DeviceID".into(), device.into());
    // usbmuxd wants the port in network byte order inside the integer.
    req.insert("PortNumber".into(), u64::from(port.swap_bytes()).into());
    s.write_all(&encode(2, &req)).map_err(ConnectError::Other)?;
    let reply = read_message(&mut s).map_err(ConnectError::Other)?;
    match reply.get("Number").and_then(Value::as_unsigned_integer) {
        Some(0) => {
            let _ = s.set_read_timeout(None);
            Ok(s)
        }
        Some(3) => Err(ConnectError::Refused),
        other => Err(ConnectError::Other(io::Error::other(format!(
            "usbmux result {other:?}"
        )))),
    }
}

/// (usbmux device, phone id from its HELLO) of finished sessions.
type Results = Arc<Mutex<Vec<(u64, Option<String>)>>>;

#[derive(Default)]
struct Phone {
    device_id: Option<String>,
    open: bool,
    next_attempt: Option<Instant>,
}

pub fn start(core: Arc<Core>) {
    let _ = std::thread::Builder::new()
        .name("usbmux".into())
        .spawn(move || run(core));
}

fn run(core: Arc<Core>) {
    let mut phones: HashMap<u64, Phone> = HashMap::new();
    let connecting = Arc::new(AtomicBool::new(false));
    let results: Results = Arc::default();
    loop {
        std::thread::sleep(Duration::from_millis(1500));
        if !core.settings().usb_enabled {
            phones.clear();
            core.set_apple_usb(false, Vec::new());
            continue;
        }
        let devices = match list_devices() {
            Ok(d) => d,
            Err(_) => {
                phones.clear();
                core.set_apple_usb(false, Vec::new());
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        phones.retain(|id, _| devices.iter().any(|d| d.id == *id));
        for (id, device_id) in results.lock().unwrap().drain(..) {
            if let (Some(p), Some(d)) = (phones.get_mut(&id), device_id) {
                p.device_id = Some(d);
            }
        }
        let session = core.current_session();
        for d in &devices {
            let p = phones.entry(d.id).or_default();
            // Idle, or this same iPhone on Wi‑Fi/BLE: move it to the cable.
            let wanted = match &session {
                None => true,
                Some((id, t)) => *t != Transport::Usb && p.device_id.as_deref() == Some(id.as_str()),
            };
            if !wanted || connecting.load(Ordering::Relaxed) || p.next_attempt.is_some_and(|t| Instant::now() < t) {
                if matches!(&session, Some((id, Transport::Usb)) if p.device_id.as_deref() == Some(id.as_str())) {
                    p.open = true;
                }
                continue;
            }
            p.next_attempt = Some(Instant::now() + Duration::from_secs(2));
            match connect(d.id, proto::PHONE_PORT) {
                Ok(stream) => {
                    p.open = true;
                    let _ = stream.set_nodelay(true);
                    connecting.store(true, Ordering::Relaxed);
                    let (core, connecting, results, id) = (core.clone(), connecting.clone(), results.clone(), d.id);
                    let _ = std::thread::Builder::new()
                        .name("usbmux-session".into())
                        .spawn(move || {
                            let device_id = core.serve(Box::new(stream), Transport::Usb, "iPhone (USB)".into(), true);
                            results.lock().unwrap().push((id, device_id));
                            connecting.store(false, Ordering::Relaxed);
                        });
                }
                Err(ConnectError::Refused) => p.open = false,
                Err(ConnectError::Other(_)) => {}
            }
        }
        let mut view: Vec<UsbDevice> = devices
            .iter()
            .map(|d| UsbDevice {
                serial: d.udid.chars().take(8).collect(),
                model: "iPhone".into(),
                state: if phones.get(&d.id).is_some_and(|p| p.open) {
                    "device".into()
                } else {
                    "closed".into()
                },
                forwarded: phones.get(&d.id).is_some_and(|p| p.open),
            })
            .collect();
        view.sort_by(|a, b| a.serial.cmp(&b.serial));
        core.set_apple_usb(true, view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn frames_round_trip() {
        let bytes = encode(7, &request("ListDevices"));
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize,
            bytes.len()
        );
        assert_eq!(&bytes[4..16], &[1, 0, 0, 0, 8, 0, 0, 0, 7, 0, 0, 0]);
        let d = read_message(&mut &bytes[..]).unwrap();
        assert_eq!(d.get("MessageType").and_then(Value::as_string), Some("ListDevices"));
    }

    #[test]
    fn device_list_keeps_usb_iphones() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>DeviceList</key><array>
<dict><key>DeviceID</key><integer>3</integer><key>MessageType</key><string>Attached</string>
<key>Properties</key><dict><key>ConnectionType</key><string>USB</string><key>DeviceID</key><integer>3</integer>
<key>SerialNumber</key><string>00008110-001A2B3C4D5E</string></dict></dict>
<dict><key>DeviceID</key><integer>9</integer><key>Properties</key><dict><key>ConnectionType</key><string>Network</string></dict></dict>
</array></dict></plist>"#;
        let d: Dictionary = plist::from_bytes(xml).unwrap();
        assert_eq!(
            parse_device_list(&d),
            vec![AppleDevice {
                id: 3,
                udid: "00008110-001A2B3C4D5E".into()
            }]
        );
    }

    /// A scripted usbmuxd: answers Connect with Result 0, then echoes like the
    /// tunnel would; checks the byte-swapped port.
    #[test]
    fn connect_against_a_fake_daemon() {
        let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, USBMUXD_PORT)) else {
            return; // a real usbmuxd is running here: nothing to fake
        };
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let req = read_message(&mut s).unwrap();
            assert_eq!(req.get("MessageType").and_then(Value::as_string), Some("Connect"));
            assert_eq!(req.get("PortNumber").and_then(Value::as_unsigned_integer), Some(0xCCBA)); // 47820 = 0xBACC
            let mut ok = Dictionary::new();
            ok.insert("MessageType".into(), "Result".into());
            ok.insert("Number".into(), 0.into());
            s.write_all(&encode(2, &ok)).unwrap();
            let mut b = [0u8; 4];
            s.read_exact(&mut b).unwrap();
            s.write_all(&b).unwrap();
        });
        let Ok(mut tunnel) = connect(5, proto::PHONE_PORT) else {
            panic!("connect")
        };
        tunnel.write_all(b"ping").unwrap();
        let mut b = [0u8; 4];
        tunnel.read_exact(&mut b).unwrap();
        assert_eq!(&b, b"ping");
        server.join().unwrap();
    }
}
