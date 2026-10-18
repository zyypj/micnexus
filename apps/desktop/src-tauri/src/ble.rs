//! Bluetooth for iPhones: iOS has no classic Bluetooth (RFCOMM) for apps, so
//! the iPhone is a BLE GATT peripheral and the PC is the central. The two
//! characteristics carry the same byte stream as TCP (docs/PROTOCOL.md):
//! `rx` (we write without response) and `tx` (the phone notifies).
//!
//! The PC connects when the iPhone's advertised name is `MNX*` (the user
//! tapped "Bluetooth": any PC) or `MNX` + the start of our PC id.

use std::sync::Arc;

use crate::core::Core;

/// Whether an advertised local name asks for this PC.
pub fn wants_us(local_name: &str, pc_id: &str) -> bool {
    let Some(rest) = local_name.strip_prefix("MNX") else {
        return false;
    };
    rest == "*" || (!rest.is_empty() && pc_id.starts_with(rest))
}

#[cfg(windows)]
pub fn start(core: Arc<Core>) {
    let _ = std::thread::Builder::new()
        .name("ble".into())
        .spawn(move || win::run(core));
}

#[cfg(not(windows))]
pub fn start(_core: Arc<Core>) {}

#[cfg(windows)]
mod win {
    use super::*;
    use std::{
        collections::{HashMap, VecDeque},
        io::{self, Read, Write},
        sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, Sender},
        },
        time::{Duration, Instant},
    };
    use windows::{
        Devices::Bluetooth::{
            Advertisement::{
                BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher, BluetoothLEScanningMode,
            },
            BluetoothConnectionStatus, BluetoothLEDevice,
            GenericAttributeProfile::{
                GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue, GattCommunicationStatus,
                GattSession, GattValueChangedEventArgs, GattWriteOption,
            },
        },
        Foundation::TypedEventHandler,
        Storage::Streams::{DataReader, DataWriter},
        core::GUID,
    };

    use crate::protocol::{BLE_RX, BLE_SERVICE, BLE_TX, Transport};

    enum Chunk {
        Data(Vec<u8>),
        Closed,
    }

    struct Inner {
        device: BluetoothLEDevice,
        rx: GattCharacteristic,
        /// Max bytes per write (ATT MTU − 3).
        chunk: usize,
        incoming: Mutex<(Receiver<Chunk>, VecDeque<u8>)>,
        closer: Sender<Chunk>,
        closed: AtomicBool,
    }

    struct BleConn(Arc<Inner>);

    impl Read for BleConn {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut guard = self.0.incoming.lock().unwrap();
            let (rx, pending) = &mut *guard;
            while pending.is_empty() {
                match rx.recv() {
                    Ok(Chunk::Data(d)) => pending.extend(d),
                    Ok(Chunk::Closed) | Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(pending.len());
            for (b, v) in buf.iter_mut().zip(pending.drain(..n)) {
                *b = v;
            }
            Ok(n)
        }
    }

    impl Write for BleConn {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.0.closed.load(Ordering::Relaxed) {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            for part in buf.chunks(self.0.chunk.max(20)) {
                let w = DataWriter::new().map_err(io::Error::other)?;
                w.WriteBytes(part).map_err(io::Error::other)?;
                let buffer = w.DetachBuffer().map_err(io::Error::other)?;
                let status = self
                    .0
                    .rx
                    .WriteValueWithOptionAsync(&buffer, GattWriteOption::WriteWithoutResponse)
                    .and_then(|op| op.join())
                    .map_err(io::Error::other)?;
                if status != GattCommunicationStatus::Success {
                    return Err(io::Error::other("BLE write failed"));
                }
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl crate::core::Conn for BleConn {
        fn try_clone_box(&self) -> io::Result<Box<dyn crate::core::Conn>> {
            Ok(Box::new(BleConn(self.0.clone())))
        }
        fn shutdown(&self) {
            if !self.0.closed.swap(true, Ordering::Relaxed) {
                let _ = self.0.closer.send(Chunk::Closed);
                let _ = self.0.device.Close();
            }
        }
        fn set_write_timeout(&self, _d: Option<Duration>) -> io::Result<()> {
            Ok(())
        }
    }

    fn buffer_bytes(buf: &windows::Storage::Streams::IBuffer) -> windows::core::Result<Vec<u8>> {
        let reader = DataReader::FromBuffer(buf)?;
        let mut out = vec![0u8; reader.UnconsumedBufferLength()? as usize];
        reader.ReadBytes(&mut out)?;
        Ok(out)
    }

    fn open(address: u64) -> windows::core::Result<BleConn> {
        let device = BluetoothLEDevice::FromBluetoothAddressAsync(address)?.join()?;
        let services = device
            .GetGattServicesForUuidAsync(GUID::from_u128(BLE_SERVICE))?
            .join()?
            .Services()?;
        let service = services.GetAt(0)?;
        let rx = service
            .GetCharacteristicsForUuidAsync(GUID::from_u128(BLE_RX))?
            .join()?
            .Characteristics()?
            .GetAt(0)?;
        let tx = service
            .GetCharacteristicsForUuidAsync(GUID::from_u128(BLE_TX))?
            .join()?
            .Characteristics()?
            .GetAt(0)?;
        let mtu = GattSession::FromDeviceIdAsync(&device.BluetoothDeviceId()?)?
            .join()
            .and_then(|s| s.MaxPduSize())
            .unwrap_or(23);
        let (sender, receiver) = mpsc::channel::<Chunk>();
        let data = sender.clone();
        tx.ValueChanged(
            &TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(move |_, args| {
                if let Ok(bytes) = buffer_bytes(&args.ok()?.CharacteristicValue()?) {
                    let _ = data.send(Chunk::Data(bytes));
                }
                Ok(())
            }),
        )?;
        let lost = sender.clone();
        device.ConnectionStatusChanged(
            &TypedEventHandler::<BluetoothLEDevice, windows::core::IInspectable>::new(move |dev, _| {
                if dev.ok()?.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected {
                    let _ = lost.send(Chunk::Closed);
                }
                Ok(())
            }),
        )?;
        let status = tx
            .WriteClientCharacteristicConfigurationDescriptorAsync(
                GattClientCharacteristicConfigurationDescriptorValue::Notify,
            )?
            .join()?;
        if status != GattCommunicationStatus::Success {
            return Err(windows::core::Error::new(windows::core::HRESULT(-1), "notify"));
        }
        Ok(BleConn(Arc::new(Inner {
            device,
            rx,
            chunk: (mtu as usize).saturating_sub(3),
            incoming: Mutex::new((receiver, VecDeque::new())),
            closer: sender,
            closed: AtomicBool::new(false),
        })))
    }

    pub fn run(core: Arc<Core>) {
        let seen: Arc<Mutex<HashMap<u64, (String, Instant)>>> = Arc::default();
        let watcher = loop {
            match BluetoothLEAdvertisementWatcher::new() {
                Ok(w) => break w,
                Err(_) => std::thread::sleep(Duration::from_secs(10)),
            }
        };
        let _ = watcher.SetScanningMode(BluetoothLEScanningMode::Active);
        {
            let seen = seen.clone();
            let handler =
                TypedEventHandler::<BluetoothLEAdvertisementWatcher, BluetoothLEAdvertisementReceivedEventArgs>::new(
                    move |_, args| {
                        let args = args.ok()?;
                        let name = args.Advertisement()?.LocalName()?.to_string();
                        let ours = name.starts_with("MNX")
                            || args
                                .Advertisement()?
                                .ServiceUuids()?
                                .into_iter()
                                .any(|u| u == GUID::from_u128(BLE_SERVICE));
                        if ours {
                            seen.lock()
                                .unwrap()
                                .insert(args.BluetoothAddress()?, (name, Instant::now()));
                        }
                        Ok(())
                    },
                );
            let _ = watcher.Received(&handler);
        }
        let connecting = Arc::new(AtomicBool::new(false));
        let mut retry_after: HashMap<u64, Instant> = HashMap::new();
        let mut running = false;
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let enabled = core.settings().bluetooth_enabled && core.snapshot().bluetooth.state == "ready";
            if enabled != running {
                running = enabled;
                let _ = if enabled { watcher.Start() } else { watcher.Stop() };
            }
            if !running || core.is_connected() || connecting.load(Ordering::Relaxed) {
                continue;
            }
            let pc_id = core.pc_id();
            let candidate = {
                let mut seen = seen.lock().unwrap();
                seen.retain(|_, (_, t)| t.elapsed() < Duration::from_secs(4));
                seen.iter()
                    .find(|(addr, (name, _))| {
                        wants_us(name, &pc_id) && retry_after.get(addr).is_none_or(|t| Instant::now() >= *t)
                    })
                    .map(|(a, _)| *a)
            };
            let Some(address) = candidate else { continue };
            retry_after.insert(address, Instant::now() + Duration::from_secs(4));
            connecting.store(true, Ordering::Relaxed);
            let (core, connecting) = (core.clone(), connecting.clone());
            let _ = std::thread::Builder::new().name("ble-session".into()).spawn(move || {
                match open(address) {
                    Ok(conn) => {
                        core.serve(Box::new(conn), Transport::Bluetooth, "Bluetooth LE".into(), false);
                    }
                    Err(e) => eprintln!("BLE connect: {e}"),
                }
                connecting.store(false, Ordering::Relaxed);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_name_selects_the_pc() {
        assert!(wants_us("MNX*", "df9ae8df-ef53"));
        assert!(wants_us("MNXdf9ae8df", "df9ae8df-ef53"));
        assert!(!wants_us("MNX0123abcd", "df9ae8df-ef53"));
        assert!(!wants_us("MNX", "df9ae8df"));
        assert!(!wants_us("iPhone", "df9ae8df"));
    }
}
