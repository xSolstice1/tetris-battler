//! What the settings screen shows: tabs, their rows, and how each row reads and changes
//! the settings. Pure (no drawing, no input), so it's testable; `render.rs` draws it and
//! `app.rs` drives it.

use crate::fx::{Effects, Motion};
use crate::settings::{ARR_RANGE, Bind, DAS_RANGE, SOFT_RANGE, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Das,
    Arr,
    Soft,
    Ghost,
    Danger,
    Preset,
    Flight,
    Collapse,
    Glide,
    Shake,
    Clears,
    Scene,
    Fullscreen,
    Master,
    Sfx,
    Music,
    Heartbeat,
    Mute,
    Key(Bind),
    ResetKeys,
}

pub const TABS: [&str; 4] = ["GAMEPLAY", "VIDEO", "AUDIO", "CONTROLS"];

const GAMEPLAY: [Row; 5] = [Row::Das, Row::Arr, Row::Soft, Row::Ghost, Row::Danger];
const VIDEO: [Row; 8] = [Row::Preset, Row::Flight, Row::Collapse, Row::Glide, Row::Shake, Row::Clears, Row::Scene, Row::Fullscreen];
const AUDIO: [Row; 5] = [Row::Master, Row::Sfx, Row::Music, Row::Heartbeat, Row::Mute];
const CONTROLS: [Row; 8] = [
    Row::Key(Bind::Left),
    Row::Key(Bind::Right),
    Row::Key(Bind::SoftDrop),
    Row::Key(Bind::HardDrop),
    Row::Key(Bind::RotateCw),
    Row::Key(Bind::RotateCcw),
    Row::Key(Bind::Hold),
    Row::ResetKeys,
];

pub fn rows(tab: usize) -> &'static [Row] {
    match tab {
        0 => &GAMEPLAY,
        1 => &VIDEO,
        2 => &AUDIO,
        _ => &CONTROLS,
    }
}

/// How a row is edited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A number in `min..=max`, moved by `step`; `zero` names the 0 value (e.g. "instant").
    Slider { min: u32, max: u32, step: u32, unit: &'static str, zero: Option<&'static str> },
    Toggle,
    /// Cycles through presets.
    Choice,
    Key(Bind),
    /// Does something when activated.
    Button,
}

/// Things outside `Settings` the screen shows and changes (session state).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extra {
    pub fullscreen: bool,
    pub muted: bool,
}

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Das => "DAS (delayed auto shift)",
            Row::Arr => "ARR (auto repeat rate)",
            Row::Soft => "Soft drop speed",
            Row::Ghost => "Ghost piece",
            Row::Danger => "Danger warning",
            Row::Preset => "Effects preset",
            Row::Flight => "Drop flight",
            Row::Collapse => "Line collapse",
            Row::Glide => "Piece glide",
            Row::Shake => "Screen shake",
            Row::Clears => "Clear effects",
            Row::Scene => "Scene effects",
            Row::Fullscreen => "Fullscreen",
            Row::Master => "Master volume",
            Row::Sfx => "Sound effects",
            Row::Music => "Music",
            Row::Heartbeat => "Danger heartbeat",
            Row::Mute => "Mute everything",
            Row::Key(b) => b.name(),
            Row::ResetKeys => "Reset keys to defaults",
        }
    }

    /// One line explaining the row, shown under the list.
    pub fn hint(self) -> &'static str {
        match self {
            Row::Das => "How long a held left / right waits before it starts repeating.",
            Row::Arr => "Time between repeats once DAS has charged. 0 slides straight to the wall.",
            Row::Soft => "Time per row while soft drop is held. 0 drops straight to the floor.",
            Row::Ghost => "Shows where the falling piece will land.",
            Row::Danger => "Red pulse and the next piece's outline when the stack nears the top.",
            Row::Preset => "Competitive keeps the board exactly where it is. Everything adds all motion.",
            Row::Flight => "Hard drops are drawn travelling down. The board shows its old state meanwhile.",
            Row::Collapse => "Rows slide down after a clear instead of snapping.",
            Row::Glide => "The falling piece eases between rows.",
            Row::Shake => "The view jolts on big hits. Moves the board.",
            Row::Clears => "Line-clear bursts, lock flashes, the border flash and combo callouts.",
            Row::Scene => "The living background, garbage shots, side callouts and meters.",
            Row::Fullscreen => "Fill the screen.",
            Row::Master => "Everything you hear.",
            Row::Sfx => "Moves, drops, clears and the rest.",
            Row::Music => "The soundtrack.",
            Row::Heartbeat => "A heartbeat near the top, quicker as the danger grows.",
            Row::Mute => "Silence everything (N).",
            Row::Key(_) => "Enter or click a slot, then press a key. Backspace clears a slot.",
            Row::ResetKeys => "Arrows, Space, Up / X, Z / Ctrl, C / Shift.",
        }
    }

    pub fn kind(self) -> Kind {
        let ms = |r: (u32, u32), zero| Kind::Slider { min: r.0, max: r.1, step: 1, unit: "ms", zero };
        match self {
            Row::Das => ms(DAS_RANGE, None),
            Row::Arr => ms(ARR_RANGE, Some("instant")),
            Row::Soft => ms(SOFT_RANGE, Some("instant")),
            Row::Master | Row::Sfx | Row::Music => Kind::Slider { min: 0, max: 100, step: 5, unit: "%", zero: Some("off") },
            Row::Preset => Kind::Choice,
            Row::Key(b) => Kind::Key(b),
            Row::ResetKeys => Kind::Button,
            _ => Kind::Toggle,
        }
    }

    pub fn number(self, s: &Settings) -> u32 {
        match self {
            Row::Das => s.das_ms,
            Row::Arr => s.arr_ms,
            Row::Soft => s.soft_ms,
            Row::Master => s.master,
            Row::Sfx => s.sfx,
            Row::Music => s.music,
            _ => 0,
        }
    }

    fn set_number(self, s: &mut Settings, v: u32) {
        if let Kind::Slider { min, max, .. } = self.kind() {
            let v = v.clamp(min, max);
            match self {
                Row::Das => s.das_ms = v,
                Row::Arr => s.arr_ms = v,
                Row::Soft => s.soft_ms = v,
                Row::Master => s.master = v,
                Row::Sfx => s.sfx = v,
                Row::Music => s.music = v,
                _ => {}
            }
        }
    }

    /// A slider set from a 0..1 position (mouse).
    pub fn set_fraction(self, s: &mut Settings, f: f32) {
        if let Kind::Slider { min, max, step, .. } = self.kind() {
            let raw = min as f32 + f.clamp(0.0, 1.0) * (max - min) as f32;
            let v = ((raw / step as f32).round() as u32) * step;
            self.set_number(s, v);
        }
    }

    pub fn fraction(self, s: &Settings) -> f32 {
        match self.kind() {
            Kind::Slider { min, max, .. } => (self.number(s) - min) as f32 / (max - min).max(1) as f32,
            _ => 0.0,
        }
    }

    pub fn on(self, s: &Settings, x: Extra) -> bool {
        match self {
            Row::Ghost => s.ghost,
            Row::Danger => s.danger,
            Row::Flight => s.effects.flight,
            Row::Collapse => s.effects.collapse,
            Row::Glide => s.effects.glide,
            Row::Shake => s.effects.shake,
            Row::Clears => s.effects.clears,
            Row::Scene => s.scene,
            Row::Fullscreen => x.fullscreen,
            Row::Heartbeat => s.heartbeat,
            Row::Mute => x.muted,
            _ => false,
        }
    }

    /// The value as shown ("133 ms", "instant", "ON", "COMPETITIVE", ...). Keys are drawn
    /// separately.
    pub fn shown(self, s: &Settings, x: Extra) -> String {
        match self.kind() {
            Kind::Slider { unit, zero, .. } => {
                let n = self.number(s);
                match zero {
                    Some(z) if n == 0 => z.to_owned(),
                    _ => format!("{n} {unit}"),
                }
            }
            Kind::Toggle => (if self.on(s, x) { "ON" } else { "OFF" }).to_owned(),
            Kind::Choice => s.effects.label().to_uppercase(),
            Kind::Key(_) | Kind::Button => String::new(),
        }
    }

    /// Left / right (`dir` = -1 / +1): sliders step, toggles flip, presets cycle.
    pub fn adjust(self, s: &mut Settings, x: &mut Extra, dir: i32) {
        match self.kind() {
            Kind::Slider { step, .. } => {
                let n = self.number(s) as i64 + dir as i64 * step as i64;
                self.set_number(s, n.max(0) as u32);
            }
            Kind::Toggle => self.toggle(s, x),
            Kind::Choice => {
                const ORDER: [Motion; 3] = [Motion::Competitive, Motion::Full, Motion::Off];
                let at = s.effects.preset().and_then(|m| ORDER.iter().position(|&o| o == m));
                let next = match at {
                    Some(i) => (i as i32 + dir).rem_euclid(ORDER.len() as i32) as usize,
                    None => 0,
                };
                s.effects = Effects::from(ORDER[next]);
            }
            Kind::Key(_) | Kind::Button => {}
        }
    }

    /// Enter / click: toggles flip, presets cycle forward, buttons run. (Keys start
    /// rebinding in `app.rs`.)
    pub fn activate(self, s: &mut Settings, x: &mut Extra) {
        match self {
            Row::ResetKeys => s.keys = Default::default(),
            _ => match self.kind() {
                Kind::Toggle => self.toggle(s, x),
                Kind::Choice => self.adjust(s, x, 1),
                _ => {}
            },
        }
    }

    fn toggle(self, s: &mut Settings, x: &mut Extra) {
        match self {
            Row::Ghost => s.ghost = !s.ghost,
            Row::Danger => s.danger = !s.danger,
            Row::Flight => s.effects.flight = !s.effects.flight,
            Row::Collapse => s.effects.collapse = !s.effects.collapse,
            Row::Glide => s.effects.glide = !s.effects.glide,
            Row::Shake => s.effects.shake = !s.effects.shake,
            Row::Clears => s.effects.clears = !s.effects.clears,
            Row::Scene => s.scene = !s.scene,
            Row::Fullscreen => x.fullscreen = !x.fullscreen,
            Row::Heartbeat => s.heartbeat = !s.heartbeat,
            Row::Mute => x.muted = !x.muted,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_has_rows_and_they_fit() {
        for t in 0..TABS.len() {
            assert!(!rows(t).is_empty() && rows(t).len() <= 10, "tab {t}");
        }
    }

    #[test]
    fn sliders_step_and_clamp() {
        let (mut s, mut x) = (Settings::default(), Extra::default());
        Row::Arr.adjust(&mut s, &mut x, -1);
        assert_eq!(s.arr_ms, 32);
        for _ in 0..200 {
            Row::Arr.adjust(&mut s, &mut x, -1);
        }
        assert_eq!(s.arr_ms, 0);
        assert_eq!(Row::Arr.shown(&s, x), "instant");
        Row::Master.set_fraction(&mut s, 0.5);
        assert_eq!(s.master, 50);
        Row::Das.set_fraction(&mut s, 2.0);
        assert_eq!(s.das_ms, DAS_RANGE.1);
    }

    #[test]
    fn the_preset_reads_custom_once_a_switch_changes() {
        let (mut s, mut x) = (Settings::default(), Extra::default());
        assert_eq!(Row::Preset.shown(&s, x), "COMPETITIVE");
        Row::Flight.activate(&mut s, &mut x);
        assert_eq!(Row::Preset.shown(&s, x), "CUSTOM");
        Row::Preset.adjust(&mut s, &mut x, 1);
        assert_eq!(s.effects, Effects::COMPETITIVE, "custom cycles back to the first preset");
        Row::Preset.adjust(&mut s, &mut x, 1);
        assert_eq!(s.effects, Effects::FULL);
    }

    #[test]
    fn reset_restores_default_keys() {
        let (mut s, mut x) = (Settings::default(), Extra::default());
        s.keys.set(Bind::Hold, 0, Some(macroquad::prelude::KeyCode::Q));
        Row::ResetKeys.activate(&mut s, &mut x);
        assert_eq!(s.keys, Default::default());
    }
}
