//! WASAPI shared-mode render thread fed by a [`Jitter`] buffer.
//!
//! The stream is always 48 kHz mono float; `AUTOCONVERTPCM` lets Windows
//! convert to whatever the device runs at (VB-CABLE defaults to 44.1 kHz
//! stereo, for example), so nothing here depends on the device format.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

use super::jitter::Jitter;

pub struct Renderer {
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub device_id: String,
}

impl Renderer {
    /// False once the device went away or the stream failed.
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(windows)]
pub fn start(device_id: &str, jitter: Arc<Mutex<Jitter>>) -> Result<Renderer, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let alive = Arc::new(AtomicBool::new(true));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let thread = {
        let (stop, alive, id) = (stop.clone(), alive.clone(), device_id.to_string());
        std::thread::Builder::new()
            .name("render".into())
            .spawn(move || {
                let result = win::run(&id, &jitter, &stop, &ready_tx);
                if let Err(e) = &result {
                    let _ = ready_tx.send(Err(e.clone()));
                    eprintln!("render {id}: {e}");
                }
                alive.store(false, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())?
    };
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Renderer {
            stop,
            alive,
            thread: Some(thread),
            device_id: device_id.to_string(),
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err("render thread exited".into()),
    }
}

#[cfg(not(windows))]
pub fn start(_device_id: &str, _jitter: Arc<Mutex<Jitter>>) -> Result<Renderer, String> {
    Err("audio output is only implemented on Windows".into())
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::mpsc::Sender;
    use windows::{
        Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            Media::Audio::{
                AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient, WAVEFORMATEX,
            },
            Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT,
            System::{
                Com::CLSCTX_ALL,
                Threading::{
                    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, WaitForSingleObject,
                },
            },
        },
        core::{HSTRING, w},
    };

    use crate::audio::{devices, jitter::RATE};

    /// Device buffer: 20 ms (WASAPI wakes us every ~10 ms).
    const BUFFER_HNS: i64 = 200_000;

    pub fn run(
        id: &str,
        jitter: &Mutex<Jitter>,
        stop: &AtomicBool,
        ready: &Sender<Result<(), String>>,
    ) -> Result<(), String> {
        devices::with_com(|| {
            let enumerator = devices::enumerator()?;
            let device = unsafe { enumerator.GetDevice(&HSTRING::from(id)) }.map_err(|e| format!("GetDevice: {e}"))?;
            let client: IAudioClient =
                unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|e| format!("Activate: {e}"))?;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
                nChannels: 1,
                nSamplesPerSec: RATE as u32,
                nAvgBytesPerSec: RATE as u32 * 4,
                nBlockAlign: 4,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    BUFFER_HNS,
                    0,
                    &format,
                    None,
                )
            }
            .map_err(|e| format!("Initialize: {e}"))?;
            let event = unsafe { CreateEventW(None, false, false, None) }.map_err(|e| e.to_string())?;
            let result = (|| {
                unsafe { client.SetEventHandle(event) }.map_err(|e| format!("SetEventHandle: {e}"))?;
                let render: IAudioRenderClient =
                    unsafe { client.GetService() }.map_err(|e| format!("GetService: {e}"))?;
                let size = unsafe { client.GetBufferSize() }.map_err(|e| e.to_string())?;
                unsafe { client.Start() }.map_err(|e| format!("Start: {e}"))?;
                let _ = ready.send(Ok(()));

                let mut task = 0u32;
                let mmcss = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) }.ok();
                let mut scratch = vec![0f32; size as usize];
                let outcome = loop {
                    if stop.load(Ordering::Relaxed) {
                        break Ok(());
                    }
                    if unsafe { WaitForSingleObject(event, 200) } != WAIT_OBJECT_0 {
                        continue;
                    }
                    let padding = match unsafe { client.GetCurrentPadding() } {
                        Ok(p) => p,
                        Err(e) => break Err(format!("GetCurrentPadding: {e}")),
                    };
                    let frames = size.saturating_sub(padding);
                    if frames == 0 {
                        continue;
                    }
                    let out = &mut scratch[..frames as usize];
                    jitter.lock().map(|mut j| j.pull(out)).unwrap_or_else(|_| out.fill(0.0));
                    match unsafe { render.GetBuffer(frames) } {
                        Ok(ptr) => unsafe {
                            std::ptr::copy_nonoverlapping(out.as_ptr(), ptr as *mut f32, frames as usize);
                            let _ = render.ReleaseBuffer(frames, 0);
                        },
                        Err(e) => break Err(format!("GetBuffer: {e}")),
                    }
                };
                unsafe {
                    let _ = client.Stop();
                    if let Some(h) = mmcss {
                        let _ = AvRevertMmThreadCharacteristics(h);
                    }
                }
                outcome
            })();
            unsafe {
                let _ = CloseHandle(event);
            }
            result
        })
    }
}
