//! Tiny offline synthesiser: builds sound effects as mono sample buffers and
//! encodes them as 16-bit WAV so macroquad can load them. Deterministic: noise
//! comes from a seeded RNG, so every build sounds the same.

use std::f32::consts::TAU;

use crate::bag::Rng;

pub const RATE: u32 = 44_100;
/// Short fade at the end of every voice so nothing clicks when it stops.
const END_FADE: f32 = 0.004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Square,
    Triangle,
    Saw,
}

fn osc(wave: Wave, phase: f32) -> f32 {
    match wave {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
        Wave::Saw => 2.0 * phase - 1.0,
    }
}

fn samples_for(secs: f32) -> usize {
    (secs * RATE as f32) as usize
}

fn end_fade(t: f32, dur: f32) -> f32 {
    ((dur - t) / END_FADE).clamp(0.0, 1.0)
}

/// One oscillator with an exponential pitch sweep and attack/decay envelope.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    wave: Wave,
    freq: f32,
    freq_end: f32,
    dur: f32,
    attack: f32,
    /// Exponential decay rate, 1/s. 0 = sustain at full level.
    decay: f32,
    gain: f32,
    vibrato_hz: f32,
    vibrato_depth: f32,
}

impl Voice {
    pub fn new(wave: Wave, freq: f32, dur: f32) -> Self {
        Voice {
            wave,
            freq,
            freq_end: freq,
            dur,
            attack: 0.002,
            decay: 0.0,
            gain: 1.0,
            vibrato_hz: 0.0,
            vibrato_depth: 0.0,
        }
    }

    pub fn sweep_to(mut self, freq: f32) -> Self {
        self.freq_end = freq;
        self
    }

    pub fn attack(mut self, secs: f32) -> Self {
        self.attack = secs.max(1e-4);
        self
    }

    pub fn decay(mut self, rate: f32) -> Self {
        self.decay = rate;
        self
    }

    pub fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    /// Pitch wobble: `depth` is a fraction of the frequency (0.03 = ±3%).
    pub fn vibrato(mut self, hz: f32, depth: f32) -> Self {
        self.vibrato_hz = hz;
        self.vibrato_depth = depth;
        self
    }

    pub fn render(&self) -> Vec<f32> {
        let n = samples_for(self.dur);
        let mut out = Vec::with_capacity(n);
        let mut phase = 0.0f32;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let mut f = self.freq * (self.freq_end / self.freq).powf(t / self.dur);
            if self.vibrato_depth > 0.0 {
                f *= 1.0 + self.vibrato_depth * (TAU * self.vibrato_hz * t).sin();
            }
            phase = (phase + f / RATE as f32).fract();
            let env = (t / self.attack).min(1.0) * (-self.decay * t).exp() * end_fade(t, self.dur);
            out.push(osc(self.wave, phase) * env * self.gain);
        }
        out
    }
}

/// White noise through a one-pole low-pass whose cutoff sweeps from `cutoff` to `cutoff_end`.
pub fn noise(rng: &mut Rng, dur: f32, cutoff: f32, cutoff_end: f32, decay: f32, gain: f32) -> Vec<f32> {
    let n = samples_for(dur);
    let mut out = Vec::with_capacity(n);
    let mut y = 0.0f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let fc = cutoff * (cutoff_end / cutoff).powf(t / dur);
        let alpha = 1.0 - (-TAU * fc / RATE as f32).exp();
        let white = (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;
        y += alpha * (white - y);
        let env = (t / 0.001).min(1.0) * (-decay * t).exp() * end_fade(t, dur);
        out.push(y * env * gain);
    }
    out
}

/// One-pole low-pass, in place. Tames the edge of square and saw waves.
pub fn lowpass(buf: &mut [f32], cutoff: f32) {
    let alpha = 1.0 - (-TAU * cutoff / RATE as f32).exp();
    let mut y = 0.0;
    for s in buf {
        y += alpha * (*s - y);
        *s = y;
    }
}

/// Bell-ish tone: a few slightly inharmonic partials, higher ones dying faster.
pub fn bell(freq: f32, dur: f32, gain: f32) -> Vec<f32> {
    const PARTIALS: [(f32, f32, f32); 4] = [(1.0, 1.0, 5.0), (2.0, 0.5, 8.0), (3.01, 0.25, 12.0), (4.2, 0.12, 16.0)];
    let mut out = Vec::new();
    for (ratio, amp, decay) in PARTIALS {
        let v = Voice::new(Wave::Sine, freq * ratio, dur).decay(decay).gain(amp * gain);
        mix(&mut out, &v.render(), 0.0, 1.0);
    }
    out
}

/// Frequency `semitones` above (or below) `base`.
pub fn note(base: f32, semitones: i32) -> f32 {
    base * 2f32.powf(semitones as f32 / 12.0)
}

/// Adds `src` into `dst` starting `at` seconds in, growing `dst` as needed.
pub fn mix(dst: &mut Vec<f32>, src: &[f32], at: f32, gain: f32) {
    let start = samples_for(at);
    if dst.len() < start + src.len() {
        dst.resize(start + src.len(), 0.0);
    }
    for (d, s) in dst[start..].iter_mut().zip(src) {
        *d += s * gain;
    }
}

/// Scales so the loudest sample hits `peak`. Relative loudness between sounds is
/// set at playback, so every effect is normalised the same way.
pub fn normalize(buf: &mut [f32], peak: f32) {
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        let k = peak / max;
        buf.iter_mut().for_each(|s| *s *= k);
    }
}

/// 16-bit PCM mono WAV.
pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_matches_data() {
        let wav = encode_wav(&[0.0, 0.5, -0.5, 1.0]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(i16::from_le_bytes([wav[50], wav[51]]), i16::MAX);
    }

    #[test]
    fn voice_has_expected_length_and_ends_silent() {
        let v = Voice::new(Wave::Square, 440.0, 0.1).render();
        assert_eq!(v.len(), 4410);
        assert!(v.last().unwrap().abs() < 0.01, "end fade avoids clicks");
    }

    #[test]
    fn mix_extends_and_offsets() {
        let mut dst = vec![1.0; 2];
        mix(&mut dst, &[1.0, 1.0], 1.0 / RATE as f32, 0.5);
        assert_eq!(dst, vec![1.0, 1.5, 0.5]);
    }

    #[test]
    fn normalize_hits_peak() {
        let mut b = vec![0.1, -0.4, 0.2];
        normalize(&mut b, 0.8);
        assert!((b[1] + 0.8).abs() < 1e-6);
    }

    #[test]
    fn noise_is_deterministic() {
        let a = noise(&mut Rng::new(3), 0.05, 2000.0, 500.0, 10.0, 1.0);
        let b = noise(&mut Rng::new(3), 0.05, 2000.0, 500.0, 10.0, 1.0);
        assert_eq!(a, b);
    }

    #[test]
    fn octave_is_twelve_semitones() {
        assert!((note(440.0, 12) - 880.0).abs() < 1e-3);
    }
}
