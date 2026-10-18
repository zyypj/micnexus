//! Runs the MicNexus core without the UI (Wi‑Fi + Bluetooth + USB), approves
//! every phone, and records the virtual cable's *capture* side — what Discord
//! & co. would hear — printing level and frequency once a second.
//!
//!   cargo run --example headless -- [seconds]
//!
//! Pair with `cargo run --example fake_phone` (another terminal).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use micnexus_desktop_lib::{
    adb, bluetooth,
    core::{Core, Event},
    lan,
    settings::Store,
};

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let dir = std::env::temp_dir().join("micnexus-headless");
    let core: Arc<Core> = Core::new(Store::new(&dir), "0.0.0-headless", |event| match event {
        Event::State(s) => {
            let session = s.session.as_ref().map(|s| {
                format!(
                    "{} via {:?} {:?}@{} muted={} buf={}ms rtt={:?}",
                    s.device_name, s.transport, s.codec, s.rate, s.muted, s.buffer_ms, s.rtt_ms
                )
            });
            println!(
                "[state] wifi={} bt={} usb={}/{} output={:?} default_mic={} session={:?}",
                s.wifi.ok,
                s.bluetooth.state,
                s.usb.state,
                s.usb.devices.len(),
                s.output.as_ref().map(|o| (&o.name, &o.capture_name)),
                s.is_default_mic,
                session
            );
        }
        Event::Level { .. } => {}
        Event::Connected { name, transport } => println!("[connected] {name} {transport:?}"),
        Event::Disconnected { name, reason } => println!("[disconnected] {name} ({reason})"),
        Event::Pending { name } => println!("[pending] {name} — approving"),
        Event::Declined { address, reason } => println!("[declined] {address} {reason}"),
    });
    // Do not touch the real default microphone from a test run.
    core.update_settings(|s| {
        s.set_default_mic = false;
        s.trusted.clear();
    });
    core.refresh_outputs();
    {
        let core = core.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(200));
                core.decide(true);
            }
        });
    }
    lan::start(core.clone());
    bluetooth::start(core.clone());
    adb::start(core.clone(), None);

    let capture = core
        .chosen_output()
        .and_then(|o| o.capture_id.map(|id| (id, o.capture_name)));
    match capture {
        Some((id, name)) => {
            println!("recording {name:?}");
            probe::record(&id, Duration::from_secs(seconds));
        }
        None => {
            println!("no virtual cable found; running without recording");
            std::thread::sleep(Duration::from_secs(seconds));
        }
    }
    let _ = Instant::now();
}

#[cfg(windows)]
mod probe {
    use std::time::{Duration, Instant};
    use windows::{
        Win32::{
            Media::{
                Audio::{
                    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, IAudioCaptureClient, IAudioClient,
                    WAVEFORMATEX,
                },
                Multimedia::WAVE_FORMAT_IEEE_FLOAT,
            },
            System::Com::CLSCTX_ALL,
        },
        core::HSTRING,
    };

    use micnexus_desktop_lib::audio::devices;

    pub fn record(id: &str, duration: Duration) {
        let _ = devices::with_com(|| {
            let e = devices::enumerator()?;
            let device = unsafe { e.GetDevice(&HSTRING::from(id)) }.map_err(|e| e.to_string())?;
            let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|e| e.to_string())?;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
                nChannels: 1,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 192_000,
                nBlockAlign: 4,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                    2_000_000,
                    0,
                    &format,
                    None,
                )
            }
            .map_err(|e| format!("Initialize: {e}"))?;
            let cap: IAudioCaptureClient = unsafe { client.GetService() }.map_err(|e| e.to_string())?;
            unsafe { client.Start() }.map_err(|e| e.to_string())?;
            let start = Instant::now();
            let mut window: Vec<f32> = Vec::new();
            while start.elapsed() < duration {
                std::thread::sleep(Duration::from_millis(10));
                loop {
                    let n = unsafe { cap.GetNextPacketSize() }.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    let mut data = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    if unsafe { cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }.is_err() {
                        break;
                    }
                    let silent = flags & 2 != 0; // AUDCLNT_BUFFERFLAGS_SILENT
                    let samples = unsafe { std::slice::from_raw_parts(data as *const f32, frames as usize) };
                    if silent {
                        window.extend(std::iter::repeat_n(0.0, frames as usize));
                    } else {
                        window.extend_from_slice(samples);
                    }
                    let _ = unsafe { cap.ReleaseBuffer(frames) };
                }
                if window.len() >= 48_000 {
                    let w: Vec<f32> = window.drain(..48_000).collect();
                    let rms = (w.iter().map(|s| s * s).sum::<f32>() / w.len() as f32).sqrt();
                    let crossings = w.windows(2).filter(|p| p[0] < 0.0 && p[1] >= 0.0).count();
                    println!(
                        "[cable] t={:>2}s rms={rms:.3} freq≈{crossings} Hz",
                        start.elapsed().as_secs()
                    );
                }
            }
            unsafe {
                let _ = client.Stop();
            }
            Ok(())
        })
        .map_err(|e| println!("probe: {e}"));
    }
}

#[cfg(not(windows))]
mod probe {
    pub fn record(_id: &str, d: std::time::Duration) {
        std::thread::sleep(d);
    }
}
