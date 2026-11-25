//! The soundtrack, written out as data for `music`. All original. Every song is
//! an 8-bar loop; melodies are scale degrees (see `music::Song`).

use super::music::{Arp, BassVoice, Chord, LeadKind, LeadVoice, PadVoice, STEMS, Song};
use super::synth::Wave;

const MINOR: &[i32] = &[0, 2, 3, 5, 7, 8, 10];
const MAJOR: &[i32] = &[0, 2, 4, 5, 7, 9, 11];
const DORIAN: &[i32] = &[0, 2, 3, 5, 7, 9, 10];
const HARMONIC_MINOR: &[i32] = &[0, 2, 3, 5, 7, 8, 11];

const MIN: &[i32] = &[0, 3, 7];
const MAJ: &[i32] = &[0, 4, 7];
const MIN7: &[i32] = &[0, 3, 7, 10];
const MAJ7: &[i32] = &[0, 4, 7, 11];
const DOM7: &[i32] = &[0, 4, 7, 10];
const SUS7: &[i32] = &[0, 5, 7, 10];

const FOUR_ON_FLOOR: &str = "X...x...X...x...";
const BACKBEAT: &str = "....X.......X...";

/// Level of each stem in the mix: pad, bass, drums, lead, hype, tension.
const GAME_MIX: [f32; STEMS] = [0.55, 0.9, 1.0, 0.75, 0.55, 0.6];

/// Main menu: an unhurried lo-fi loop with electric piano. Doesn't react, so it's one stem.
pub static LOBBY: Song = Song {
    bpm: 88.0,
    tonic: 50, // D3
    scale: MAJOR,
    chords: [
        Chord(0, MAJ7),
        Chord(-3, MIN7),
        Chord(5, MAJ7),
        Chord(-5, SUS7),
        Chord(0, MAJ7),
        Chord(-3, MIN7),
        Chord(2, MIN7),
        Chord(-5, DOM7),
    ],
    swing: 0.16,
    lead: [
        "4 . . . . . 2 . . . 1 . . . . .",
        "- - - - 0 . . . 1 . . . 2 . . .",
        "6 . . . . . 4 . . . 2 . . . . .",
        "3 . . . . . 1 . . . . . - - - -",
        "7 . . . . . 4 . . . 6 . . . . .",
        "8 . . . 7 . . . 5 . . . 4 . . .",
        "4 . . . . . 3 . . . 1 . . . . .",
        "2 . . . . . 1 . . . -1 . . . . .",
    ],
    lead_voice: LeadVoice { kind: LeadKind::Keys, wave: Wave::Sine, detune: 0.0, cutoff: 3000.0, vibrato: 0.0, octave: 1, echo: 0.3 },
    bass: &["r . . . . . . . r . . . f . . .", "r . . . . . . . r . . . o . f ."],
    bass_voice: BassVoice { wave: Wave::Triangle, cutoff: 3.0, env: 2.0, sub: 0.8, octave: 2 },
    arp: Some(Arp { pattern: &[0, 2, 1, 3, 2, 4, 3, 5], every: 2, wave: Wave::Triangle, octave: 1, cutoff: 1500.0 }),
    pad_voice: PadVoice { cutoff: 900.0, detune: 10.0 },
    kick: &["x.......x.x.....", "x.......x.....x."],
    snare: &["....x.......x...", "....x.......x..g"],
    hat: &["x.g.x.g.x.g.x.g.", "x.g.x.g.x.g.x.x."],
    fill: "",
    gains: [0.7, 0.8, 0.6, 0.8, 0.25, 0.0],
    pump: 0.2,
    mono: true,
};

/// Classic: bright, driving chiptune in A minor.
pub static STACK_ATTACK: Song = Song {
    bpm: 128.0,
    tonic: 57, // A3
    scale: MINOR,
    chords: [
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(3, MAJ),
        Chord(-2, MAJ),
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(-2, MAJ),
        Chord(-5, DOM7),
    ],
    swing: 0.0,
    lead: [
        "4 . . 4 7 . 4 . 6 . 4 . 3 . 2 .",
        "0 . . . 2 . 0 . -2 . . . - - 0 2",
        "4 . . 4 7 . 4 . 9 . 7 . 6 . 4 .",
        "6 . . . 5 . 4 . 3 . . . - - - -",
        "4 . . 4 7 . 4 . 6 . 4 . 3 . 2 .",
        "0 . . . 2 . 3 . 4 . 5 . 4 . 2 .",
        "3 . 4 . 3 . 2 . 1 . . . -1 . . .",
        "1 . . . . . -1+ . 1 . . . - - - -",
    ],
    lead_voice: LeadVoice { kind: LeadKind::Synth, wave: Wave::Square, detune: 7.0, cutoff: 2800.0, vibrato: 0.008, octave: 1, echo: 0.25 },
    bass: &["r . o . r . o . r . o . r . o ."],
    bass_voice: BassVoice { wave: Wave::Square, cutoff: 3.0, env: 8.0, sub: 0.5, octave: 2 },
    arp: Some(Arp { pattern: &[0, 1, 2, 3, 4, 3, 2, 1], every: 1, wave: Wave::Square, octave: 1, cutoff: 2500.0 }),
    pad_voice: PadVoice { cutoff: 1400.0, detune: 9.0 },
    kick: &[FOUR_ON_FLOOR, "X...x...X...x.x."],
    snare: &[BACKBEAT],
    hat: &["x.x.x.x.x.x.x.xo"],
    fill: "....X...x.x.xxXX",
    gains: GAME_MIX,
    pump: 0.35,
    mono: false,
};

/// Battler: heavier, faster, saw bass in 16ths, E minor.
pub static MONSTER_RUSH: Song = Song {
    bpm: 140.0,
    tonic: 52, // E3
    scale: MINOR,
    chords: [
        Chord(0, MIN),
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(-2, MAJ),
        Chord(0, MIN),
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(-5, DOM7),
    ],
    swing: 0.0,
    lead: [
        "7 . 7 . 6 . 7 . - 4 . . 5 . 4 .",
        "2 . . . 3 . 4 . - - 2 . 0 . . .",
        "5 . 5 . 4 . 5 . - 7 . . 9 . 7 .",
        "8 . . . 6 . 5 . 4 . . . 3 . . .",
        "7 . 7 . 6 . 7 . - 4 . . 5 . 4 .",
        "2 . 4 . 7 . 9 . 8 . 7 . 4 . . .",
        "9 . . . 7 . . . 5 . 4 . 2 . . .",
        "4 . . . 3 . 1 . -1+ . . . 1 . 4 .",
    ],
    lead_voice: LeadVoice { kind: LeadKind::Synth, wave: Wave::Saw, detune: 12.0, cutoff: 2400.0, vibrato: 0.01, octave: 1, echo: 0.2 },
    bass: &["r - r r r - r o r - r r r - o r"],
    bass_voice: BassVoice { wave: Wave::Saw, cutoff: 2.5, env: 9.0, sub: 0.6, octave: 2 },
    arp: Some(Arp { pattern: &[0, 2, 4, 2, 3, 1, 2, 0], every: 1, wave: Wave::Saw, octave: 1, cutoff: 2000.0 }),
    pad_voice: PadVoice { cutoff: 1100.0, detune: 14.0 },
    kick: &[FOUR_ON_FLOOR, "X...x...X...x..x"],
    snare: &["....X.......X...", "....X.......X.g."],
    hat: &["xgxgxgxgxgxgxgxg", "xgxgxgxgxgxgxgxo"],
    fill: "....X...X.x.XxXX",
    gains: GAME_MIX,
    pump: 0.4,
    mono: false,
};

/// Shadow 1v1 against the gentler shadows: a funky D dorian groove.
pub static SPARRING: Song = Song {
    bpm: 112.0,
    tonic: 50, // D3
    scale: DORIAN,
    chords: [
        Chord(0, MIN7),
        Chord(5, DOM7),
        Chord(-4, MAJ7),
        Chord(-5, DOM7),
        Chord(0, MIN7),
        Chord(5, DOM7),
        Chord(-4, MAJ7),
        Chord(-5, DOM7),
    ],
    swing: 0.12,
    lead: [
        "4 . . 6 . . 7 . 6 . 4 . - - 2 .",
        "3 . . . 5 . 4 . 3 . 2 . 3 . . .",
        "2 . . 4 . . 6 . 4 . . . 2 . 0 .",
        "4 . . . 3 . 1 . -1+ . . . 1 . . .",
        "4 . . 6 . . 7 . 6 . 4 . - - 2 .",
        "3 . . . 5 . 7 . 8 . 7 . 5 . . .",
        "6 . . . 4 . 2 . 4 . . . 1 . 0 .",
        "1 . . . . . . . -1+ . . . - - - -",
    ],
    lead_voice: LeadVoice { kind: LeadKind::Keys, wave: Wave::Sine, detune: 0.0, cutoff: 3000.0, vibrato: 0.0, octave: 1, echo: 0.3 },
    bass: &["r . - r - - o . - r - f . - o .", "r . - r - - o . - r - f . s . ."],
    bass_voice: BassVoice { wave: Wave::Square, cutoff: 2.0, env: 6.0, sub: 0.7, octave: 2 },
    arp: Some(Arp { pattern: &[0, 1, 2, 3, 2, 1], every: 2, wave: Wave::Triangle, octave: 1, cutoff: 1800.0 }),
    pad_voice: PadVoice { cutoff: 1200.0, detune: 10.0 },
    kick: &["X.....x...x.....", "X.....x...x...x."],
    snare: &["....X..g....X.g."],
    hat: &["x.x.x.x.x.x.x.o."],
    fill: "....X..g..x.X.xx",
    gains: GAME_MIX,
    pump: 0.25,
    mono: false,
};

/// Shadow 1v1 against the mid shadows: electro in G minor.
pub static RIVAL: Song = Song {
    bpm: 132.0,
    tonic: 55, // G3
    scale: MINOR,
    chords: [
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(3, MAJ),
        Chord(-2, MAJ),
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(5, MIN),
        Chord(-5, MAJ),
    ],
    swing: 0.0,
    lead: [
        "7 . . 4 . . 7 . 8 . 9 . 8 . 7 .",
        "5 . . . 4 . . . 2 . . . 4 . . .",
        "4 . . 2 . . 4 . 6 . 7 . 6 . 4 .",
        "6 . . . 5 . 4 . 3 . . . - - - -",
        "7 . . 4 . . 7 . 8 . 9 . 8 . 7 .",
        "9 . . . 8 . 7 . 5 . . . 4 . 5 .",
        "3 . . . 5 . 7 . 9 . . . 8 . 7 .",
        "6+ . . . 4 . . . 1 . . . -1+ . . .",
    ],
    lead_voice: LeadVoice { kind: LeadKind::Synth, wave: Wave::Square, detune: 10.0, cutoff: 2600.0, vibrato: 0.01, octave: 1, echo: 0.3 },
    bass: &["r . r o - r r . r . r o - r o r"],
    bass_voice: BassVoice { wave: Wave::Saw, cutoff: 2.5, env: 7.0, sub: 0.6, octave: 2 },
    arp: Some(Arp { pattern: &[0, 1, 2, 4, 3, 2, 1, 2], every: 1, wave: Wave::Square, octave: 1, cutoff: 2200.0 }),
    pad_voice: PadVoice { cutoff: 1300.0, detune: 12.0 },
    kick: &[FOUR_ON_FLOOR],
    snare: &[BACKBEAT, "....X.......X..g"],
    hat: &["..x...x...x...x.", "..x...x...x.x.xo"],
    fill: "....X...X...XxXX",
    gains: GAME_MIX,
    pump: 0.45,
    mono: false,
};

/// Shadow 1v1 against the hardest shadows: fast, dark, harmonic C minor.
pub static NIGHTMARE: Song = Song {
    bpm: 160.0,
    tonic: 48, // C3
    scale: HARMONIC_MINOR,
    chords: [
        Chord(0, MIN),
        Chord(0, MIN),
        Chord(-4, MAJ),
        Chord(-4, MAJ),
        Chord(5, MIN),
        Chord(5, MIN),
        Chord(-5, DOM7),
        Chord(-5, DOM7),
    ],
    swing: 0.0,
    lead: [
        "7 . 7 6 7 . 4 . 7 . 9 . 7 . 4 .",
        "2 . 4 . 7 . 4 . 2 . 1 . 0 . -1 .",
        "0 . 0 -1 0 . 5 . 7 . 5 . 2 . 0 .",
        "5 . . . 4 . . . 2 . . . 0 . 2 .",
        "3 . 3 2 3 . 7 . 9 . 7 . 5 . 3 .",
        "5 . . . 7 . . . 10 . . . 9 . 7 .",
        "6 . 6 4 6 . 8 . 11 . 8 . 6 . 4 .",
        "13 . . . 11 . . . 8 . . . 6 . . .",
    ],
    lead_voice: LeadVoice { kind: LeadKind::Synth, wave: Wave::Saw, detune: 15.0, cutoff: 2600.0, vibrato: 0.012, octave: 1, echo: 0.2 },
    bass: &["r r o r r r o r r r o r r o r o"],
    bass_voice: BassVoice { wave: Wave::Saw, cutoff: 2.0, env: 10.0, sub: 0.7, octave: 2 },
    arp: Some(Arp { pattern: &[0, 1, 2, 3, 4, 5, 4, 3], every: 1, wave: Wave::Saw, octave: 1, cutoff: 2400.0 }),
    pad_voice: PadVoice { cutoff: 1000.0, detune: 16.0 },
    kick: &["X.....x.X.x.....", "X.....x.X.x...x."],
    snare: &["....X.......X...", "....X..g....X.gg"],
    hat: &["xgxgxgxgxgxgxgxg"],
    fill: "....X.X.XxXxXXXX",
    gains: GAME_MIX,
    pump: 0.45,
    mono: false,
};

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::super::mood::{Ending, Mood, Track};
    use super::super::music::{self, BARS, MIX_PEAK, STEPS_PER_BAR, rms};
    use super::super::player::{MUSIC_VOLUME, ease, targets};
    use super::super::synth::{RATE, encode_wav};
    use super::*;

    const TRACKS: [Track; 6] = [
        Track::Menu,
        Track::Classic,
        Track::Battler,
        Track::Shadow(0),
        Track::Shadow(1),
        Track::Shadow(2),
    ];

    fn render_all() -> Vec<(Track, Vec<Vec<f32>>)> {
        TRACKS.iter().map(|&t| (t, music::render(t.song()))).collect()
    }

    #[test]
    fn patterns_are_sixteen_steps_and_parse() {
        for t in TRACKS {
            let s = t.song();
            for (bar, line) in s.lead.iter().enumerate() {
                let toks: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(toks.len(), STEPS_PER_BAR, "{t:?} lead bar {bar}");
                for tok in &toks {
                    let n = tok.trim_end_matches('+');
                    assert!(*tok == "." || *tok == "-" || n.parse::<i32>().is_ok(), "{t:?} lead token {tok}");
                }
            }
            assert_ne!(s.lead[0].split_whitespace().next(), Some("."), "{t:?}: the loop can't open on a hold");
            for line in s.bass {
                let toks: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(toks.len(), STEPS_PER_BAR, "{t:?} bass {line}");
                assert!(toks.iter().all(|k| ["r", "o", "f", "s", ".", "-"].contains(k)), "{t:?} bass {line}");
            }
            for line in s.kick.iter().chain(s.snare).chain(s.hat).chain([&s.fill]).filter(|l| !l.is_empty()) {
                assert_eq!(line.len(), STEPS_PER_BAR, "{t:?} drums {line}");
            }
        }
    }

    #[test]
    fn stems_loop_seamlessly_at_exact_lengths() {
        for (t, stems) in render_all() {
            let song = t.song();
            let n = song.loop_len();
            let expected = (BARS * STEPS_PER_BAR) as f64 * song.step_secs() as f64 * RATE as f64;
            assert!((n as f64 - expected).abs() <= 1.0, "{t:?}");
            assert_eq!(stems.len(), if song.mono { 1 } else { music::STEMS }, "{t:?}");
            for (k, s) in stems.iter().enumerate() {
                assert_eq!(s.len(), n, "{t:?} stem {k}: every stem the same length");
                assert!(rms(s) > 0.01, "{t:?} stem {k} is silent");
                // The jump across the seam is no bigger than a typical step inside the loop.
                let mut d: Vec<f32> = s.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
                d.sort_by(f32::total_cmp);
                let p999 = d[d.len() * 999 / 1000];
                let seam = (s[0] - s[n - 1]).abs();
                assert!(seam <= p999, "{t:?} stem {k}: seam {seam} vs p99.9 {p999}");
            }
            let peak = (0..n).map(|i| stems.iter().map(|s| s[i]).sum::<f32>().abs()).fold(0.0, f32::max);
            assert!(peak <= MIX_PEAK + 1e-3, "{t:?} full mix peaks at {peak}");
            assert!(peak > MIX_PEAK * 0.9, "{t:?} full mix is normalised");
        }
    }

    /// Energy of `x` at `freq` (Goertzel).
    fn tone(x: &[f32], freq: f32) -> f32 {
        let w = std::f32::consts::TAU * freq / RATE as f32;
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &v in x {
            let s0 = v + 2.0 * w.cos() * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2
    }

    #[test]
    fn melodies_play_the_written_notes() {
        // Each song's first lead note, against the semitones either side.
        for t in TRACKS.into_iter().filter(|t| !t.song().mono) {
            let s = t.song();
            let first: i32 = s.lead[0].split_whitespace().next().unwrap().parse().unwrap();
            let n = s.scale.len() as i32;
            let midi = s.tonic + 12 * s.lead_voice.octave + s.scale[first.rem_euclid(n) as usize] + 12 * first.div_euclid(n);
            let stems = music::render(s);
            let step = (s.step_secs() * RATE as f32) as usize;
            let x = &stems[music::LEAD][step / 4..step];
            let at = |m: i32| tone(x, music::midi_hz(m as f32));
            // (Echoes of the loop's last note ring here too, so not by a huge margin.)
            let (on, below, above) = (at(midi), at(midi - 1), at(midi + 1));
            assert!(on > 1.5 * below && on > 1.5 * above, "{t:?} opens on MIDI {midi}: {on} vs {below} / {above}");
        }
    }

    #[test]
    fn rendering_is_deterministic_and_steppable() {
        let whole = music::render(&SPARRING);
        let mut r = music::Render::new(&SPARRING);
        let mut frames = 0;
        while !r.step(60_000) {
            frames += 1;
        }
        assert!(frames > 10, "work is spread over frames ({frames})");
        assert_eq!(r.finish(), whole);
    }

    #[test]
    fn music_sits_under_the_effects() {
        let sfx = super::super::synthesize();
        // A single-line clear at the volume it plays at (0.6) through the SFX master (0.8).
        let clear = rms(&sfx.clear[0]) * 0.6 * 0.8;
        for (t, stems) in render_all() {
            let n = stems[0].len();
            let mood = Mood { intensity: 1.0, ..Mood::menu() };
            let vol = targets(&Mood { track: t, ..mood }, 0.0);
            let mix: Vec<f32> = (0..n).map(|i| stems.iter().zip(vol).map(|(s, v)| s[i] * v).sum::<f32>() * MUSIC_VOLUME).collect();
            let music_rms = rms(&mix);
            assert!(music_rms > 0.02, "{t:?} audible: {music_rms}");
            assert!(music_rms < clear, "{t:?}: music {music_rms} vs a clear {clear}");
        }
    }

    #[test]
    fn stems_react_to_the_mood() {
        let calm = Mood { track: Track::Classic, ..Mood::menu() };
        let hot = Mood { intensity: 1.0, ..calm };
        let danger = Mood { danger: 1.0, ..calm };
        let (c, h, d) = (targets(&calm, 0.0), targets(&hot, 0.0), targets(&danger, 0.0));
        assert_eq!(c[music::HYPE], 0.0, "no hype layer while cruising");
        assert!(h[music::HYPE] > 0.9 && h[music::LEAD] > c[music::LEAD] && h[music::DRUMS] > c[music::DRUMS]);
        assert_eq!(c[music::TENSION], 0.0);
        assert!(d[music::TENSION] > 0.9 && d[music::PAD] < c[music::PAD]);
        let paused = targets(&Mood { paused: true, ..hot }, 0.0);
        assert!(paused.iter().sum::<f32>() < c.iter().sum::<f32>() * 0.5, "paused ducks");
        assert_eq!(paused[music::DRUMS], 0.0);
        let held = targets(&Mood { holding: true, ..calm }, 0.0);
        assert!(held[music::DRUMS] == 0.0 && held[music::PAD] > 0.0, "countdown: pad and bass only");
        let ended = Mood { ended: Some(Ending::Won), ..hot };
        assert_eq!(targets(&ended, 0.5).iter().sum::<f32>(), 0.0, "the sting plays alone");
        assert!(targets(&ended, 3.0)[music::PAD] > 0.0, "then the pad returns");
    }

    /// Native synthesis time, per track. Only asserted in release (the shipped and tested build).
    #[test]
    fn synthesis_is_fast() {
        let start = Instant::now();
        let stings = music::victory_sting().len() + music::defeat_sting().len();
        println!("stings: {:.1} s of audio in {:.3} s", stings as f32 / RATE as f32, start.elapsed().as_secs_f32());
        let mut secs = 0.0;
        let mut worst = 0.0f32;
        for t in TRACKS {
            let one = Instant::now();
            let stems = music::render(t.song());
            let len = stems[0].len() as f32 / RATE as f32;
            secs += stems.len() as f32 * len;
            let took = one.elapsed().as_secs_f32();
            worst = worst.max(took);
            println!("{t:?}: {} x {len:.1} s stems in {took:.3} s", stems.len());
        }
        let took = start.elapsed().as_secs_f32();
        println!("all music: {secs:.0} s of stems + stings in {took:.3} s");
        // In game a track renders a slice per frame: no slice may stall a frame.
        let mut r = music::Render::new(&SPARRING);
        let mut slice = 0.0f32;
        loop {
            let t = Instant::now();
            let done = r.step(60_000);
            slice = slice.max(t.elapsed().as_secs_f32());
            if done {
                break;
            }
        }
        println!("longest render slice: {:.1} ms", slice * 1000.0);
        #[cfg(not(debug_assertions))]
        assert!(slice < 0.03, "a render slice took {slice} s");
        // Generous, since tests run in parallel: a track is normally ~0.1 s.
        #[cfg(not(debug_assertions))]
        assert!(worst < 0.75, "a track took {worst} s");
    }

    /// `BBMUSIC_DIR=/tmp/bbmusic cargo test --release dump_wavs -- --nocapture` writes
    /// every stem, a full mix per track and the stings, for listening.
    #[test]
    fn dump_wavs() {
        let Ok(dir) = std::env::var("BBMUSIC_DIR") else { return };
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, s: &[f32]| std::fs::write(format!("{dir}/{name}.wav"), encode_wav(s)).unwrap();
        let names = ["pad", "bass", "drums", "lead", "hype", "tension"];
        for (t, stems) in render_all() {
            let tn = format!("{t:?}").to_lowercase().replace(['(', ')'], "");
            let n = stems[0].len();
            let mix: Vec<f32> = (0..n).map(|i| stems.iter().map(|s| s[i]).sum()).collect();
            // Twice round, so the seam can be heard.
            write(&format!("{tn}-mix-x2"), &[mix.as_slice(), mix.as_slice()].concat());
            if stems.len() > 1 {
                for (k, s) in stems.iter().enumerate() {
                    write(&format!("{tn}-{}", names[k]), s);
                }
                let calm = targets(&Mood { track: t, ..Mood::menu() }, 0.0);
                let calm_mix: Vec<f32> = (0..n).map(|i| stems.iter().zip(calm).map(|(s, v)| s[i] * v).sum()).collect();
                write(&format!("{tn}-calm"), &calm_mix);
            }
        }
        write("sting-victory", &music::victory_sting());
        write("sting-defeat", &music::defeat_sting());

        // A scripted minute of a 1v1, mixed the way the player mixes it live: countdown,
        // cruising, a combo streak, garbage flying, danger, a pause, then the win.
        let stems = music::render(Track::Shadow(1).song());
        let n = stems[0].len();
        let base = Mood { track: Track::Shadow(1), ..Mood::menu() };
        let script = |t: f32| -> Mood {
            match t {
                t if t < 3.0 => Mood { holding: true, ..base },
                t if t < 15.0 => base,
                t if t < 27.0 => Mood { intensity: ((t - 15.0) / 6.0).min(1.0), ..base },
                t if t < 33.0 => Mood { intensity: 0.2, ..base },
                t if t < 43.0 => Mood { intensity: 0.4, danger: ((t - 33.0) / 5.0).min(1.0), ..base },
                t if t < 47.0 => Mood { paused: true, danger: 0.8, ..base },
                t if t < 52.0 => Mood { intensity: 0.7, danger: 0.3, ..base },
                _ => Mood { ended: Some(Ending::Won), ..base },
            }
        };
        let sting = music::victory_sting();
        let frame = RATE as usize / 60;
        let total = 58 * RATE as usize;
        let mut vol = [0.0f32; music::STEMS];
        let mut out = Vec::with_capacity(total);
        let mut end_at = None;
        for f in 0..total / frame {
            let t = (f * frame) as f32 / RATE as f32;
            let mood = script(t);
            if mood.ended.is_some() && end_at.is_none() {
                end_at = Some(f * frame);
            }
            let since = end_at.map_or(0.0, |e| (f * frame - e) as f32 / RATE as f32);
            ease(&mut vol, &targets(&mood, since), 1.0 / 60.0, mood.ended.is_some() && since < 1.0);
            for i in f * frame..(f + 1) * frame {
                let mut x: f32 = stems.iter().zip(vol).map(|(s, v)| s[i % n] * v).sum::<f32>() * MUSIC_VOLUME;
                if let Some(e) = end_at {
                    x += sting.get(i - e).copied().unwrap_or(0.0) * MUSIC_VOLUME * 0.9;
                }
                out.push(x);
            }
        }
        // Louder than in game (where it sits under the effects), for listening.
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        out.iter_mut().for_each(|x| *x *= MIX_PEAK / peak);
        write("demo-shadow1-session", &out);
    }
}
