//! Music engine: turns a `Song` (chords, patterns and melodies written as text in
//! `songs`) into looping stems. Every stem of a song is exactly the same length,
//! so started together they stay in sync, and each is built *circularly*: a note
//! ringing past the end of the loop wraps round to its start, so the seam is as
//! smooth as any other point in the loop.
//!
//! Rendering is split into small jobs (one per note) so it can be spread over
//! several frames instead of stalling one (`Render::step`). Pure and
//! deterministic: noise comes from seeded RNGs, no audio device involved.

use std::f32::consts::{PI, TAU};

use crate::bag::Rng;

use super::synth::{RATE, Wave};

/// Pad, bass, drums, lead, hype, tension. Volumes are driven per stem at runtime.
pub const STEMS: usize = 6;
pub const PAD: usize = 0;
pub const BASS: usize = 1;
pub const DRUMS: usize = 2;
pub const LEAD: usize = 3;
/// Arpeggios and extra percussion that join in when the game heats up.
pub const HYPE: usize = 4;
/// Ticking, pulsing layer for when the stack is near the top.
pub const TENSION: usize = 5;

pub const STEPS_PER_BAR: usize = 16;
pub const BARS: usize = 8;
const STEPS: usize = STEPS_PER_BAR * BARS;
/// Loudest point of a song's full mix (all stems at full volume).
pub const MIX_PEAK: f32 = 0.9;

const SR: f32 = RATE as f32;

pub fn midi_hz(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

/// A chord: root in semitones from the song's tonic, and intervals above it.
#[derive(Clone, Copy, Debug)]
pub struct Chord(pub i32, pub &'static [i32]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LeadKind {
    /// Two detuned oscillators through a filter.
    Synth,
    /// FM electric piano.
    Keys,
}

#[derive(Clone, Copy, Debug)]
pub struct LeadVoice {
    pub kind: LeadKind,
    pub wave: Wave,
    /// Detune between the two oscillators, cents.
    pub detune: f32,
    pub cutoff: f32,
    pub vibrato: f32,
    /// Octaves above the tonic.
    pub octave: i32,
    /// Feedback of a dotted-eighth echo (0 = dry).
    pub echo: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BassVoice {
    pub wave: Wave,
    /// Resting filter cutoff as a multiple of the note's frequency...
    pub cutoff: f32,
    /// ...plus this much more at the start of each note.
    pub env: f32,
    pub sub: f32,
    /// Octaves below the tonic.
    pub octave: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Arp {
    /// Chord-tone indices (past the chord's size climbs octaves).
    pub pattern: &'static [i32],
    /// Steps (16ths) per note.
    pub every: usize,
    pub wave: Wave,
    pub octave: i32,
    pub cutoff: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PadVoice {
    pub cutoff: f32,
    pub detune: f32,
}

/// A loop of `BARS` bars. Patterns are strings of 16 one-per-step tokens
/// separated by spaces or (for drums) one character per step.
#[derive(Clone, Copy, Debug)]
pub struct Song {
    pub bpm: f32,
    /// MIDI note of the tonic (the pad sits around here).
    pub tonic: i32,
    /// Semitones of the scale degrees melodies are written in.
    pub scale: &'static [i32],
    /// One per bar.
    pub chords: [Chord; BARS],
    /// Delay of every odd 16th, as a fraction of a step.
    pub swing: f32,
    /// Melody, one string per bar: scale degrees (`-1` is the degree below the
    /// tonic, `7` the octave; a `+` suffix sharpens), `.` holds, `-` rests.
    pub lead: [&'static str; BARS],
    pub lead_voice: LeadVoice,
    /// Bass per bar (cycled): `r` root, `o` octave up, `f` fifth, `s` octave down,
    /// `.` hold, `-` rest.
    pub bass: &'static [&'static str],
    pub bass_voice: BassVoice,
    pub arp: Option<Arp>,
    pub pad_voice: PadVoice,
    /// Drum lines per bar (cycled), one char per step: `X` accent, `x` hit, `g`
    /// ghost, `o` open hat (hat line only), `.` nothing.
    pub kick: &'static [&'static str],
    pub snare: &'static [&'static str],
    pub hat: &'static [&'static str],
    /// Replaces the snare line in the last bar (a fill); empty for none.
    pub fill: &'static str,
    /// Mix level of each stem.
    pub gains: [f32; STEMS],
    /// Sidechain duck of pad and bass on each kick (0..1).
    pub pump: f32,
    /// Render everything into a single stem (a track that doesn't react).
    pub mono: bool,
}

impl Song {
    /// Exact length of the loop in samples.
    pub fn loop_len(&self) -> usize {
        (STEPS as f64 * self.step_secs() as f64 * RATE as f64).round() as usize
    }

    pub fn step_secs(&self) -> f32 {
        60.0 / self.bpm / 4.0
    }

    fn step_samples(&self) -> f64 {
        self.loop_len() as f64 / STEPS as f64
    }

    /// Sample where a step starts (swing applied).
    fn at(&self, step: usize) -> usize {
        let swing = if step % 2 == 1 { self.swing as f64 } else { 0.0 };
        ((step as f64 + swing) * self.step_samples()).round() as usize % self.loop_len()
    }

    fn steps_to_samples(&self, steps: usize) -> usize {
        (steps as f64 * self.step_samples()).round() as usize
    }

    fn degree(&self, d: i32, sharp: bool) -> i32 {
        let n = self.scale.len() as i32;
        self.scale[d.rem_euclid(n) as usize] + 12 * d.div_euclid(n) + sharp as i32
    }

    /// Kick positions, for the sidechain pump.
    fn kick_samples(&self) -> Vec<usize> {
        let mut v = Vec::new();
        for bar in 0..BARS {
            let line = self.kick[bar % self.kick.len()].as_bytes();
            for (s, &c) in line.iter().enumerate() {
                if c != b'.' {
                    v.push(self.at(bar * STEPS_PER_BAR + s));
                }
            }
        }
        v
    }
}

/// Splits a 16-step line into (step, token, length-in-steps) notes; `.` holds the
/// previous token, `-` is silence.
fn notes(line: &str) -> Vec<(usize, &str, usize)> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let mut out: Vec<(usize, &str, usize)> = Vec::new();
    for (i, &t) in toks.iter().enumerate() {
        match t {
            "." => {
                if let Some(last) = out.last_mut() {
                    if last.0 + last.2 == i {
                        last.2 += 1;
                    }
                }
            }
            "-" => {}
            _ => out.push((i, t, 1)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Oscillators, filters, envelopes

/// Soft clipper close to `tanh` (Padé approximant), much cheaper.
fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// sin(2π·t) for t in [0, 1), by a parabola with one correction step (error < 0.1%).
fn sine01(t: f32) -> f32 {
    let x = if t < 0.5 { t } else { t - 1.0 } * 2.0; // -1..1, one cycle
    let y = 4.0 * x * (1.0 - x.abs());
    0.225 * (y * y.abs() - y) + y
}

fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Band-limited (PolyBLEP) oscillator.
#[derive(Clone, Copy)]
struct Osc {
    wave: Wave,
    phase: f32,
}

impl Osc {
    fn new(wave: Wave, phase: f32) -> Self {
        Osc { wave, phase }
    }

    fn next(&mut self, freq: f32) -> f32 {
        let dt = (freq / SR).min(0.45);
        let t = self.phase;
        let v = match self.wave {
            Wave::Sine => sine01(t),
            Wave::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
            Wave::Square => {
                let sq = if t < 0.5 { 1.0 } else { -1.0 };
                let half = if t < 0.5 { t + 0.5 } else { t - 0.5 };
                sq + poly_blep(t, dt) - poly_blep(half, dt)
            }
            Wave::Triangle => 4.0 * (t - 0.5).abs() - 1.0,
        };
        // (`fract` is a libm call on baseline x86-64; this is the hot loop.)
        let p = t + dt;
        self.phase = if p >= 1.0 { p - 1.0 } else { p };
        v
    }
}

/// Chamberlin state-variable low-pass with resonance.
#[derive(Clone, Copy, Default)]
struct Svf {
    low: f32,
    band: f32,
    f: f32,
    q: f32,
}

impl Svf {
    fn new(res: f32) -> Self {
        Svf {
            q: 1.0 - res.clamp(0.0, 0.9),
            ..Default::default()
        }
    }

    fn cutoff(&mut self, hz: f32) {
        self.f = 2.0 * (PI * hz.clamp(20.0, 7000.0) / SR).sin();
    }

    fn low(&mut self, x: f32) -> f32 {
        self.low += self.f * self.band;
        let high = x - self.low - self.q * self.band;
        self.band += self.f * high;
        self.low
    }
}

/// One-pole low-pass coefficient.
fn one_pole(hz: f32) -> f32 {
    1.0 - (-TAU * hz / SR).exp()
}

fn white(rng: &mut Rng) -> f32 {
    (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
}

fn secs(n: usize) -> f32 {
    n as f32 / SR
}

fn samples(secs: f32) -> usize {
    (secs * SR) as usize
}

/// Exponential decay computed by repeated multiplication (no `exp` per sample).
#[derive(Clone, Copy)]
struct Decay {
    v: f32,
    k: f32,
}

impl Decay {
    /// Falls by e every `1 / rate` seconds.
    fn new(rate: f32) -> Self {
        Decay { v: 1.0, k: (-rate / SR).exp() }
    }

    fn next(&mut self) -> f32 {
        let v = self.v;
        self.v *= self.k;
        v
    }
}

/// Gate envelope: linear attack, exponential decay towards `sustain`, and an
/// exponential release after `gate` samples (rendered for `release` seconds).
#[derive(Clone, Copy)]
struct Adsr {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl Adsr {
    fn len(&self, gate: usize) -> usize {
        gate + samples(self.release)
    }

    /// The whole envelope, `len(gate)` samples.
    fn curve(&self, gate: usize) -> Vec<f32> {
        let n = self.len(gate);
        let attack = samples(self.attack).max(1) as f32;
        let mut dec = Decay::new(1.0 / self.decay.max(1e-4));
        let rel_n = samples(self.release).max(1) as f32;
        // Exponential fall reaching ~0 at the end of the release, then a short linear fade.
        let mut rel = Decay::new(5.0 / self.release.max(1e-4));
        let mut held = 0.0;
        (0..n)
            .map(|i| {
                if i < gate {
                    held = (i as f32 / attack).min(1.0) * (self.sustain + (1.0 - self.sustain) * dec.next());
                    held
                } else {
                    held * rel.next() * (1.0 - (i - gate) as f32 / rel_n).max(0.0)
                }
            })
            .collect()
    }
}

/// Linear fade over the last `over` samples of `n`, so nothing ends with a click.
fn tail(i: usize, n: usize, over: f32) -> f32 {
    ((n - i) as f32 / over).min(1.0)
}

// ---------------------------------------------------------------------------
// Instruments. Each returns a buffer of the whole sound, tail included.

fn kick(vel: f32) -> Vec<f32> {
    let n = samples(0.32);
    let mut osc = Osc::new(Wave::Sine, 0.0);
    let (mut pitch, mut amp) = (Decay::new(32.0), Decay::new(8.0));
    (0..n)
        .map(|i| {
            let f = 48.0 + 120.0 * pitch.next();
            let body = osc.next(f) * amp.next();
            let click = if i < 90 { (1.0 - i as f32 / 90.0) * 0.4 } else { 0.0 };
            soft_clip((body + click) * 1.6) * vel * tail(i, n, 200.0)
        })
        .collect()
}

fn snare(vel: f32, rng: &mut Rng) -> Vec<f32> {
    let n = samples(0.22);
    let mut osc = Osc::new(Wave::Triangle, 0.0);
    let k = one_pole(1800.0);
    let mut lp = 0.0;
    let (mut body, mut noise) = (Decay::new(28.0), Decay::new(17.0));
    (0..n)
        .map(|i| {
            let t = secs(i);
            let tone = osc.next(200.0 - 40.0 * (t * 20.0).min(1.0)) * body.next() * 0.55;
            let w = white(rng);
            lp += k * (w - lp);
            let rattle = (w - lp) * noise.next() * 0.75;
            (tone + rattle) * vel * tail(i, n, 200.0)
        })
        .collect()
}

fn clap(vel: f32, rng: &mut Rng) -> Vec<f32> {
    let n = samples(0.2);
    let mut f = Svf::new(0.5);
    f.cutoff(1400.0);
    let k = one_pole(800.0);
    let mut lp = 0.0;
    // Three quick bursts, then a tail.
    let starts = [0, samples(0.011), samples(0.022)];
    let mut bursts = [Decay::new(160.0); 3];
    let mut body = Decay::new(22.0);
    (0..n)
        .map(|i| {
            let mut env = body.next() * 0.5;
            for (b, &s) in bursts.iter_mut().zip(&starts) {
                if i >= s {
                    env += b.next();
                }
            }
            let w = white(rng);
            lp += k * (w - lp);
            let x = f.low(w - lp) * 1.6;
            x * env * vel * tail(i, n, 200.0)
        })
        .collect()
}

/// High-passed noise. `decay` 1/s.
fn hat(vel: f32, dur: f32, decay: f32, hp: f32, rng: &mut Rng) -> Vec<f32> {
    let n = samples(dur);
    let k = one_pole(hp);
    let mut lp = 0.0;
    let mut env = Decay::new(decay);
    (0..n)
        .map(|i| {
            let w = white(rng);
            lp += k * (w - lp);
            let attack = (i as f32 / 66.0).min(1.0);
            (w - lp) * attack * env.next() * tail(i, n, 150.0) * vel
        })
        .collect()
}

fn bass(freq: f32, gate: usize, vel: f32, v: &BassVoice) -> Vec<f32> {
    let env = Adsr { attack: 0.003, decay: 0.25, sustain: 0.85, release: 0.04 }.curve(gate);
    let mut osc = Osc::new(v.wave, 0.0);
    let mut sub = Osc::new(Wave::Sine, 0.0);
    let mut filt = Svf::new(0.35);
    let mut sweep = Decay::new(14.0 / 16.0);
    env.iter()
        .enumerate()
        .map(|(i, e)| {
            if i % 16 == 0 {
                filt.cutoff(freq * (v.cutoff + v.env * sweep.next()));
            }
            let x = filt.low(osc.next(freq)) + sub.next(freq) * v.sub;
            soft_clip(x * 1.3) * e * vel
        })
        .collect()
}

fn lead(freq: f32, gate: usize, vel: f32, v: &LeadVoice) -> Vec<f32> {
    match v.kind {
        LeadKind::Synth => {
            let env = Adsr { attack: 0.006, decay: 0.2, sustain: 0.7, release: 0.09 }.curve(gate);
            let ratio = 2f32.powf(v.detune / 1200.0);
            let mut a = Osc::new(v.wave, 0.0);
            let mut b = Osc::new(v.wave, 0.37);
            let mut lfo = Osc::new(Wave::Sine, 0.0);
            let mut filt = Svf::new(0.2);
            let mut sweep = Decay::new(9.0 / 16.0);
            env.iter()
                .enumerate()
                .map(|(i, e)| {
                    let t = secs(i);
                    if i % 16 == 0 {
                        filt.cutoff(v.cutoff * (1.0 + 1.5 * sweep.next()));
                    }
                    // Vibrato fades in on held notes.
                    let wobble = lfo.next(5.5);
                    let f = if t > 0.18 { freq * (1.0 + v.vibrato * ((t - 0.18) / 0.25).min(1.0) * wobble) } else { freq };
                    let x = a.next(f) + b.next(f * ratio);
                    filt.low(x * 0.5) * e * vel
                })
                .collect()
        }
        LeadKind::Keys => {
            let env = Adsr { attack: 0.003, decay: 0.6, sustain: 0.35, release: 0.35 }.curve(gate);
            let mut car = Osc::new(Wave::Sine, 0.0);
            let mut modu = Osc::new(Wave::Sine, 0.0);
            let mut tine = Osc::new(Wave::Sine, 0.0);
            let (mut index, mut ring) = (Decay::new(5.0), Decay::new(14.0));
            env.iter()
                .map(|e| {
                    // Phase modulation, ratio 1:1 (a soft electric piano).
                    let m = modu.next(freq) * (1.2 * index.next() + 0.15) * 0.16;
                    car.next(freq);
                    let p = car.phase + m;
                    let body = sine01(p - p.floor());
                    let bell = tine.next(freq * 4.0) * 0.18 * ring.next();
                    (body + bell) * e * vel
                })
                .collect()
        }
    }
}

fn pluck(freq: f32, gate: usize, vel: f32, wave: Wave, cutoff: f32) -> Vec<f32> {
    let n = (gate + samples(0.12)).min(samples(0.45));
    let mut osc = Osc::new(wave, 0.0);
    let mut filt = Svf::new(0.3);
    let (mut sweep, mut amp) = (Decay::new(16.0 / 16.0), Decay::new(9.0));
    (0..n)
        .map(|i| {
            if i % 16 == 0 {
                filt.cutoff(freq * 1.5 + cutoff * sweep.next());
            }
            let env = (i as f32 / 88.0).min(1.0) * amp.next() * tail(i, n, 300.0);
            filt.low(osc.next(freq)) * env * vel
        })
        .collect()
}

/// Pad and drone are filtered dark, so they're rendered at a quarter of the
/// rate (most of the work of a song) and interpolated back up.
const SLOW: usize = 4;

/// Linear interpolation of a buffer rendered at `RATE / SLOW` up to `n` samples.
fn upsample(slow: &[f32], n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let j = i / SLOW;
            let frac = (i % SLOW) as f32 / SLOW as f32;
            let a = slow.get(j).copied().unwrap_or(0.0);
            let b = slow.get(j + 1).copied().unwrap_or(0.0);
            a + (b - a) * frac
        })
        .collect()
}

/// Sustained chord: two detuned saws and a triangle per tone, softly filtered.
fn pad(freqs: &[f32], gate: usize, vel: f32, v: &PadVoice) -> Vec<f32> {
    let env = Adsr { attack: 0.25, decay: 1.0, sustain: 0.85, release: 0.5 }.curve(gate);
    let ratio = 2f32.powf(v.detune / 1200.0);
    // Frequencies scaled up so the oscillators step at the slow rate.
    let mut oscs: Vec<(Osc, Osc, Osc, f32)> = freqs
        .iter()
        .enumerate()
        .map(|(k, &f)| {
            let p = k as f32 * 0.23;
            (Osc::new(Wave::Saw, p), Osc::new(Wave::Saw, p + 0.5), Osc::new(Wave::Triangle, p), f * SLOW as f32)
        })
        .collect();
    let k = one_pole(v.cutoff * SLOW as f32);
    let (mut l1, mut l2) = (0.0, 0.0);
    let norm = 1.0 / (freqs.len() as f32).sqrt();
    let slow: Vec<f32> = env
        .iter()
        .step_by(SLOW)
        .map(|e| {
            let mut x = 0.0;
            for (a, b, c, f) in &mut oscs {
                x += a.next(*f * ratio) * 0.5 + b.next(*f / ratio) * 0.5 + c.next(*f) * 0.4;
            }
            l1 += k * (x - l1);
            l2 += k * (l1 - l2);
            l2 * norm * e * vel
        })
        .collect();
    upsample(&slow, env.len())
}

/// Two sines a semitone apart, trembling: an uneasy high drone.
fn drone(freq: f32, gate: usize, vel: f32) -> Vec<f32> {
    let env = Adsr { attack: 0.4, decay: 1.0, sustain: 1.0, release: 0.4 }.curve(gate);
    let mut a = Osc::new(Wave::Sine, 0.0);
    let mut b = Osc::new(Wave::Sine, 0.0);
    let mut lfo = Osc::new(Wave::Sine, 0.0);
    let semi = 2f32.powf(1.0 / 12.0);
    let f = freq * SLOW as f32;
    let slow: Vec<f32> = env
        .iter()
        .step_by(SLOW)
        .map(|e| {
            let trem = 0.65 + 0.35 * lfo.next(7.0 * SLOW as f32);
            (a.next(f) + b.next(f * semi)) * 0.5 * trem * e * vel
        })
        .collect();
    upsample(&slow, env.len())
}

// ---------------------------------------------------------------------------
// Scheduling and rendering

#[derive(Clone, Debug)]
enum Inst {
    Kick,
    Snare,
    Clap,
    Hat,
    OpenHat,
    Shaker,
    Crash,
    Tick,
    Bass,
    /// Tension's pulsing low notes.
    Pulse,
    Lead,
    Pluck,
    Pad(Vec<f32>),
    Drone,
}

#[derive(Clone, Debug)]
struct Job {
    stem: usize,
    at: usize,
    gate: usize,
    freq: f32,
    vel: f32,
    inst: Inst,
}

/// A song being rendered, job by job.
pub struct Render {
    song: &'static Song,
    stems: Vec<Vec<f32>>,
    jobs: Vec<Job>,
    next: usize,
    /// Post-processing stages done: one per stem, then the final mixdown.
    post: usize,
}

fn vel_of(c: u8) -> Option<f32> {
    match c {
        b'X' => Some(1.0),
        b'x' => Some(0.75),
        b'o' => Some(0.7),
        b'g' => Some(0.3),
        _ => None,
    }
}

impl Render {
    pub fn new(song: &'static Song) -> Self {
        let len = song.loop_len();
        let mut r = Render {
            song,
            stems: vec![vec![0.0; len]; STEMS],
            jobs: Vec::new(),
            next: 0,
            post: 0,
        };
        r.schedule();
        r
    }

    fn job(&mut self, stem: usize, step: usize, steps: usize, freq: f32, vel: f32, inst: Inst) {
        if self.song.gains[stem] <= 0.0 {
            return;
        }
        let gate = self.song.steps_to_samples(steps).max(1);
        let at = self.song.at(step);
        self.jobs.push(Job { stem, at, gate, freq, vel, inst });
    }

    fn schedule(&mut self) {
        let s = self.song;
        let tonic = s.tonic;
        for bar in 0..BARS {
            let b0 = bar * STEPS_PER_BAR;
            let Chord(root, ivs) = s.chords[bar];
            let last = bar == BARS - 1;

            // Pad: chord tones folded into the octave around the tonic, held
            // across bars while the chord doesn't change.
            let same = |a: usize, b: usize| {
                let (Chord(ra, ia), Chord(rb, ib)) = (s.chords[a], s.chords[b]);
                ra == rb && ia == ib
            };
            let held = (bar + 1..BARS).take_while(|&b| same(b, bar)).count();
            let freqs: Vec<f32> = ivs
                .iter()
                .map(|&iv| {
                    let t = (root + iv + 2).rem_euclid(12) - 2;
                    midi_hz((tonic + t) as f32)
                })
                .collect();
            if bar == 0 || !same(bar - 1, bar) {
                self.job(PAD, b0, STEPS_PER_BAR * (1 + held), 0.0, 1.0, Inst::Pad(freqs));
            }

            // Bass.
            let bass_root = tonic - 12 * s.bass_voice.octave + root;
            for (st, tok, len) in notes(s.bass[bar % s.bass.len()]) {
                let off = match tok {
                    "o" => 12,
                    "f" => 7,
                    "s" => -12,
                    _ => 0,
                };
                let vel = if st % 4 == 0 { 1.0 } else { 0.8 };
                self.job(BASS, b0 + st, len, midi_hz((bass_root + off) as f32), vel, Inst::Bass);
            }

            // Drums.
            let kick = s.kick[bar % s.kick.len()].as_bytes();
            let snare = if last && !s.fill.is_empty() { s.fill } else { s.snare[bar % s.snare.len()] }.as_bytes();
            let hat = s.hat[bar % s.hat.len()].as_bytes();
            for st in 0..STEPS_PER_BAR {
                let step = b0 + st;
                if let Some(v) = kick.get(st).copied().and_then(vel_of) {
                    self.job(DRUMS, step, 1, 0.0, v, Inst::Kick);
                }
                if let Some(v) = snare.get(st).copied().and_then(vel_of) {
                    self.job(DRUMS, step, 1, 0.0, v, Inst::Snare);
                    // The hype layer doubles accented snares with a clap.
                    if v >= 0.75 {
                        self.job(HYPE, step, 1, 0.0, v * 0.8, Inst::Clap);
                    }
                }
                match hat.get(st) {
                    Some(b'o') => self.job(DRUMS, step, 1, 0.0, 0.55, Inst::OpenHat),
                    Some(&c) => {
                        if let Some(v) = vel_of(c) {
                            self.job(DRUMS, step, 1, 0.0, v * 0.6, Inst::Hat);
                        }
                    }
                    None => {}
                }
                // Hype: a 16th shaker, accented on the off-beats.
                let shake = if st % 4 == 2 { 0.7 } else { 0.35 };
                self.job(HYPE, step, 1, 0.0, shake, Inst::Shaker);
                // Tension: a ticking clock and a pulse on the root.
                self.job(TENSION, step, 1, 0.0, if st % 4 == 0 { 0.9 } else { 0.5 }, Inst::Tick);
                if st % 2 == 0 {
                    let f = midi_hz((tonic - 12 + root.rem_euclid(12) - 12 * (root.rem_euclid(12) > 6) as i32) as f32);
                    self.job(TENSION, step, 1, f, if st % 4 == 0 { 1.0 } else { 0.7 }, Inst::Pulse);
                }
            }
            if bar % 4 == 0 {
                self.job(HYPE, b0, 1, 0.0, if bar == 0 { 0.9 } else { 0.6 }, Inst::Crash);
            }
            // Tension: an uneasy drone two octaves over the chord root, a bar at a time.
            let drone_root = tonic + 12 + root.rem_euclid(12);
            self.job(TENSION, b0, STEPS_PER_BAR, midi_hz(drone_root as f32), 1.0, Inst::Drone);

            // Lead.
            let lead_base = tonic + 12 * s.lead_voice.octave;
            for (st, tok, len) in notes(s.lead[bar]) {
                let (num, sharp) = match tok.strip_suffix('+') {
                    Some(n) => (n, true),
                    None => (tok, false),
                };
                if let Ok(d) = num.parse::<i32>() {
                    let m = lead_base + s.degree(d, sharp);
                    self.job(LEAD, b0 + st, len, midi_hz(m as f32), if st % 4 == 0 { 1.0 } else { 0.85 }, Inst::Lead);
                }
            }

            // Arpeggio over the chord.
            if let Some(arp) = s.arp {
                let n = ivs.len() as i32;
                let base = tonic + 12 * arp.octave + root;
                for (k, st) in (0..STEPS_PER_BAR).step_by(arp.every).enumerate() {
                    let idx = arp.pattern[k % arp.pattern.len()];
                    let m = base + ivs[idx.rem_euclid(n) as usize] + 12 * idx.div_euclid(n);
                    let vel = if st % 4 == 0 { 1.0 } else { 0.7 };
                    self.job(HYPE, b0 + st, arp.every, midi_hz(m as f32), vel, Inst::Pluck);
                }
            }
        }
    }

    /// Samples a job writes, as a rough cost.
    fn cost(job: &Job) -> usize {
        let voices = match &job.inst {
            Inst::Pad(f) => f.len() * 3,
            Inst::Lead | Inst::Bass => 2,
            _ => 1,
        };
        job.gate * voices
    }

    /// Renders jobs, then post-processes, until about `budget` sample-voices are
    /// done. True when everything is.
    pub fn step(&mut self, budget: usize) -> bool {
        let mut spent = 0;
        while spent < budget {
            if self.next < self.jobs.len() {
                spent += self.run_job();
            } else if self.post <= STEMS {
                spent += self.post_stage(self.post);
                self.post += 1;
            } else {
                break;
            }
        }
        self.post > STEMS
    }

    fn run_job(&mut self) -> usize {
        let job = self.jobs[self.next].clone();
        let mut rng = Rng::new(0x5EED_0000 ^ (self.next as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let s = self.song;
        let buf = match &job.inst {
            Inst::Kick => kick(job.vel),
            Inst::Snare => snare(job.vel, &mut rng),
            Inst::Clap => clap(job.vel, &mut rng),
            Inst::Hat => hat(job.vel, 0.05, 60.0, 7000.0, &mut rng),
            Inst::OpenHat => hat(job.vel, 0.32, 9.0, 6500.0, &mut rng),
            Inst::Shaker => hat(job.vel, 0.06, 45.0, 5000.0, &mut rng),
            Inst::Crash => hat(job.vel, 1.8, 2.2, 4000.0, &mut rng),
            Inst::Tick => {
                let mut t = hat(job.vel, 0.03, 120.0, 8000.0, &mut rng);
                let mut o = Osc::new(Wave::Sine, 0.0);
                let mut env = Decay::new(150.0);
                for x in t.iter_mut() {
                    *x += o.next(2600.0) * 0.25 * job.vel * env.next();
                }
                t
            }
            Inst::Bass => bass(job.freq, job.gate, job.vel, &s.bass_voice),
            Inst::Pulse => {
                let v = BassVoice { wave: Wave::Saw, cutoff: 2.0, env: 10.0, sub: 0.6, octave: 2 };
                bass(job.freq, job.gate / 2, job.vel, &v)
            }
            Inst::Lead => lead(job.freq, job.gate, job.vel, &s.lead_voice),
            Inst::Pluck => {
                let arp = s.arp.expect("plucks come from an arp");
                pluck(job.freq, job.gate, job.vel, arp.wave, arp.cutoff)
            }
            Inst::Pad(freqs) => pad(freqs, job.gate, job.vel, &s.pad_voice),
            Inst::Drone => drone(job.freq, job.gate, job.vel),
        };
        add_wrapped(&mut self.stems[job.stem], &buf, job.at);
        self.next += 1;
        Self::cost(&job).max(1000)
    }

    /// Stage `k < STEMS` finishes stem `k` (echo, sidechain, DC, level); stage
    /// `STEMS` mixes down (mono songs) and normalises. Returns a rough cost.
    fn post_stage(&mut self, k: usize) -> usize {
        let s = self.song;
        let n = s.loop_len();
        if k < STEMS {
            let buf = &mut self.stems[k];
            let echo_delay = s.steps_to_samples(3);
            match k {
                LEAD if s.lead_voice.echo > 0.0 => echo(buf, echo_delay, s.lead_voice.echo),
                HYPE => echo(buf, echo_delay, 0.25),
                _ => {}
            }
            if s.pump > 0.0 && matches!(k, PAD | BASS | HYPE) {
                let env = pump_env(n, &s.kick_samples(), s.pump, s.step_secs() * 2.5);
                for (x, g) in buf.iter_mut().zip(&env) {
                    *x *= g;
                }
            }
            dc_block_loop(buf);
            // Stems come out at wildly different raw levels; even them out, then apply the mix.
            let rms = rms(buf);
            if rms > 0.0 {
                let g = s.gains[k] * 0.2 / rms;
                buf.iter_mut().for_each(|x| *x *= g);
            }
            return n * 12;
        }
        if s.mono {
            let mut mix = vec![0.0; n];
            for buf in &self.stems {
                for (m, x) in mix.iter_mut().zip(buf) {
                    *m += x;
                }
            }
            self.stems = vec![mix];
        }
        let peak_sum = (0..n).map(|i| self.stems.iter().map(|b| b[i]).sum::<f32>().abs()).fold(0.0, f32::max);
        let peak_one = self.stems.iter().flatten().fold(0.0f32, |m, x| m.max(x.abs()));
        let peak = peak_sum.max(peak_one);
        if peak > 0.0 {
            let g = MIX_PEAK / peak;
            self.stems.iter_mut().flatten().for_each(|x| *x *= g);
        }
        n * 8
    }

    /// The finished stems (`STEMS` of them, or one for a `mono` song), rendering
    /// whatever `step` hasn't yet.
    pub fn finish(mut self) -> Vec<Vec<f32>> {
        while !self.step(usize::MAX) {}
        self.stems
    }
}

/// Renders a whole song at once.
#[cfg(test)]
pub fn render(song: &'static Song) -> Vec<Vec<f32>> {
    Render::new(song).finish()
}

/// Adds `src` into the loop `dst` from `at`, wrapping round past the end.
fn add_wrapped(dst: &mut [f32], src: &[f32], at: usize) {
    let n = dst.len();
    let mut pos = at % n;
    let mut rest = src;
    while !rest.is_empty() {
        let take = rest.len().min(n - pos);
        for (d, x) in dst[pos..pos + take].iter_mut().zip(&rest[..take]) {
            *d += x;
        }
        rest = &rest[take..];
        pos = 0;
    }
}

/// Circular feedback echo: each repeat wraps round the loop like the notes do.
fn echo(buf: &mut [f32], delay: usize, feedback: f32) {
    let n = buf.len();
    let dry = buf.to_vec();
    let mut g = feedback;
    for k in 1..=4 {
        let d = (delay * k) % n;
        // buf[i] += dry[i - d], wrapping: the first `d` samples hear the loop's end.
        for (b, x) in buf[d..].iter_mut().zip(&dry) {
            *b += x * g;
        }
        for (b, x) in buf[..d].iter_mut().zip(&dry[n - d..]) {
            *b += x * g;
        }
        g *= feedback;
    }
}

/// Removes DC (and sub-sonic rumble) with a 20 Hz high-pass run round the loop
/// twice, so the filter's state at the seam matches the loop's end.
fn dc_block_loop(buf: &mut [f32]) {
    let r = 1.0 - TAU * 20.0 / SR;
    let (mut x1, mut y1) = (0.0f32, 0.0f32);
    for &x in buf.iter() {
        y1 = x - x1 + r * y1;
        x1 = x;
    }
    for s in buf.iter_mut() {
        let x = *s;
        y1 = x - x1 + r * y1;
        x1 = x;
        *s = y1;
    }
}

/// Gain per sample ducking after each kick, recovering over `recover` seconds.
fn pump_env(n: usize, kicks: &[usize], depth: f32, recover: f32) -> Vec<f32> {
    let mut env = vec![1.0; n];
    if kicks.is_empty() {
        return env;
    }
    let mut sorted = kicks.to_vec();
    sorted.sort_unstable();
    let rec = samples(recover).max(1);
    // Duck over a few ms rather than instantly, which would click.
    let dip = samples(0.004).max(1);
    for (k, &start) in sorted.iter().enumerate() {
        let end = if k + 1 < sorted.len() { sorted[k + 1] } else { sorted[0] + n };
        for i in start..end {
            let x = ((i - start) as f32 / rec as f32).min(1.0);
            let down = ((i - start) as f32 / dip as f32).min(1.0);
            env[i % n] = 1.0 - depth * down * (1.0 - x) * (1.0 - x);
        }
    }
    env
}

pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt()
}

// ---------------------------------------------------------------------------
// Stings

fn place(dst: &mut Vec<f32>, src: &[f32], at: f32, gain: f32) {
    let start = samples(at);
    if dst.len() < start + src.len() {
        dst.resize(start + src.len(), 0.0);
    }
    for (d, s) in dst[start..].iter_mut().zip(src) {
        *d += s * gain;
    }
}

fn normalized(mut buf: Vec<f32>) -> Vec<f32> {
    let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        buf.iter_mut().for_each(|x| *x *= MIX_PEAK / peak);
    }
    // Make sure it ends in silence.
    let n = buf.len();
    let fade = samples(0.05).min(n);
    for i in 0..fade {
        buf[n - fade + i] *= 1.0 - i as f32 / fade as f32;
    }
    buf
}

/// Short fanfare for a won match. Starts after a beat of silence so the win SFX lands first.
pub fn victory_sting() -> Vec<f32> {
    let lead_v = LeadVoice { kind: LeadKind::Synth, wave: Wave::Square, detune: 8.0, cutoff: 3200.0, vibrato: 0.006, octave: 0, echo: 0.0 };
    let pad_v = PadVoice { cutoff: 2200.0, detune: 9.0 };
    let bass_v = BassVoice { wave: Wave::Saw, cutoff: 3.0, env: 6.0, sub: 0.6, octave: 0 };
    let mut out = vec![0.0; samples(0.25)];
    let t0 = 0.25;
    let beat = 0.15;
    // C major run up, then a held high C over F -> G -> C.
    for (k, m) in [72, 76, 79, 84].iter().enumerate() {
        place(&mut out, &lead(midi_hz(*m as f32), samples(beat * 0.9), 0.9, &lead_v), t0 + k as f32 * beat, 0.5);
    }
    place(&mut out, &lead(midi_hz(88.0), samples(beat * 1.8), 0.9, &lead_v), t0 + 4.0 * beat, 0.5);
    place(&mut out, &lead(midi_hz(86.0), samples(beat * 0.9), 0.9, &lead_v), t0 + 6.0 * beat, 0.5);
    place(&mut out, &lead(midi_hz(91.0), samples(1.0), 1.0, &lead_v), t0 + 7.0 * beat, 0.55);
    let chords: [(f32, &[i32], i32); 3] = [(0.0, &[65, 69, 72], 41), (4.0, &[67, 71, 74], 43), (7.0, &[64, 67, 72, 76], 36)];
    for (at, notes, root) in chords {
        let freqs: Vec<f32> = notes.iter().map(|&m| midi_hz(m as f32)).collect();
        let len = if at < 7.0 { samples(beat * 3.5) } else { samples(1.3) };
        place(&mut out, &pad(&freqs, len, 1.0, &pad_v), t0 + at * beat, 0.6);
        place(&mut out, &bass(midi_hz(root as f32), len, 1.0, &bass_v), t0 + at * beat, 0.5);
        place(&mut out, &kick(1.0), t0 + at * beat, 0.6);
    }
    let mut rng = Rng::new(0xFA2F_A2E);
    place(&mut out, &hat(1.0, 1.8, 2.0, 4000.0, &mut rng), t0 + 7.0 * beat, 0.35);
    for k in 0..7 {
        place(&mut out, &snare(0.5 + 0.07 * k as f32, &mut rng), t0 + k as f32 * beat, 0.3);
    }
    normalized(out)
}

/// Falling, slowing minor phrase for a lost run.
pub fn defeat_sting() -> Vec<f32> {
    let lead_v = LeadVoice { kind: LeadKind::Synth, wave: Wave::Triangle, detune: 12.0, cutoff: 1800.0, vibrato: 0.012, octave: 0, echo: 0.0 };
    let pad_v = PadVoice { cutoff: 1200.0, detune: 12.0 };
    let bass_v = BassVoice { wave: Wave::Saw, cutoff: 2.0, env: 3.0, sub: 0.8, octave: 0 };
    let mut out = vec![0.0; samples(0.35)];
    let t0 = 0.35;
    // A minor: E D C B ... A, each note a bit longer than the last.
    let mut t = t0;
    for (k, m) in [76, 74, 72, 71].iter().enumerate() {
        let d = 0.2 + 0.05 * k as f32;
        place(&mut out, &lead(midi_hz(*m as f32), samples(d * 0.9), 0.8, &lead_v), t, 0.5);
        t += d;
    }
    place(&mut out, &lead(midi_hz(69.0), samples(1.1), 0.8, &lead_v), t, 0.5);
    let chords: [(f32, &[i32], i32); 2] = [(t0, &[65, 69, 72], 41), (t, &[64, 69, 72], 45)];
    for (k, (at, notes, root)) in chords.into_iter().enumerate() {
        let freqs: Vec<f32> = notes.iter().map(|&m| midi_hz(m as f32)).collect();
        let len = if k == 0 { samples(t - t0) } else { samples(1.3) };
        place(&mut out, &pad(&freqs, len, 1.0, &pad_v), at, 0.6);
        place(&mut out, &bass(midi_hz(root as f32), len, 1.0, &bass_v), at, 0.5);
    }
    place(&mut out, &kick(0.8), t, 0.5);
    normalized(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn fast_sine_and_clip_are_close() {
        for i in 0..1000 {
            let t = i as f32 / 1000.0;
            assert!((sine01(t) - (t * TAU).sin()).abs() < 2e-3, "{t}");
            let x = t * 6.0 - 3.0;
            assert!((soft_clip(x) - x.tanh()).abs() < 0.03, "{x}");
        }
    }

    #[test]
    fn notes_hold_and_rest() {
        let n = notes("4 . . 2 - 0 . -1+");
        assert_eq!(n, vec![(0, "4", 3), (3, "2", 1), (5, "0", 2), (7, "-1+", 1)]);
    }

    #[test]
    fn adsr_releases_to_silence() {
        let e = Adsr { attack: 0.01, decay: 0.1, sustain: 0.5, release: 0.1 };
        let gate = samples(0.2);
        let c = e.curve(gate);
        assert_eq!(c.len(), e.len(gate));
        assert!(c[gate - 1] > 0.45);
        assert!(c.last().unwrap().abs() < 1e-3);
    }

    #[test]
    fn pump_recovers_between_kicks() {
        let dip = samples(0.004);
        let env = pump_env(40000, &[0, 20000], 0.5, 10000.0 / SR);
        assert_eq!(env[0], 1.0, "eases into the duck");
        assert!(env[dip] < 0.55);
        assert!((env[15000] - 1.0).abs() < 1e-6);
        assert!(env[20000 + dip] < 0.55);
    }

    #[test]
    fn wrapped_notes_come_round_to_the_start() {
        let mut dst = vec![0.0; 4];
        add_wrapped(&mut dst, &[1.0, 2.0, 3.0], 2);
        assert_eq!(dst, vec![3.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn stings_are_short_and_end_silent() {
        for s in [victory_sting(), defeat_sting()] {
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!((peak - MIX_PEAK).abs() < 1e-3);
            assert!(s.len() < RATE as usize * 4, "{} s", s.len() as f32 / SR);
            assert!(s.last().unwrap().abs() < 1e-4);
        }
    }
}
