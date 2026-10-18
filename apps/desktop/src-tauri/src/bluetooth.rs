//! Bluetooth: an RFCOMM server registered in SDP under the MicNexus UUID.
//! Phones already paired with this PC connect to it directly (Bluetooth
//! traffic does not go through Windows Firewall).

use std::sync::Arc;

use crate::core::Core;

#[cfg(windows)]
pub fn start(core: Arc<Core>) {
    let _ = std::thread::Builder::new()
        .name("bluetooth".into())
        .spawn(move || win::run(core));
}

#[cfg(not(windows))]
pub fn start(core: Arc<Core>) {
    core.set_bluetooth("unsupported", None);
}

#[cfg(windows)]
pub use win::turn_on;

#[cfg(not(windows))]
pub fn turn_on() -> Result<(), String> {
    Err("Windows only".into())
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::{
        io::{self, Read, Write},
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Devices::Bluetooth::{
                AF_BTH, BLUETOOTH_FIND_RADIO_PARAMS, BTHPROTO_RFCOMM, BluetoothFindFirstRadio, BluetoothFindRadioClose,
                BluetoothIsConnectable, NS_BTH, SOCKADDR_BTH,
            },
            Foundation::{CloseHandle, HANDLE},
            Networking::WinSock::{
                CSADDR_INFO, FIONBIO, RNRSERVICE_DELETE, RNRSERVICE_REGISTER, SD_BOTH, SEND_RECV_FLAGS, SO_SNDTIMEO,
                SOCK_STREAM, SOCKADDR, SOCKET, SOCKET_ADDRESS, SOL_SOCKET, WSADATA, WSAEWOULDBLOCK, WSAGetLastError,
                WSAQUERYSETW, WSASetServiceW, WSAStartup, accept, bind, closesocket, getsockname, ioctlsocket, listen,
                recv, send, setsockopt, shutdown, socket,
            },
        },
        core::{GUID, PWSTR},
    };

    use crate::protocol::{BLUETOOTH_UUID, Transport};

    /// Closes the socket when the last clone goes away.
    struct Socket(SOCKET);

    impl Drop for Socket {
        fn drop(&mut self) {
            unsafe {
                closesocket(self.0);
            }
        }
    }

    struct BtStream {
        socket: Arc<Socket>,
        closed: Arc<AtomicBool>,
    }

    impl Read for BtStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = unsafe { recv(self.socket.0, buf, SEND_RECV_FLAGS(0)) };
            if n < 0 {
                return Err(io::Error::other(format!("recv: {:?}", unsafe { WSAGetLastError() })));
            }
            Ok(n as usize)
        }
    }

    impl Write for BtStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = unsafe { send(self.socket.0, buf, SEND_RECV_FLAGS(0)) };
            if n < 0 {
                return Err(io::Error::other(format!("send: {:?}", unsafe { WSAGetLastError() })));
            }
            Ok(n as usize)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl crate::core::Conn for BtStream {
        fn try_clone_box(&self) -> io::Result<Box<dyn crate::core::Conn>> {
            Ok(Box::new(BtStream {
                socket: self.socket.clone(),
                closed: self.closed.clone(),
            }))
        }
        fn shutdown(&self) {
            if !self.closed.swap(true, Ordering::Relaxed) {
                unsafe {
                    shutdown(self.socket.0, SD_BOTH);
                }
            }
        }
        fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()> {
            let ms = d.map(|d| d.as_millis() as u32).unwrap_or(0);
            unsafe { setsockopt(self.socket.0, SOL_SOCKET, SO_SNDTIMEO, Some(&ms.to_le_bytes())) };
            Ok(())
        }
    }

    /// The Bluetooth radio as Windows Settings shows it: Some(on) or None (no adapter).
    fn winrt_radio() -> Option<windows::Devices::Radios::Radio> {
        use windows::Devices::Radios::{Radio, RadioKind};
        let radios = Radio::GetRadiosAsync().ok()?.join().ok()?;
        (0..radios.Size().ok()?)
            .filter_map(|i| radios.GetAt(i).ok())
            .find(|r| r.Kind().ok() == Some(RadioKind::Bluetooth))
    }

    /// "ready" | "off" | "unsupported"
    fn radio_state() -> &'static str {
        use windows::Devices::Radios::RadioState;
        match winrt_radio().map(|r| r.State()) {
            None => return "unsupported",
            Some(Ok(RadioState::On)) => {}
            Some(_) => return "off",
        }
        // Win32 view of the same radio: it must accept incoming connections.
        unsafe {
            let params = BLUETOOTH_FIND_RADIO_PARAMS {
                dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
            };
            let mut radio = HANDLE::default();
            let Ok(find) = BluetoothFindFirstRadio(&params, &mut radio) else {
                return "off";
            };
            let connectable = BluetoothIsConnectable(Some(radio)).as_bool();
            let _ = CloseHandle(radio);
            let _ = BluetoothFindRadioClose(find);
            if connectable { "ready" } else { "off" }
        }
    }

    /// Turns the Bluetooth radio on (the "Ligar Bluetooth" button).
    pub fn turn_on() -> Result<(), String> {
        use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioState};
        let access = Radio::RequestAccessAsync()
            .and_then(|op| op.join())
            .map_err(|e| e.to_string())?;
        if access != RadioAccessStatus::Allowed {
            return Err("access denied".into());
        }
        let radio = winrt_radio().ok_or("no Bluetooth adapter")?;
        let status = radio
            .SetStateAsync(RadioState::On)
            .and_then(|op| op.join())
            .map_err(|e| e.to_string())?;
        if status == RadioAccessStatus::Allowed {
            Ok(())
        } else {
            Err("Windows did not allow it".into())
        }
    }

    struct Listener {
        socket: Arc<Socket>,
        sockaddr: SOCKADDR_BTH,
        name: Vec<u16>,
        class_id: GUID,
    }

    impl Listener {
        fn open(name: &str) -> Result<Self, String> {
            unsafe {
                let s =
                    socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32).map_err(|e| format!("socket: {e}"))?;
                let socket = Arc::new(Socket(s));
                let mut addr = SOCKADDR_BTH {
                    addressFamily: AF_BTH,
                    port: u32::MAX,
                    ..Default::default()
                };
                let len = size_of::<SOCKADDR_BTH>() as i32;
                if bind(s, &addr as *const _ as *const SOCKADDR, len) != 0 {
                    return Err(format!("bind: {:?}", WSAGetLastError()));
                }
                if listen(s, 2) != 0 {
                    return Err(format!("listen: {:?}", WSAGetLastError()));
                }
                let mut got = len;
                if getsockname(s, &mut addr as *mut _ as *mut SOCKADDR, &mut got) != 0 {
                    return Err(format!("getsockname: {:?}", WSAGetLastError()));
                }
                let mut nonblocking = 1u32;
                ioctlsocket(s, FIONBIO, &mut nonblocking);
                let mut l = Listener {
                    socket,
                    sockaddr: addr,
                    name: format!("MicNexus ({name})").encode_utf16().chain(Some(0)).collect(),
                    class_id: GUID::from_u128(BLUETOOTH_UUID),
                };
                l.service(true)?;
                Ok(l)
            }
        }

        /// Registers (or removes) the SDP record pointing at our RFCOMM channel.
        fn service(&mut self, register: bool) -> Result<(), String> {
            unsafe {
                let mut csaddr = CSADDR_INFO {
                    LocalAddr: SOCKET_ADDRESS {
                        lpSockaddr: &mut self.sockaddr as *mut _ as *mut SOCKADDR,
                        iSockaddrLength: size_of::<SOCKADDR_BTH>() as i32,
                    },
                    RemoteAddr: SOCKET_ADDRESS::default(),
                    iSocketType: SOCK_STREAM.0,
                    iProtocol: BTHPROTO_RFCOMM as i32,
                };
                let set = WSAQUERYSETW {
                    dwSize: size_of::<WSAQUERYSETW>() as u32,
                    lpszServiceInstanceName: PWSTR(self.name.as_mut_ptr()),
                    lpServiceClassId: &mut self.class_id,
                    dwNameSpace: NS_BTH,
                    dwNumberOfCsAddrs: 1,
                    lpcsaBuffer: &mut csaddr,
                    ..Default::default()
                };
                let op = if register {
                    RNRSERVICE_REGISTER
                } else {
                    RNRSERVICE_DELETE
                };
                if WSASetServiceW(&set, op, 0) != 0 {
                    return Err(format!("WSASetService: {:?}", WSAGetLastError()));
                }
            }
            Ok(())
        }

        /// Non-blocking accept.
        fn accept(&self) -> Result<Option<BtStream>, String> {
            unsafe {
                match accept(self.socket.0, None, None) {
                    Ok(s) => {
                        let mut blocking = 0u32;
                        ioctlsocket(s, FIONBIO, &mut blocking);
                        Ok(Some(BtStream {
                            socket: Arc::new(Socket(s)),
                            closed: Arc::default(),
                        }))
                    }
                    Err(_) if WSAGetLastError() == WSAEWOULDBLOCK => Ok(None),
                    Err(e) => Err(format!("accept: {e}")),
                }
            }
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = self.service(false);
        }
    }

    pub fn run(core: Arc<Core>) {
        let mut data = WSADATA::default();
        if unsafe { WSAStartup(0x0202, &mut data) } != 0 {
            core.set_bluetooth("error", Some("WSAStartup".into()));
            return;
        }
        let mut listener: Option<Listener> = None;
        let mut last_check = Instant::now() - Duration::from_secs(60);
        loop {
            if last_check.elapsed() >= Duration::from_secs(3) {
                last_check = Instant::now();
                let state = if core.settings().bluetooth_enabled {
                    radio_state()
                } else {
                    "off"
                };
                if state != "ready" {
                    listener = None;
                    core.set_bluetooth(state, None);
                } else if listener.is_none() {
                    match Listener::open(&core.pc_name()) {
                        Ok(l) => {
                            listener = Some(l);
                            core.set_bluetooth("ready", None);
                        }
                        Err(e) => core.set_bluetooth("error", Some(e)),
                    }
                }
            }
            let Some(l) = &listener else {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            };
            match l.accept() {
                Ok(Some(stream)) => {
                    let core = core.clone();
                    let _ = std::thread::Builder::new().name("bt-session".into()).spawn(move || {
                        core.serve(Box::new(stream), Transport::Bluetooth, "bluetooth".into(), true);
                    });
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(250)),
                Err(e) => {
                    listener = None;
                    core.set_bluetooth("error", Some(e));
                    last_check = Instant::now();
                }
            }
        }
    }
}
