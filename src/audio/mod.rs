//! Sound effects. Everything is synthesised at startup (see `synth`), so there
//! are no audio asset files. Sounds are driven by the events the effects layer
//! has *presented*, so they line up with what's on screen (e.g. the hard-drop
//! thud plays when the falling piece lands, not when the logic locked it).

mod synth;

use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};

use crate::bag::Rng;
use crate::game::Event;
use synth::{Voice, Wave, bell, encode_wav, lowpass, mix, noise, normalize, note};

/// Combo chimes climb a major pentatonic scale, one step per combo.
pub const COMBO_STEPS: usize = 12;
const PENTATONIC: [i32; COMBO_STEPS] = [0, 2, 4, 7, 9, 12, 14, 16, 19, 21, 24, 26];
const C4: f32 = 261.63;
const C5: f32 = 523.25;
const C6: f32 = 1046.5;
const PEAK: f32 = 0.9;
const MASTER_VOLUME: f32 = 0.8;

/// Every sound effect, generic over the sample representation so the pure
/// synthesis step (`Bank<Vec<f32>>`) can be unit tested without an audio device.
pub struct Bank<T> {
    shift: T,
    rotate: T,
    soft_drop: T,
    hold: T,
    lock: T,
    hard_drop: T,
    /// Indexed by lines cleared - 1.
    clear: [T; 4],
    combo: [T; COMBO_STEPS],
    t_spin: T,
    perfect: T,
    level_up: T,
    game_over: T,
    garbage_rise: T,
    garbage_warn: T,
}

impl<T> Bank<T> {
    fn try_map<U>(self, mut f: impl FnMut(T) -> Option<U>) -> Option<Bank<U>> {
        let map_array = |items: Vec<T>, f: &mut dyn FnMut(T) -> Option<U>| -> Option<Vec<U>> {
            items.into_iter().map(f).collect()
        };
        Some(Bank {
            shift: f(self.shift)?,
            rotate: f(self.rotate)?,
            soft_drop: f(self.soft_drop)?,
            hold: f(self.hold)?,
            lock: f(self.lock)?,
            hard_drop: f(self.hard_drop)?,
            clear: map_array(self.clear.into(), &mut f)?.try_into().ok()?,
            combo: map_array(self.combo.into(), &mut f)?.try_into().ok()?,
            t_spin: f(self.t_spin)?,
            perfect: f(self.perfect)?,
            level_up: f(self.level_up)?,
            game_over: f(self.game_over)?,
            garbage_rise: f(self.garbage_rise)?,
            garbage_warn: f(self.garbage_warn)?,
        })
    }
}

fn finish(mut buf: Vec<f32>) -> Vec<f32> {
    normalize(&mut buf, PEAK);
    buf
}

/// Rising or falling run of notes, `spacing` seconds apart.
fn arpeggio(wave: Wave, base: f32, steps: &[i32], spacing: f32, dur: f32, decay: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, &s) in steps.iter().enumerate() {
        let v = Voice::new(wave, note(base, s), dur).decay(decay);
        mix(&mut out, &v.render(), i as f32 * spacing, 1.0);
    }
    out
}

pub fn combo_chime_freq(step: usize) -> f32 {
    note(C5, PENTATONIC[step.min(COMBO_STEPS - 1)])
}

fn line_clear(rng: &mut Rng, lines: usize) -> Vec<f32> {
    let l = lines as f32;
    let mut out = noise(rng, 0.18 + 0.06 * l, 1500.0 * l, 300.0, 14.0 - 2.0 * l, 0.8);
    let pop = Voice::new(Wave::Sine, 300.0 + 80.0 * l, 0.12)
        .sweep_to(600.0 + 150.0 * l)
        .decay(25.0);
    mix(&mut out, &pop.render(), 0.0, 0.6);
    if lines >= 3 {
        let thump = Voice::new(Wave::Sine, 110.0, 0.28).sweep_to(40.0).decay(10.0);
        mix(&mut out, &thump.render(), 0.0, 0.9);
    }
    if lines >= 4 {
        let sparkle = arpeggio(Wave::Triangle, C6, &[0, 4, 7, 12], 0.045, 0.14, 18.0);
        mix(&mut out, &sparkle, 0.03, 0.5);
    }
    finish(out)
}

/// Pure synthesis of every effect. Deterministic.
pub fn synthesize() -> Bank<Vec<f32>> {
    let mut rng = Rng::new(0xB10C_B4A1);

    let shift = finish(Voice::new(Wave::Square, 1400.0, 0.02).decay(180.0).render());

    let mut rotate = Voice::new(Wave::Triangle, 660.0, 0.05).sweep_to(990.0).decay(50.0).render();
    mix(&mut rotate, &Voice::new(Wave::Sine, 1320.0, 0.03).decay(120.0).render(), 0.0, 0.3);

    let soft_drop = finish(noise(&mut rng, 0.012, 3000.0, 3000.0, 200.0, 1.0));

    let mut hold = noise(&mut rng, 0.14, 800.0, 5000.0, 12.0, 1.0);
    let hold_tone = Voice::new(Wave::Sine, 330.0, 0.12).sweep_to(660.0).attack(0.03).decay(15.0);
    mix(&mut hold, &hold_tone.render(), 0.0, 0.4);

    let mut lock = Voice::new(Wave::Sine, 220.0, 0.06).sweep_to(110.0).decay(50.0).render();
    mix(&mut lock, &noise(&mut rng, 0.015, 2500.0, 2500.0, 300.0, 1.0), 0.0, 0.6);

    let mut hard_drop = Voice::new(Wave::Sine, 160.0, 0.16).sweep_to(45.0).decay(18.0).render();
    mix(&mut hard_drop, &noise(&mut rng, 0.05, 1800.0, 400.0, 60.0, 1.0), 0.0, 0.8);
    let mut grit = Voice::new(Wave::Square, 80.0, 0.08).sweep_to(40.0).decay(40.0).render();
    lowpass(&mut grit, 900.0);
    mix(&mut hard_drop, &grit, 0.0, 0.25);

    let clear = [1, 2, 3, 4].map(|l| line_clear(&mut rng, l));

    let combo = std::array::from_fn(|step| {
        let f = combo_chime_freq(step);
        let mut chime = bell(f, 0.6, 1.0);
        if step >= COMBO_STEPS / 2 {
            // High combos get an octave shimmer on top.
            mix(&mut chime, &bell(f * 2.0, 0.4, 0.3), 0.02, 1.0);
        }
        finish(chime)
    });

    let mut t_spin = Voice::new(Wave::Saw, 220.0, 0.22)
        .sweep_to(880.0)
        .vibrato(18.0, 0.04)
        .decay(8.0)
        .render();
    mix(&mut t_spin, &Voice::new(Wave::Square, 440.0, 0.18).sweep_to(1760.0).decay(12.0).render(), 0.0, 0.3);
    lowpass(&mut t_spin, 3500.0);

    let mut perfect = arpeggio(Wave::Square, C5, &[0, 4, 7, 12, 16, 19, 24], 0.07, 0.25, 6.0);
    lowpass(&mut perfect, 4000.0);
    for s in [0, 4, 7] {
        mix(&mut perfect, &bell(note(C6, s), 0.9, 0.5), 0.5, 1.0);
    }

    let level_up = arpeggio(Wave::Triangle, C5, &[0, 4, 7, 12], 0.06, 0.15, 14.0);

    let mut game_over = arpeggio(Wave::Triangle, C4, &[12, 7, 3, 0, -5], 0.14, 0.3, 5.0);
    mix(&mut game_over, &noise(&mut rng, 0.8, 300.0, 120.0, 3.0, 1.0), 0.0, 0.5);

    let mut garbage_rise = noise(&mut rng, 0.3, 400.0, 120.0, 7.0, 1.0);
    let mut grind = Voice::new(Wave::Square, 70.0, 0.25).sweep_to(55.0).decay(9.0).render();
    lowpass(&mut grind, 500.0);
    mix(&mut garbage_rise, &grind, 0.0, 0.5);

    // Two-tone alarm blip when garbage is queued against you.
    let mut garbage_warn = Voice::new(Wave::Square, 880.0, 0.06).decay(20.0).render();
    mix(&mut garbage_warn, &Voice::new(Wave::Square, 660.0, 0.08).decay(20.0).render(), 0.07, 1.0);
    lowpass(&mut garbage_warn, 2500.0);

    Bank {
        shift,
        rotate: finish(rotate),
        soft_drop,
        hold: finish(hold),
        lock: finish(lock),
        hard_drop: finish(hard_drop),
        clear,
        combo,
        t_spin: finish(t_spin),
        perfect: finish(perfect),
        level_up: finish(level_up),
        game_over: finish(game_over),
        garbage_rise: finish(garbage_rise),
        garbage_warn: finish(garbage_warn),
    }
}

pub struct Audio {
    /// `None` if any sound failed to load; the game then runs silently.
    bank: Option<Bank<Sound>>,
    muted: bool,
}

impl Audio {
    pub async fn load() -> Self {
        let samples = synthesize();
        // load_sound_from_bytes is async (on wasm the browser decodes in the
        // background), so collect the encoded WAVs first, then await each.
        let mut encoded = Vec::new();
        let wavs = samples.try_map(|s| {
            encoded.push(encode_wav(&s));
            Some(encoded.len() - 1)
        });
        let mut sounds = Vec::with_capacity(encoded.len());
        for wav in &encoded {
            match load_sound_from_bytes(wav).await {
                Ok(s) => sounds.push(Some(s)),
                Err(_) => sounds.push(None),
            }
        }
        let bank = wavs.and_then(|w| w.try_map(|i| sounds[i].take()));
        Audio { bank, muted: false }
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
    }

    fn play(&self, pick: impl Fn(&Bank<Sound>) -> &Sound, volume: f32) {
        if let (Some(bank), false) = (&self.bank, self.muted) {
            play_sound(
                pick(bank),
                PlaySoundParams {
                    looped: false,
                    volume: volume * MASTER_VOLUME,
                },
            );
        }
    }

    /// Plays whatever a presented event sounds like.
    pub fn handle(&self, event: &Event) {
        match event {
            Event::Moved => self.play(|b| &b.shift, 0.25),
            Event::Rotated => self.play(|b| &b.rotate, 0.35),
            Event::SoftDropped => self.play(|b| &b.soft_drop, 0.15),
            Event::Held => self.play(|b| &b.hold, 0.4),
            Event::HardDropped { distance, .. } => {
                self.play(|b| &b.hard_drop, (0.55 + *distance as f32 * 0.025).min(1.0))
            }
            Event::Locked { .. } => self.play(|b| &b.lock, 0.35),
            Event::LinesCleared(c) => {
                let lines = c.lines() as usize;
                if lines > 0 {
                    self.play(|b| &b.clear[lines.min(4) - 1], 0.6);
                    let step = (c.combo as usize).min(COMBO_STEPS - 1);
                    self.play(|b| &b.combo[step], 0.55);
                }
                if c.t_spin {
                    self.play(|b| &b.t_spin, 0.6);
                }
                if c.perfect_clear {
                    self.play(|b| &b.perfect, 0.8);
                }
            }
            Event::LevelUp { .. } => self.play(|b| &b.level_up, 0.5),
            Event::GameOver => self.play(|b| &b.game_over, 0.7),
            Event::GarbageIncoming { .. } => self.play(|b| &b.garbage_warn, 0.35),
            Event::GarbageRose { lines } => {
                self.play(|b| &b.garbage_rise, (0.4 + *lines as f32 * 0.08).min(0.9))
            }
            Event::Spawned => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(bank: &Bank<Vec<f32>>) -> Vec<&Vec<f32>> {
        let mut v = vec![
            &bank.shift,
            &bank.rotate,
            &bank.soft_drop,
            &bank.hold,
            &bank.lock,
            &bank.hard_drop,
            &bank.t_spin,
            &bank.perfect,
            &bank.level_up,
            &bank.game_over,
            &bank.garbage_rise,
            &bank.garbage_warn,
        ];
        v.extend(bank.clear.iter());
        v.extend(bank.combo.iter());
        v
    }

    #[test]
    fn every_sound_is_audible_normalised_and_short() {
        let bank = synthesize();
        for s in all(&bank) {
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!((peak - PEAK).abs() < 1e-3, "peak {peak}");
            assert!(s.len() < synth::RATE as usize * 2, "longer than 2 s");
        }
    }

    #[test]
    fn combo_chimes_climb() {
        let freqs: Vec<f32> = (0..COMBO_STEPS).map(combo_chime_freq).collect();
        assert!(freqs.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(combo_chime_freq(99), freqs[COMBO_STEPS - 1], "caps at the top step");
    }

    #[test]
    fn bigger_clears_are_longer() {
        let bank = synthesize();
        assert!(bank.clear.windows(2).all(|w| w[1].len() > w[0].len()));
    }

    #[test]
    fn synthesis_is_deterministic() {
        assert_eq!(synthesize().clear[3], synthesize().clear[3]);
    }

    #[test]
    fn try_map_preserves_slots_and_short_circuits() {
        let bank = synthesize();
        let lens = bank.try_map(|s| Some(s.len())).unwrap();
        assert_eq!(lens.combo.len(), COMBO_STEPS);
        assert!(synthesize().try_map(|_| None::<()>).is_none());
    }
}
