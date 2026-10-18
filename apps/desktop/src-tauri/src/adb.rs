//! USB cable: talks to the adb server (port 5037) with its socket protocol,
//! forwards a local port to the phone's MicNexus port and connects through it.
//! With "open on the phone" on, plugging the cable starts the app there.
//!
//! The bundled adb.exe is only used to start the server when none is
//! running; an adb from Android Studio keeps working side by side.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    core::{Core, UsbDevice, UsbStatus},
    protocol::{self as proto, Transport},
};

const ADB_PORT: u16 = 5037;
static STARTED_SERVER: AtomicBool = AtomicBool::new(false);

fn connect() -> io::Result<TcpStream> {
    let s = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, ADB_PORT)),
        Duration::from_secs(2),
    )?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    Ok(s)
}

fn read_exact_string(s: &mut TcpStream, n: usize) -> io::Result<String> {
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn read_hex_block(s: &mut TcpStream) -> io::Result<String> {
    let len = usize::from_str_radix(&read_exact_string(s, 4)?, 16).map_err(|_| io::Error::other("bad length"))?;
    read_exact_string(s, len)
}

/// Sends one request and checks the OKAY/FAIL status.
fn request(s: &mut TcpStream, cmd: &str) -> io::Result<()> {
    s.write_all(format!("{:04x}{cmd}", cmd.len()).as_bytes())?;
    match read_exact_string(s, 4)?.as_str() {
        "OKAY" => Ok(()),
        "FAIL" => Err(io::Error::other(read_hex_block(s).unwrap_or_default())),
        other => Err(io::Error::other(format!("unexpected adb reply {other:?}"))),
    }
}

fn shell(serial: &str, cmd: &str) -> io::Result<String> {
    let mut s = connect()?;
    request(&mut s, &format!("host:transport:{serial}"))?;
    request(&mut s, &format!("shell:{cmd}"))?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

fn forward(serial: &str, local: u16, remote: u16) -> io::Result<()> {
    let mut s = connect()?;
    request(
        &mut s,
        &format!("host-serial:{serial}:forward:tcp:{local};tcp:{remote}"),
    )?;
    // Newer servers send a second OKAY once the forward is in place.
    s.set_read_timeout(Some(Duration::from_millis(500)))?;
    let _ = read_exact_string(&mut s, 4);
    Ok(())
}

fn free_local_port() -> io::Result<u16> {
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?.local_addr()?.port())
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub model: String,
}

/// Parses a `host:track-devices-l` block.
pub fn parse_devices(block: &str) -> Vec<AdbDevice> {
    block
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts
                .find_map(|p| p.strip_prefix("model:"))
                .map(|m| m.replace('_', " "))
                .unwrap_or_else(|| serial.clone());
            Some(AdbDevice { serial, state, model })
        })
        .collect()
}

pub fn find_adb(resources: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(r) = resources {
        candidates.push(r.join("adb").join("adb.exe"));
    }
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(home) = std::env::var(var) {
            candidates.push(Path::new(&home).join("platform-tools").join("adb.exe"));
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            Path::new(&local)
                .join("Android")
                .join("Sdk")
                .join("platform-tools")
                .join("adb.exe"),
        );
    }
    if let Ok(path) = std::env::var("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|p| p.join("adb.exe")));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn start_server(exe: &Path) {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("start-server")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    if cmd.status().is_ok_and(|s| s.success()) {
        STARTED_SERVER.store(true, Ordering::Relaxed);
    }
}

/// Stops the adb server on exit, but only if MicNexus started it.
pub fn shutdown() {
    if STARTED_SERVER.load(Ordering::Relaxed)
        && let Ok(mut s) = connect()
    {
        let _ = request(&mut s, "host:kill");
    }
}

/// (serial, phone id learned from its HELLO) of finished USB sessions.
type Results = Arc<Mutex<Vec<(String, Option<String>)>>>;

#[derive(Default)]
struct Phone {
    model: String,
    state: String,
    installed: Option<bool>,
    port: Option<u16>,
    launched: bool,
    device_id: Option<String>,
    next_attempt: Option<Instant>,
}

pub fn start(core: Arc<Core>, resources: Option<PathBuf>) {
    let devices: Arc<Mutex<Option<Vec<AdbDevice>>>> = Arc::default();
    {
        let (core, devices) = (core.clone(), devices.clone());
        let _ = std::thread::Builder::new()
            .name("adb-track".into())
            .spawn(move || track(core, resources, devices));
    }
    let _ = std::thread::Builder::new()
        .name("usb".into())
        .spawn(move || manage(core, devices));
}

/// Keeps `devices` in sync with the adb server (None = no server reachable).
fn track(core: Arc<Core>, resources: Option<PathBuf>, devices: Arc<Mutex<Option<Vec<AdbDevice>>>>) {
    loop {
        if !core.settings().usb_enabled {
            *devices.lock().unwrap() = Some(Vec::new());
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        let result = (|| -> io::Result<()> {
            let mut s = connect()?;
            s.set_read_timeout(None)?;
            request(&mut s, "host:track-devices-l")?;
            loop {
                let block = read_hex_block(&mut s)?;
                *devices.lock().unwrap() = Some(parse_devices(&block));
            }
        })();
        if result.is_err() {
            *devices.lock().unwrap() = None;
            match find_adb(resources.as_deref()) {
                Some(exe) => start_server(&exe),
                None => {
                    core.set_usb(UsbStatus {
                        state: "no-adb",
                        devices: Vec::new(),
                        error: None,
                    });
                    std::thread::sleep(Duration::from_secs(10));
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn manage(core: Arc<Core>, devices: Arc<Mutex<Option<Vec<AdbDevice>>>>) {
    let mut phones: HashMap<String, Phone> = HashMap::new();
    let connecting = Arc::new(AtomicBool::new(false));
    let results: Results = Arc::default();
    loop {
        std::thread::sleep(Duration::from_millis(700));
        let Some(list) = devices.lock().unwrap().clone() else {
            phones.clear();
            continue;
        };
        phones.retain(|serial, _| list.iter().any(|d| &d.serial == serial));
        for (serial, device_id) in results.lock().unwrap().drain(..) {
            if let Some(p) = phones.get_mut(&serial)
                && device_id.is_some()
            {
                p.device_id = device_id;
            }
        }
        let settings = core.settings();
        let session = core.current_session();
        for d in &list {
            let p = phones.entry(d.serial.clone()).or_default();
            p.model = d.model.clone();
            p.state = d.state.clone();
            if d.state != "device" {
                continue;
            }
            if p.installed != Some(true) {
                p.installed = shell(&d.serial, &format!("pm path {}", proto::ANDROID_PACKAGE))
                    .ok()
                    .map(|out| out.contains("package:"));
                if p.installed != Some(true) {
                    continue;
                }
            }
            if p.port.is_none() {
                p.port = free_local_port()
                    .ok()
                    .filter(|port| forward(&d.serial, *port, proto::PHONE_PORT).is_ok());
            }
            let Some(port) = p.port else { continue };
            // Connect when idle, or to move this same phone from Wi‑Fi/Bluetooth to the cable.
            let wanted = match &session {
                None => true,
                Some((id, transport)) => *transport != Transport::Usb && p.device_id.as_deref() == Some(id.as_str()),
            };
            if !wanted || connecting.load(Ordering::Relaxed) || p.next_attempt.is_some_and(|t| Instant::now() < t) {
                continue;
            }
            if settings.usb_auto_launch && !p.launched && session.is_none() {
                p.launched = true;
                let _ = shell(
                    &d.serial,
                    &format!("am start -n {}/.MainActivity --ez usb true", proto::ANDROID_PACKAGE),
                );
                p.next_attempt = Some(Instant::now() + Duration::from_millis(1500));
                continue;
            }
            p.next_attempt = Some(Instant::now() + Duration::from_secs(2));
            connecting.store(true, Ordering::Relaxed);
            let (core, connecting, results, serial) =
                (core.clone(), connecting.clone(), results.clone(), d.serial.clone());
            let _ = std::thread::Builder::new().name("usb-connect".into()).spawn(move || {
                let mut device_id = None;
                if let Ok(stream) =
                    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(2))
                {
                    let _ = stream.set_nodelay(true);
                    device_id = core.serve(Box::new(stream), Transport::Usb, serial.clone());
                }
                results.lock().unwrap().push((serial, device_id));
                connecting.store(false, Ordering::Relaxed);
            });
        }
        let mut view: Vec<UsbDevice> = phones
            .iter()
            .map(|(serial, p)| UsbDevice {
                serial: serial.clone(),
                model: p.model.clone(),
                state: if p.state == "device" && p.installed == Some(false) {
                    "no-app".into()
                } else {
                    p.state.clone()
                },
                forwarded: p.port.is_some(),
            })
            .collect();
        view.sort_by(|a, b| a.serial.cmp(&b.serial));
        core.set_usb(UsbStatus {
            state: "ready",
            devices: view,
            error: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_devices_l() {
        let block = "R5CT1234\tdevice product:dm1qxx model:SM_S911B device:dm1q transport_id:3\n\
                     emulator-5554\tunauthorized transport_id:4\n";
        let d = parse_devices(block);
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0],
            AdbDevice {
                serial: "R5CT1234".into(),
                state: "device".into(),
                model: "SM S911B".into()
            }
        );
        assert_eq!(d[1].state, "unauthorized");
        assert_eq!(d[1].model, "emulator-5554");
        assert!(parse_devices("").is_empty());
    }
}
