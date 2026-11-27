//! Plays the music: renders the track a screen needs (a few milliseconds per
//! frame, so nothing stalls), starts all its stems in the same frame so they stay
//! locked together, and every frame eases each stem's volume towards what the
//! `Mood` asks for. Ends of matches get a short sting.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound, set_sound_volume, stop_sound};
use macroquad::time::get_time;

use super::mood::{Ending, Mood, Track};
use super::music::{self, BASS, DRUMS, HYPE, LEAD, PAD, Render, STEMS, TENSION};
use super::synth::encode_wav;

/// Music at full intensity, relative to the effects (which top out near 0.8).
pub const MUSIC_VOLUME: f32 = 0.3;
/// Rendering time allowed per frame, seconds.
const FRAME_BUDGET: f64 = 0.004;
/// Sample-voices per `Render::step` call between clock checks.
const STEP: usize = 15_000;

type Loading = Pin<Box<dyn Future<Output = Option<Sound>>>>;

/// Encodes and loads on first poll, so `prepare` can take the stems one per frame.
fn load(samples: Vec<f32>) -> Loading {
    Box::pin(async move { load_sound_from_bytes(&encode_wav(&samples)).await.ok() })
}

/// Polls a future once, without an executor (macroquad's loaders only wait for frames).
fn poll(f: &mut Loading) -> Poll<Option<Sound>> {
    f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}

enum Pending {
    Rendering(Box<Render>),
    /// Per stem: `None` while loading, then whether it loaded.
    Loading(Vec<Loading>, Vec<Option<Option<Sound>>>),
}

struct Loaded {
    track: Track,
    stems: Vec<Sound>,
    playing: bool,
    vol: [f32; STEMS],
}

impl Loaded {
    fn stop(&mut self) {
        if self.playing {
            self.stems.iter().for_each(stop_sound);
            self.playing = false;
        }
    }
}

/// Stem volumes (0..1) for a mood, before the master volume. `since_end` is the
/// seconds since the match ended.
pub fn targets(mood: &Mood, since_end: f32) -> [f32; STEMS] {
    let mut v = [0.0; STEMS];
    if mood.track == Track::Menu {
        v[0] = 0.8;
        return v;
    }
    let i = mood.intensity.clamp(0.0, 1.0);
    let d = mood.danger.clamp(0.0, 1.0);
    let smooth = |lo: f32, hi: f32, x: f32| {
        let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    v[PAD] = 0.85 - 0.35 * d;
    v[BASS] = 0.9;
    v[DRUMS] = 0.8 + 0.2 * i;
    v[LEAD] = (0.55 + 0.45 * i) * (1.0 - 0.4 * d);
    v[HYPE] = smooth(0.1, 0.55, i) * (1.0 - 0.5 * d);
    v[TENSION] = smooth(0.0, 0.6, d);
    if mood.ended.is_some() {
        // The sting has the floor; then just the pad, softly.
        v = [0.0; STEMS];
        if since_end > 2.5 {
            v[PAD] = 0.35;
        }
    } else if mood.paused {
        v = [0.4, 0.25, 0.0, 0.0, 0.0, 0.0];
    } else if mood.holding {
        v[DRUMS] = 0.0;
        v[LEAD] = 0.0;
        v[HYPE] = 0.0;
        v[TENSION] = 0.0;
    }
    v
}

/// Moves stem volumes towards `goal`: swelling in quickly, letting go slowly (the
/// hype layer slowest, so a combo's lift lingers); `cut` fades out fast (a switch
/// of track, or a match ending).
pub fn ease(vol: &mut [f32; STEMS], goal: &[f32; STEMS], dt: f32, cut: bool) {
    for (k, (v, &g)) in vol.iter_mut().zip(goal).enumerate() {
        let rate = if g > *v {
            3.0
        } else if cut {
            4.0
        } else if k == HYPE {
            0.5
        } else {
            1.2
        };
        *v += (g - *v) * (dt * rate).min(1.0);
    }
}

pub struct Player {
    menu: Option<Loaded>,
    game: Option<Loaded>,
    pending: Option<(Track, Pending)>,
    win: Option<Sound>,
    lose: Option<Sound>,
    /// Burst of energy from recent clears, decaying; adds to the mood's intensity.
    energy: f32,
    ended: Option<Ending>,
    since_end: f32,
}

impl Player {
    pub async fn new() -> Self {
        let win = load_sound_from_bytes(&encode_wav(&music::victory_sting())).await.ok();
        let lose = load_sound_from_bytes(&encode_wav(&music::defeat_sting())).await.ok();
        Player {
            menu: None,
            game: None,
            pending: None,
            win,
            lose,
            energy: 0.0,
            ended: None,
            since_end: 0.0,
        }
    }

    /// A clear or hit just landed: lift the music for a moment.
    pub fn bump(&mut self, amount: f32) {
        self.energy = (self.energy + amount).min(0.6);
    }

    fn slot(&mut self, track: Track) -> &mut Option<Loaded> {
        if track == Track::Menu { &mut self.menu } else { &mut self.game }
    }

    /// Call every frame. `volume` is the master music volume (0 when muted or off).
    pub fn update(&mut self, dt: f32, mood: &Mood, volume: f32) {
        let want = mood.track;
        self.energy = (self.energy - dt * 0.12).max(0.0);

        // Match end: play a sting once.
        if mood.ended != self.ended {
            self.ended = mood.ended;
            self.since_end = 0.0;
            if let Some(end) = mood.ended {
                let sting = if end == Ending::Won { &self.win } else { &self.lose };
                if let (Some(s), true) = (sting, volume > 0.0) {
                    play_sound(s, PlaySoundParams { looped: false, volume: volume * 0.9 });
                }
            }
        }
        self.since_end += dt;

        let ready = self.slot(want).as_ref().is_some_and(|l| l.track == want);
        if !ready {
            self.prepare(want);
        }

        let mut mood = *mood;
        mood.intensity = (mood.intensity + self.energy).min(1.0);
        let target = targets(&mood, self.since_end);
        let since_end = self.since_end;
        let others_quiet = [&self.menu, &self.game]
            .into_iter()
            .flatten()
            .filter(|l| l.track != want && l.playing)
            .all(|l| l.vol.iter().all(|&v| v < 0.01));

        for loaded in [&mut self.menu, &mut self.game].into_iter().flatten() {
            let active = loaded.track == want;
            if active && !loaded.playing && others_quiet {
                // Every stem in the same frame: they start on the same sample.
                for s in &loaded.stems {
                    play_sound(s, PlaySoundParams { looped: true, volume: 0.0 });
                }
                loaded.playing = true;
                loaded.vol = [0.0; STEMS];
            }
            if !loaded.playing {
                continue;
            }
            let goal = if active { target } else { [0.0; STEMS] };
            let cut = !active || (mood.ended.is_some() && since_end < 1.0);
            ease(&mut loaded.vol, &goal, dt, cut);
            for (sound, v) in loaded.stems.iter().zip(loaded.vol) {
                set_sound_volume(sound, v * volume);
            }
            if !active && loaded.vol.iter().all(|&v| v < 0.01) {
                loaded.stop();
            }
        }
    }

    /// While idling on a menu, gets the track that will probably be wanted next
    /// ready (the menu's own music comes first).
    pub fn prefetch(&mut self, track: Track) {
        let menu_playing = self.menu.as_ref().is_some_and(|l| l.playing);
        // Don't replace a game track that is still fading out.
        let game_fading = self.game.as_ref().is_some_and(|l| l.playing);
        let ready = self.slot(track).as_ref().is_some_and(|l| l.track == track);
        if menu_playing && !game_fading && !ready && track != Track::Menu {
            self.prepare(track);
        }
    }

    /// Advances (or starts) rendering and loading `track`.
    fn prepare(&mut self, track: Track) {
        if self.pending.as_ref().is_none_or(|(t, _)| *t != track) {
            self.pending = Some((track, Pending::Rendering(Box::new(Render::new(track.song())))));
        }
        let start = get_time();
        let Some((_, job)) = &mut self.pending else { return };
        if let Pending::Rendering(r) = job {
            let mut done = false;
            while !done && get_time() - start < FRAME_BUDGET {
                done = r.step(STEP);
            }
            if !done {
                return;
            }
            let Some((_, Pending::Rendering(r))) = self.pending.take() else { unreachable!() };
            let stems = r.finish();
            let count = stems.len();
            let loads = stems.into_iter().map(load).collect();
            self.pending = Some((track, Pending::Loading(loads, vec![None; count])));
            return;
        }
        let Some((_, Pending::Loading(loads, sounds))) = &mut self.pending else { return };
        // Stems still loading get polled; only one new one starts per frame
        // (encoding and decoding a stem takes a few milliseconds).
        let mut started = false;
        for (f, slot) in loads.iter_mut().zip(sounds.iter_mut()) {
            if slot.is_some() {
                continue;
            }
            if started {
                break;
            }
            started = true;
            if let Poll::Ready(s) = poll(f) {
                *slot = Some(s);
            }
        }
        if sounds.iter().all(Option::is_some) {
            let Some((_, Pending::Loading(_, sounds))) = self.pending.take() else { unreachable!() };
            // If any stem failed to load the track stays silent rather than half-playing.
            let stems: Vec<Sound> = sounds.into_iter().flatten().collect::<Option<_>>().unwrap_or_default();
            let slot = self.slot(track);
            if let Some(old) = slot.as_mut() {
                old.stop();
            }
            *slot = Some(Loaded { track, stems, playing: false, vol: [0.0; STEMS] });
        }
    }
}

#[cfg(test)]
mod chunk_timing {
    use super::*;
    /// The longest single piece of work music preparation does in one frame
    /// (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn longest_chunks() {
        for track in [Track::Classic, Track::Battler, Track::shadow(0, 8), Track::shadow(4, 8), Track::shadow(7, 8)] {
            let mut r = Render::new(track.song());
            let mut worst = 0.0f64;
            loop {
                let t = std::time::Instant::now();
                let done = r.step(STEP);
                worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
                if done {
                    break;
                }
            }
            let stems = r.finish();
            let t = std::time::Instant::now();
            let wav = encode_wav(&stems[0]);
            let enc = t.elapsed().as_secs_f64() * 1000.0;
            println!("{track:?}: worst step {worst:.1} ms, encode one stem {enc:.1} ms ({} KB)", wav.len() / 1024);
        }
    }
}
