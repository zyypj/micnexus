//! Adaptive playout buffer between the network and a WASAPI render stream.
//!
//! - Starts playing once `target` samples are queued.
//! - An underrun (the network was late) stops playback until the target is
//!   reached again and raises the target, up to `max`.
//! - Ten seconds without underruns lowers the target by 2 ms, down to `min`.
//! - A burst (TCP retransmission catching up) never adds delay: anything
//!   over twice the target plus 20 ms is dropped with a short crossfade.

use std::collections::VecDeque;

pub const RATE: usize = 48_000;
const MS: usize = RATE / 1000;
/// Crossfade length for drops and fade-out length on underruns (2 ms).
const FADE: usize = 2 * MS;

pub struct Jitter {
    buf: VecDeque<f32>,
    target: usize,
    min: usize,
    max: usize,
    playing: bool,
    stable: usize,
    pub underruns: u32,
    pub dropped: u64,
    /// The device got silence last time: fade in on the next samples.
    silent: bool,
}

impl Jitter {
    pub fn new(target_ms: usize, min_ms: usize, max_ms: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(RATE),
            target: target_ms * MS,
            min: min_ms * MS,
            max: max_ms * MS,
            playing: false,
            stable: 0,
            underruns: 0,
            dropped: 0,
            silent: true,
        }
    }

    pub fn buffered_ms(&self) -> u32 {
        (self.buf.len() / MS) as u32
    }

    pub fn target_ms(&self) -> u32 {
        (self.target / MS) as u32
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.playing = false;
    }

    pub fn push(&mut self, samples: &[f32]) {
        self.buf.extend(samples.iter().copied());
        let limit = self.target * 2 + 20 * MS;
        if self.buf.len() > limit {
            let excess = self.buf.len() - self.target;
            // Crossfade what would have played next into what plays after the drop.
            let head: Vec<f32> = self.buf.iter().take(FADE).copied().collect();
            self.buf.drain(..excess);
            self.dropped += excess as u64;
            for (i, h) in head.iter().enumerate().take(self.buf.len().min(FADE)) {
                let t = (i + 1) as f32 / (FADE + 1) as f32;
                self.buf[i] = h * (1.0 - t) + self.buf[i] * t;
            }
        }
    }

    pub fn pull(&mut self, out: &mut [f32]) {
        if !self.playing {
            if self.buf.len() < self.target {
                out.fill(0.0);
                self.silent = true;
                return;
            }
            self.playing = true;
        }
        let n = self.buf.len().min(out.len());
        for (o, s) in out.iter_mut().zip(self.buf.drain(..n)) {
            *o = s;
        }
        if n < out.len() {
            // Underrun: fade out what we had, wait for the buffer to refill.
            let fade = n.min(FADE);
            for i in 0..fade {
                out[n - fade + i] *= 1.0 - (i + 1) as f32 / fade as f32;
            }
            out[n..].fill(0.0);
            self.playing = false;
            self.underruns += 1;
            self.stable = 0;
            self.target = (self.target + self.target / 2).clamp(self.min, self.max);
            self.silent = true;
            return;
        }
        if self.silent {
            // Fresh start after silence: short fade-in instead of a click.
            let fade = n.min(FADE);
            for (i, o) in out.iter_mut().take(fade).enumerate() {
                *o *= (i + 1) as f32 / fade as f32;
            }
        }
        self.silent = false;
        self.stable += n;
        if self.stable >= 10 * RATE && self.target > self.min {
            self.target = (self.target - 2 * MS).max(self.min);
            self.stable = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.1 + (i % 100) as f32 / 1000.0).collect()
    }

    #[test]
    fn waits_for_target_then_plays() {
        let mut j = Jitter::new(30, 20, 200);
        j.push(&ramp(20 * MS));
        let mut out = vec![1.0; 10 * MS];
        j.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0.0), "still buffering");
        j.push(&ramp(10 * MS));
        j.pull(&mut out);
        assert!(out[10..].iter().all(|&s| s > 0.0), "playing");
        assert_eq!(j.buffered_ms(), 20);
    }

    #[test]
    fn underrun_raises_target() {
        let mut j = Jitter::new(30, 20, 200);
        j.push(&ramp(30 * MS));
        let mut out = vec![0.0; 40 * MS];
        j.pull(&mut out);
        assert_eq!(j.underruns, 1);
        assert_eq!(j.target_ms(), 45);
        assert_eq!(out[35 * MS], 0.0);
    }

    #[test]
    fn bursts_are_dropped_not_delayed() {
        let mut j = Jitter::new(30, 20, 200);
        j.push(&ramp(500 * MS));
        assert_eq!(j.buffered_ms(), 30);
        assert!(j.dropped > 0);
    }

    #[test]
    fn stable_network_lowers_target() {
        let mut j = Jitter::new(40, 20, 200);
        let mut out = vec![0.0; 10 * MS];
        j.push(&ramp(40 * MS));
        for _ in 0..1001 {
            j.push(&ramp(10 * MS));
            j.pull(&mut out);
        }
        assert_eq!(j.underruns, 0);
        assert_eq!(j.target_ms(), 38);
    }
}
