//! App state and phone sessions, independent of Tauri (the UI layer only
//! subscribes to [`Event`]s and calls methods), so the whole pipeline can be
//! driven from tests and `examples/fake_phone.rs`.

use std::{
    io::{self, Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::{
    audio::{AudioOut, Decoder, Meter, devices},
    protocol::{self as proto, Codec, Frame, Hello, PcInfo, Transport, kind},
    settings::{self, Settings, Store, Stored, Trusted},
};

/// A byte stream to a phone (TCP or RFCOMM).
pub trait Conn: Read + Write + Send {
    fn try_clone_box(&self) -> io::Result<Box<dyn Conn>>;
    /// Unblocks any pending read on every clone.
    fn shutdown(&self);
    fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()>;
}

impl Conn for std::net::TcpStream {
    fn try_clone_box(&self) -> io::Result<Box<dyn Conn>> {
        Ok(Box::new(self.try_clone()?))
    }
    fn shutdown(&self) {
        let _ = std::net::TcpStream::shutdown(self, std::net::Shutdown::Both);
    }
    fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        std::net::TcpStream::set_write_timeout(self, d)
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiStatus {
    pub ok: bool,
    /// This PC's IPv4 addresses.
    pub addresses: Vec<String>,
    /// Phones with MicNexus open on the network (answering discovery).
    pub phones: Vec<LanPhone>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanPhone {
    pub id: String,
    pub name: String,
    pub model: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothStatus {
    /// "ready" | "off" | "unsupported" | "error"
    pub state: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDevice {
    pub serial: String,
    pub model: String,
    /// "device" | "unauthorized" | "offline" | "no-app"
    pub state: String,
    pub forwarded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbStatus {
    /// "ready" | "no-adb" | "error"
    pub state: &'static str,
    pub devices: Vec<UsbDevice>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub device_id: String,
    pub device_name: String,
    pub model: String,
    pub transport: Transport,
    pub codec: Codec,
    pub rate: u32,
    pub muted: bool,
    pub connected_at: u64,
    pub rtt_ms: Option<u32>,
    pub buffer_ms: u32,
    pub underruns: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingView {
    pub device_id: String,
    pub device_name: String,
    pub model: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcView {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub pc: PcView,
    pub wifi: WifiStatus,
    pub bluetooth: BluetoothStatus,
    pub usb: UsbStatus,
    pub session: Option<SessionView>,
    pub pending: Option<PendingView>,
    pub output: Option<devices::Endpoint>,
    pub is_default_mic: bool,
    pub cable_install: &'static str,
}

pub enum Event {
    State(Box<Snapshot>),
    Level {
        peak: f32,
        rms: f32,
    },
    Connected {
        name: String,
        transport: Transport,
    },
    Disconnected {
        name: String,
        reason: String,
    },
    /// A new Wi‑Fi phone asks to connect: bring the window up.
    Pending {
        name: String,
    },
    /// The phone answered our connection with BYE (not now / busy).
    Declined {
        address: String,
        reason: String,
    },
}

enum Msg {
    Frame(Frame),
    Closed,
    Decision(bool),
    Send(Vec<u8>),
    Disconnect(&'static str),
}

struct Session {
    token: u64,
    device_id: String,
    device_name: String,
    model: String,
    transport: Transport,
    codec: Codec,
    rate: u32,
    connected_at: u64,
    muted: AtomicBool,
    rtt_ms: AtomicU32,
    tx: Sender<Msg>,
    audio: AudioOut,
}

struct Pending {
    view: PendingView,
    tx: Sender<Msg>,
}

struct Status {
    wifi: WifiStatus,
    bluetooth: BluetoothStatus,
    usb: UsbStatus,
    cable_install: &'static str,
    outputs: Vec<devices::Endpoint>,
    is_default_mic: bool,
}

pub struct Core {
    store: Store,
    data: Mutex<Stored>,
    version: String,
    emit: Box<dyn Fn(Event) + Send + Sync>,
    status: Mutex<Status>,
    session: Mutex<Option<Arc<Session>>>,
    pending: Mutex<Option<Pending>>,
    next_token: AtomicU64,
    /// Cables whose sample rates were already checked this run.
    aligned: Mutex<std::collections::HashSet<String>>,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(6);
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

impl Core {
    pub fn new(store: Store, version: &str, emit: impl Fn(Event) + Send + Sync + 'static) -> Arc<Self> {
        let data = store.load();
        Arc::new(Self {
            store,
            data: Mutex::new(data),
            version: version.to_string(),
            emit: Box::new(emit),
            status: Mutex::new(Status {
                wifi: WifiStatus {
                    ok: false,
                    addresses: Vec::new(),
                    phones: Vec::new(),
                    error: None,
                },
                bluetooth: BluetoothStatus {
                    state: "off",
                    error: None,
                },
                usb: UsbStatus {
                    state: "no-adb",
                    devices: Vec::new(),
                    error: None,
                },
                cable_install: "idle",
                outputs: Vec::new(),
                is_default_mic: false,
            }),
            session: Mutex::new(None),
            pending: Mutex::new(None),
            next_token: AtomicU64::new(1),
            aligned: Mutex::default(),
        })
    }

    // ------------------------------------------------------------ settings --

    pub fn settings(&self) -> Settings {
        self.data.lock().unwrap().settings.clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Settings {
        let mut data = self.data.lock().unwrap();
        f(&mut data.settings);
        self.store.save(&data);
        let s = data.settings.clone();
        drop(data);
        self.refresh_outputs();
        s
    }

    fn update_persisted(&self, f: impl FnOnce(&mut settings::Persisted)) {
        let mut data = self.data.lock().unwrap();
        f(&mut data.persisted);
        self.store.save(&data);
    }

    pub fn pc_id(&self) -> String {
        self.data.lock().unwrap().persisted.pc_id.clone()
    }

    pub fn pc_name(&self) -> String {
        let name = self.data.lock().unwrap().settings.pc_name.trim().to_string();
        if name.is_empty() {
            settings::computer_name()
        } else {
            name
        }
    }

    pub fn last_device(&self) -> Option<String> {
        self.data.lock().unwrap().persisted.last_device.clone()
    }

    pub fn pc_info(&self) -> PcInfo {
        PcInfo {
            v: proto::PROTOCOL_VERSION,
            id: self.pc_id(),
            name: self.pc_name(),
            version: self.version.clone(),
            busy: self.session.lock().unwrap().is_some(),
        }
    }

    pub fn is_trusted(&self, device_id: &str) -> bool {
        self.data
            .lock()
            .unwrap()
            .settings
            .trusted
            .iter()
            .any(|t| t.id == device_id)
    }

    pub fn forget(&self, device_id: &str) {
        self.update_settings(|s| s.trusted.retain(|t| t.id != device_id));
        self.emit_state();
    }

    // -------------------------------------------------------------- status --

    pub fn set_wifi(&self, wifi: WifiStatus) {
        let mut st = self.status.lock().unwrap();
        let changed = serde_json::to_string(&st.wifi).ok() != serde_json::to_string(&wifi).ok();
        st.wifi = wifi;
        drop(st);
        if changed {
            self.emit_state();
        }
    }

    pub fn set_bluetooth(&self, state: &'static str, error: Option<String>) {
        let mut st = self.status.lock().unwrap();
        if st.bluetooth.state == state && st.bluetooth.error == error {
            return;
        }
        st.bluetooth = BluetoothStatus { state, error };
        drop(st);
        self.emit_state();
    }

    pub fn set_usb(&self, usb: UsbStatus) {
        let mut st = self.status.lock().unwrap();
        let changed = serde_json::to_string(&st.usb).ok() != serde_json::to_string(&usb).ok();
        st.usb = usb;
        drop(st);
        if changed {
            self.emit_state();
        }
    }

    pub fn set_cable_install(&self, state: &'static str) {
        self.status.lock().unwrap().cable_install = state;
        self.emit_state();
    }

    pub fn outputs(&self) -> Vec<devices::Endpoint> {
        self.status.lock().unwrap().outputs.clone()
    }

    /// Where the phone's audio goes: the chosen endpoint, else the best cable.
    pub fn chosen_output(&self) -> Option<devices::Endpoint> {
        let wanted = self.data.lock().unwrap().settings.output_id.clone();
        let outputs = self.status.lock().unwrap().outputs.clone();
        wanted
            .and_then(|id| outputs.iter().find(|e| e.id == id).cloned())
            .or_else(|| devices::best_cable(&outputs).cloned())
    }

    fn monitor_output(&self) -> Option<String> {
        let s = self.settings();
        if !s.monitor {
            return None;
        }
        let outputs = self.outputs();
        s.monitor_id
            .and_then(|id| outputs.iter().find(|e| e.id == id).map(|e| e.id.clone()))
            .or_else(|| outputs.iter().find(|e| e.is_default).map(|e| e.id.clone()))
    }

    /// Re-reads the audio endpoints (called every few seconds and on changes).
    #[cfg(windows)]
    pub fn refresh_outputs(&self) {
        let Ok(outputs) = devices::outputs() else { return };
        // Align the cable's two sides before any app opens its microphone
        // (a format change mid-stream makes Windows restart those streams).
        if let Some(cable) = self.chosen_output_from(&outputs)
            && let Some(capture) = &cable.capture_id
            && self.aligned.lock().unwrap().insert(cable.id.clone())
            && let Err(e) = devices::align_cable_rates(&cable.id, capture)
        {
            eprintln!("cable rates: {e}");
        }
        let capture = self.chosen_output_from(&outputs).and_then(|e| e.capture_id);
        let is_default = match capture {
            Some(id) => devices::default_mics()
                .map(|d| d.console.as_deref() == Some(id.as_str()))
                .unwrap_or(false),
            None => false,
        };
        let mut st = self.status.lock().unwrap();
        let changed = serde_json::to_string(&st.outputs).ok() != serde_json::to_string(&outputs).ok()
            || st.is_default_mic != is_default;
        st.outputs = outputs;
        st.is_default_mic = is_default;
        drop(st);
        if changed {
            self.emit_state();
        }
    }

    #[cfg(not(windows))]
    pub fn refresh_outputs(&self) {}

    #[cfg(windows)]
    fn chosen_output_from(&self, outputs: &[devices::Endpoint]) -> Option<devices::Endpoint> {
        let wanted = self.data.lock().unwrap().settings.output_id.clone();
        wanted
            .and_then(|id| outputs.iter().find(|e| e.id == id).cloned())
            .or_else(|| devices::best_cable(outputs).cloned())
    }

    pub fn snapshot(&self) -> Snapshot {
        let session = self.session.lock().unwrap().as_ref().map(|s| {
            let (buffer_ms, underruns) = s.audio.stats();
            let rtt = s.rtt_ms.load(Ordering::Relaxed);
            SessionView {
                device_id: s.device_id.clone(),
                device_name: s.device_name.clone(),
                model: s.model.clone(),
                transport: s.transport,
                codec: s.codec,
                rate: s.rate,
                muted: s.muted.load(Ordering::Relaxed),
                connected_at: s.connected_at,
                rtt_ms: (rtt != u32::MAX).then_some(rtt),
                buffer_ms,
                underruns,
            }
        });
        let pending = self.pending.lock().unwrap().as_ref().map(|p| p.view.clone());
        let output = self.chosen_output();
        let st = self.status.lock().unwrap();
        Snapshot {
            pc: PcView {
                id: self.pc_id(),
                name: self.pc_name(),
                version: self.version.clone(),
            },
            wifi: st.wifi.clone(),
            bluetooth: st.bluetooth.clone(),
            usb: st.usb.clone(),
            session,
            pending,
            output,
            is_default_mic: st.is_default_mic,
            cable_install: st.cable_install,
        }
    }

    pub fn emit_state(&self) {
        (self.emit)(Event::State(Box::new(self.snapshot())));
    }

    // ------------------------------------------------------------- actions --

    pub fn is_connected(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }

    /// Id of the connected phone, if any.
    pub fn current_device(&self) -> Option<String> {
        self.session.lock().unwrap().as_ref().map(|s| s.device_id.clone())
    }

    /// (phone id, transport) of the current session.
    pub fn current_session(&self) -> Option<(String, Transport)> {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| (s.device_id.clone(), s.transport))
    }

    pub fn is_muted(&self) -> bool {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.muted.load(Ordering::Relaxed))
    }

    /// Mute from the PC: silences at once and asks the phone to follow.
    pub fn set_muted(&self, muted: bool) {
        let Some(s) = self.session.lock().unwrap().clone() else {
            return;
        };
        s.muted.store(muted, Ordering::Relaxed);
        if muted {
            s.audio.flush();
            (self.emit)(Event::Level { peak: 0.0, rms: 0.0 });
        }
        let _ =
            s.tx.send(Msg::Send(proto::encode_frame(kind::MUTE, &[u8::from(muted)])));
        self.emit_state();
    }

    pub fn toggle_mute(&self) {
        self.set_muted(!self.is_muted());
    }

    pub fn disconnect(&self) {
        if let Some(s) = self.session.lock().unwrap().as_ref() {
            let _ = s.tx.send(Msg::Disconnect("user"));
        }
    }

    pub fn decide(&self, accept: bool) {
        if let Some(p) = self.pending.lock().unwrap().as_ref() {
            let _ = p.tx.send(Msg::Decision(accept));
        }
    }

    /// Puts the default microphone back if a crash left the cable as default.
    pub fn restore_default_mic_after_crash(&self) {
        if self.session.lock().unwrap().is_none() {
            self.restore_default_mic();
        }
    }

    // ------------------------------------------------------- default mic --

    /// The "use as default microphone" toggle or the output changed mid-session.
    pub fn default_mic_setting_changed(&self) {
        if self.session.lock().unwrap().is_none() {
            return;
        }
        self.restore_default_mic();
        self.switch_default_mic();
    }

    #[cfg(windows)]
    fn switch_default_mic(&self) {
        if !self.settings().set_default_mic {
            return;
        }
        let Some(capture) = self.chosen_output().and_then(|e| e.capture_id) else {
            return;
        };
        let Ok(current) = devices::default_mics() else { return };
        if current.console.as_deref() == Some(capture.as_str())
            && current.communications.as_deref() == Some(capture.as_str())
        {
            return; // already the default (the user's choice; nothing to restore)
        }
        let needs_save = self.data.lock().unwrap().persisted.restore_mics.is_none();
        if needs_save {
            self.update_persisted(|p| p.restore_mics = Some(current));
        }
        if let Err(e) = devices::set_default_mic(&capture) {
            eprintln!("default mic: {e}");
        }
        self.refresh_outputs();
    }

    #[cfg(windows)]
    fn restore_default_mic(&self) {
        let Some(previous) = self.data.lock().unwrap().persisted.restore_mics.clone() else {
            return;
        };
        let ours = self.chosen_output().and_then(|e| e.capture_id);
        let current = devices::default_mics().ok();
        // Only undo our own change: if the user picked another mic meanwhile, keep it.
        let still_ours = match (&ours, &current) {
            (Some(ours), Some(cur)) => cur.console.as_deref() == Some(ours.as_str()),
            _ => true,
        };
        if still_ours && let Err(e) = devices::restore_default_mics(&previous) {
            eprintln!("restore default mic: {e}");
        }
        self.update_persisted(|p| p.restore_mics = None);
        self.refresh_outputs();
    }

    #[cfg(not(windows))]
    fn switch_default_mic(&self) {}
    #[cfg(not(windows))]
    fn restore_default_mic(&self) {}

    // ------------------------------------------------------------ sessions --

    /// Serves one phone connection until it ends (blocks; run on its own
    /// thread). Returns the phone's id when it introduced itself.
    pub fn serve(self: &Arc<Self>, mut conn: Box<dyn Conn>, transport: Transport, address: String) -> Option<String> {
        let _ = conn.set_write_timeout(Some(Duration::from_secs(5)));
        let (tx, rx) = mpsc::channel::<Msg>();
        let reader = match conn.try_clone_box() {
            Ok(mut r) => {
                let tx = tx.clone();
                std::thread::Builder::new()
                    .name("phone-reader".into())
                    .spawn(move || {
                        loop {
                            match proto::read_frame(&mut r) {
                                Ok(f) => {
                                    if tx.send(Msg::Frame(f)).is_err() {
                                        break;
                                    }
                                }
                                Err(_) => {
                                    let _ = tx.send(Msg::Closed);
                                    break;
                                }
                            }
                        }
                    })
                    .ok()
            }
            Err(_) => None,
        };
        let device_id = match reader {
            Some(_) => self.handshake(&mut conn, transport, &address, tx, &rx),
            None => None,
        };
        conn.shutdown();
        if let Some(r) = reader {
            let _ = r.join();
        }
        device_id
    }

    fn handshake(
        self: &Arc<Self>,
        conn: &mut Box<dyn Conn>,
        transport: Transport,
        address: &str,
        tx: Sender<Msg>,
        rx: &mpsc::Receiver<Msg>,
    ) -> Option<String> {
        // The PC always speaks first; the phone answers HELLO, or BYE when it
        // does not want this PC (docs/PROTOCOL.md).
        conn.write_all(&proto::json_frame(kind::INFO, &self.pc_info())).ok()?;
        let first = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Msg::Frame(f)) => f,
            _ => return None,
        };
        if first.kind != kind::HELLO {
            if first.kind == kind::BYE {
                let reason = serde_json::from_slice::<proto::Bye>(&first.payload)
                    .map(|b| b.reason)
                    .unwrap_or_default();
                (self.emit)(Event::Declined {
                    address: address.to_string(),
                    reason,
                });
            }
            return None;
        }
        let hello = serde_json::from_slice::<Hello>(&first.payload).ok()?;
        let id = Some(hello.id.clone());
        let bye = |conn: &mut Box<dyn Conn>, reason: &str| {
            let _ = conn.write_all(&proto::json_frame(kind::BYE, &proto::Bye { reason: reason.into() }));
            id.clone()
        };
        if hello.v != proto::PROTOCOL_VERSION {
            return bye(conn, "version");
        }
        let busy_with_other = self
            .session
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.device_id != hello.id);
        if busy_with_other {
            return bye(conn, "busy");
        }

        // Cable and Bluetooth imply physical access/pairing; Wi‑Fi asks once.
        if transport == Transport::Wifi && !self.is_trusted(&hello.id) {
            {
                let mut pending = self.pending.lock().unwrap();
                if pending.is_some() {
                    drop(pending);
                    return bye(conn, "busy");
                }
                *pending = Some(Pending {
                    view: PendingView {
                        device_id: hello.id.clone(),
                        device_name: hello.name.clone(),
                        model: hello.model.clone(),
                        address: address.to_string(),
                    },
                    tx: tx.clone(),
                });
            }
            let _ = conn.write_all(&proto::encode_frame(kind::PENDING, &[]));
            self.emit_state();
            (self.emit)(Event::Pending {
                name: hello.name.clone(),
            });
            let deadline = Instant::now() + APPROVAL_TIMEOUT;
            let accepted = loop {
                match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(Msg::Decision(a)) => break Some(a),
                    Ok(Msg::Frame(f)) if f.kind == kind::BYE => break None,
                    Ok(Msg::Frame(_)) => continue,
                    Ok(Msg::Closed) | Err(_) => break None,
                    Ok(_) => continue,
                }
            };
            *self.pending.lock().unwrap() = None;
            self.emit_state();
            match accepted {
                Some(true) => {
                    let (id, name) = (hello.id.clone(), hello.name.clone());
                    self.update_settings(|s| {
                        s.trusted.retain(|t| t.id != id);
                        s.trusted.push(Trusted {
                            id,
                            name,
                            added_at: now_ms(),
                        });
                    });
                }
                Some(false) => return bye(conn, "rejected"),
                None => return id,
            }
        }

        let (codec, rate, frame_ms) = proto::choose_codec(transport, &hello.codecs);
        let welcome = proto::Welcome {
            v: proto::PROTOCOL_VERSION,
            id: self.pc_id(),
            name: self.pc_name(),
            version: self.version.clone(),
            codec,
            rate,
            frame_ms,
        };
        if conn.write_all(&proto::json_frame(kind::WELCOME, &welcome)).is_err() {
            return id;
        }
        let Ok(mut decoder) = Decoder::new(codec, rate) else {
            return id;
        };
        let target_ms = if transport == Transport::Bluetooth { 60 } else { 30 };
        let session = Arc::new(Session {
            token: self.next_token.fetch_add(1, Ordering::Relaxed),
            device_id: hello.id.clone(),
            device_name: hello.name.clone(),
            model: hello.model.clone(),
            transport,
            codec,
            rate,
            connected_at: now_ms(),
            muted: AtomicBool::new(hello.muted),
            rtt_ms: AtomicU32::new(u32::MAX),
            tx,
            audio: AudioOut::new(target_ms),
        });
        let replaced = self.session.lock().unwrap().replace(session.clone());
        if let Some(old) = replaced {
            let _ = old.tx.send(Msg::Disconnect("replaced"));
        }
        let chosen = self.chosen_output();
        #[cfg(windows)]
        if let Some(out) = &chosen
            && let Some(capture) = &out.capture_id
            && let Err(e) = devices::align_cable_rates(&out.id, capture)
        {
            eprintln!("cable rates: {e}");
        }
        let main_out = chosen.map(|e| e.id);
        if let Err(e) = session
            .audio
            .ensure(main_out.as_deref(), self.monitor_output().as_deref())
        {
            eprintln!("audio output: {e}");
        }
        self.switch_default_mic();
        self.update_persisted(|p| p.last_device = Some(hello.id.clone()));
        (self.emit)(Event::Connected {
            name: hello.name.clone(),
            transport,
        });
        self.emit_state();

        let reason = self.run_session(conn, &session, rx, &mut decoder);

        let still_current = {
            let mut current = self.session.lock().unwrap();
            if current.as_ref().is_some_and(|s| s.token == session.token) {
                *current = None;
                true
            } else {
                false
            }
        };
        if still_current {
            self.restore_default_mic();
            (self.emit)(Event::Level { peak: 0.0, rms: 0.0 });
            (self.emit)(Event::Disconnected {
                name: session.device_name.clone(),
                reason: reason.to_string(),
            });
        }
        self.emit_state();
        id
    }

    /// Returns why the session ended.
    fn run_session(
        &self,
        conn: &mut Box<dyn Conn>,
        s: &Session,
        rx: &mpsc::Receiver<Msg>,
        decoder: &mut Decoder,
    ) -> &'static str {
        let mut last_rx = Instant::now();
        let mut last_tick = Instant::now();
        let mut last_seq: Option<u32> = None;
        let mut meter = Meter::default();
        let mut samples = Vec::with_capacity(5760);
        let mut last_buffer = 0u32;
        loop {
            let msg = rx.recv_timeout(Duration::from_millis(250));
            match msg {
                Ok(Msg::Frame(f)) => {
                    last_rx = Instant::now();
                    match f.kind {
                        kind::AUDIO if f.payload.len() >= 4 => {
                            if s.muted.load(Ordering::Relaxed) {
                                continue;
                            }
                            let seq = u32::from_le_bytes([f.payload[0], f.payload[1], f.payload[2], f.payload[3]]);
                            samples.clear();
                            // Conceal at most 3 missing Opus frames (the phone dropped them).
                            if let Some(prev) = last_seq
                                && s.codec == Codec::Opus
                            {
                                for _ in 0..seq.wrapping_sub(prev).saturating_sub(1).min(3) {
                                    decoder.decode(&[], &mut samples);
                                }
                            }
                            last_seq = Some(seq);
                            decoder.decode(&f.payload[4..], &mut samples);
                            s.audio.push(&samples);
                            if let Some((peak, rms)) = meter.feed(&samples) {
                                (self.emit)(Event::Level { peak, rms });
                            }
                        }
                        kind::MUTE => {
                            let muted = f.payload.first() == Some(&1);
                            let was = s.muted.swap(muted, Ordering::Relaxed);
                            if muted {
                                s.audio.flush();
                                (self.emit)(Event::Level { peak: 0.0, rms: 0.0 });
                            }
                            if was != muted {
                                self.emit_state();
                            }
                        }
                        kind::PING if f.payload.len() >= 8 => {
                            if f.payload.len() >= 12 {
                                let rtt =
                                    u32::from_le_bytes([f.payload[8], f.payload[9], f.payload[10], f.payload[11]]);
                                s.rtt_ms.store(rtt, Ordering::Relaxed);
                            }
                            if conn
                                .write_all(&proto::encode_frame(kind::PONG, &f.payload[..8]))
                                .is_err()
                            {
                                return "network";
                            }
                        }
                        kind::BYE => return "phone",
                        _ => {}
                    }
                }
                Ok(Msg::Send(bytes)) => {
                    if conn.write_all(&bytes).is_err() {
                        return "network";
                    }
                }
                Ok(Msg::Disconnect(reason)) => {
                    let _ = conn.write_all(&proto::json_frame(kind::BYE, &proto::Bye { reason: reason.into() }));
                    return reason;
                }
                Ok(Msg::Closed) => return "network",
                Ok(Msg::Decision(_)) => {}
                Err(RecvTimeoutError::Disconnected) => return "network",
                Err(RecvTimeoutError::Timeout) => {}
            }
            if last_rx.elapsed() > IDLE_TIMEOUT {
                return "timeout";
            }
            if last_tick.elapsed() >= Duration::from_secs(1) {
                last_tick = Instant::now();
                let (buffer_ms, underruns) = s.audio.stats();
                let stats = proto::Stats { buffer_ms, underruns };
                if conn.write_all(&proto::json_frame(kind::STATS, &stats)).is_err() {
                    return "network";
                }
                // Follow device changes (cable installed, output picked, device unplugged).
                let main_out = self.chosen_output().map(|e| e.id);
                let _ = s.audio.ensure(main_out.as_deref(), self.monitor_output().as_deref());
                if buffer_ms.abs_diff(last_buffer) >= 5 {
                    last_buffer = buffer_ms;
                    self.emit_state();
                }
            }
        }
    }
}
