//! Phone audio → 48 kHz float → playout buffers → WASAPI render streams
//! (the virtual cable, plus the speakers when "listen" is on).

pub mod devices;
pub mod jitter;
pub mod render;

use std::sync::{Arc, Mutex};

use jitter::{Jitter, RATE};
use render::Renderer;

use crate::protocol::Codec;

pub enum Decoder {
    Pcm { ratio: usize, prev: f32 },
    Opus(Box<opus_decoder::OpusDecoder>),
}

impl Decoder {
    pub fn new(codec: Codec, rate: u32) -> Result<Self, String> {
        match codec {
            Codec::Pcm16 => {
                let ratio = RATE / rate.max(1) as usize;
                if ratio == 0 || !RATE.is_multiple_of(rate as usize) {
                    return Err(format!("unsupported PCM rate {rate}"));
                }
                Ok(Self::Pcm { ratio, prev: 0.0 })
            }
            Codec::Opus => opus_decoder::OpusDecoder::new(RATE as u32, 1)
                .map(|d| Self::Opus(Box::new(d)))
                .map_err(|e| format!("opus: {e}")),
        }
    }

    /// Decodes one AUDIO payload (without the sequence number), appending
    /// 48 kHz mono samples to `out`. An empty `data` conceals a lost frame.
    pub fn decode(&mut self, data: &[u8], out: &mut Vec<f32>) {
        match self {
            Self::Pcm { ratio, prev } => {
                for pair in data.as_chunks::<2>().0 {
                    let s = i16::from_le_bytes(*pair) as f32 / 32768.0;
                    // Linear interpolation up to 48 kHz (24 kHz Bluetooth fallback).
                    for k in 1..=*ratio {
                        out.push(*prev + (s - *prev) * k as f32 / *ratio as f32);
                    }
                    *prev = s;
                }
            }
            Self::Opus(dec) => {
                let mut pcm = [0f32; 5760];
                if let Ok(n) = dec.decode_float(data, &mut pcm, false) {
                    out.extend_from_slice(&pcm[..n.min(pcm.len())]);
                }
            }
        }
    }
}

/// Peak/RMS over ~50 ms windows for the level meter.
#[derive(Default)]
pub struct Meter {
    peak: f32,
    sum: f64,
    count: usize,
}

impl Meter {
    pub const WINDOW: usize = RATE / 20;

    /// Returns `(peak, rms)` each time a window completes.
    pub fn feed(&mut self, samples: &[f32]) -> Option<(f32, f32)> {
        let mut done = None;
        for &s in samples {
            self.peak = self.peak.max(s.abs());
            self.sum += (s * s) as f64;
            self.count += 1;
            if self.count >= Self::WINDOW {
                done = Some((
                    self.peak.min(1.0),
                    ((self.sum / self.count as f64).sqrt() as f32).min(1.0),
                ));
                *self = Self::default();
            }
        }
        done
    }
}

struct Output {
    jitter: Arc<Mutex<Jitter>>,
    renderer: Renderer,
}

/// The render side of a session. Created on connect, dropped on disconnect.
pub struct AudioOut {
    main: Mutex<Option<Output>>,
    monitor: Mutex<Option<Output>>,
    target_ms: usize,
}

impl AudioOut {
    pub fn new(target_ms: usize) -> Self {
        Self {
            main: Mutex::new(None),
            monitor: Mutex::new(None),
            target_ms,
        }
    }

    fn open(&self, device_id: &str) -> Result<Output, String> {
        let jitter = Arc::new(Mutex::new(Jitter::new(self.target_ms, 20, 250)));
        let renderer = render::start(device_id, jitter.clone())?;
        Ok(Output { jitter, renderer })
    }

    /// (Re)opens the outputs when the chosen devices changed or failed.
    pub fn ensure(&self, main: Option<&str>, monitor: Option<&str>) -> Result<(), String> {
        let mut errors = Vec::new();
        for (slot, wanted) in [(&self.main, main), (&self.monitor, monitor)] {
            let mut slot = slot.lock().unwrap();
            let current_ok = slot
                .as_ref()
                .is_some_and(|o| o.renderer.alive() && Some(o.renderer.device_id.as_str()) == wanted);
            if current_ok {
                continue;
            }
            *slot = None;
            if let Some(id) = wanted {
                match self.open(id) {
                    Ok(o) => *slot = Some(o),
                    Err(e) => errors.push(e),
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    pub fn push(&self, samples: &[f32]) {
        for slot in [&self.main, &self.monitor] {
            if let Some(o) = slot.lock().unwrap().as_ref() {
                o.jitter.lock().unwrap().push(samples);
            }
        }
    }

    /// Drops whatever is queued (mute: stop at once, not 40 ms later).
    pub fn flush(&self) {
        for slot in [&self.main, &self.monitor] {
            if let Some(o) = slot.lock().unwrap().as_ref() {
                o.jitter.lock().unwrap().clear();
            }
        }
    }

    /// (buffered ms, underruns) of the main output.
    pub fn stats(&self) -> (u32, u32) {
        self.main
            .lock()
            .unwrap()
            .as_ref()
            .map(|o| {
                let j = o.jitter.lock().unwrap();
                (j.buffered_ms(), j.underruns)
            })
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_decodes_and_upsamples() {
        let mut d = Decoder::new(Codec::Pcm16, 48_000).unwrap();
        let mut out = Vec::new();
        d.decode(&[0x00, 0x40, 0x00, 0xc0], &mut out);
        assert_eq!(out, vec![0.5, -0.5]);

        let mut d = Decoder::new(Codec::Pcm16, 24_000).unwrap();
        let mut out = Vec::new();
        d.decode(&[0x00, 0x40], &mut out);
        assert_eq!(out, vec![0.25, 0.5]);
        assert!(Decoder::new(Codec::Pcm16, 44_100).is_err());
    }

    #[test]
    fn meter_reports_every_window() {
        let mut m = Meter::default();
        assert!(m.feed(&vec![0.5; Meter::WINDOW - 1]).is_none());
        let (peak, rms) = m.feed(&[0.5]).unwrap();
        assert_eq!(peak, 0.5);
        assert!((rms - 0.5).abs() < 1e-4);
    }

    #[test]
    fn opus_decoder_conceals_without_packets() {
        let mut d = Decoder::new(Codec::Opus, 48_000).unwrap();
        let mut out = Vec::new();
        d.decode(&[], &mut out);
        // No crash; PLC output length depends on the (empty) history.
        assert!(out.len() <= 5760);
    }
}
