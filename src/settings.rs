//! Player settings: handling, effects, audio and key bindings. Saved with the progress
//! (see `save.rs`) as `set.*` and `key.*` lines; anything missing keeps its default.
//!
//! Pure data and parsing: the settings screen lives in `render.rs` / `app.rs`.

use macroquad::prelude::KeyCode;

use crate::fx::Effects;

/// Something a key can be bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Left,
    Right,
    SoftDrop,
    HardDrop,
    RotateCw,
    RotateCcw,
    Hold,
}

impl Bind {
    pub const ALL: [Bind; 7] = [Bind::Left, Bind::Right, Bind::SoftDrop, Bind::HardDrop, Bind::RotateCw, Bind::RotateCcw, Bind::Hold];

    pub fn id(self) -> &'static str {
        match self {
            Bind::Left => "left",
            Bind::Right => "right",
            Bind::SoftDrop => "soft_drop",
            Bind::HardDrop => "hard_drop",
            Bind::RotateCw => "rotate_cw",
            Bind::RotateCcw => "rotate_ccw",
            Bind::Hold => "hold",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Bind::Left => "Move left",
            Bind::Right => "Move right",
            Bind::SoftDrop => "Soft drop",
            Bind::HardDrop => "Hard drop",
            Bind::RotateCw => "Rotate clockwise",
            Bind::RotateCcw => "Rotate counter-clockwise",
            Bind::Hold => "Hold",
        }
    }

    fn defaults(self) -> [Option<KeyCode>; 2] {
        use KeyCode::*;
        match self {
            Bind::Left => [Some(Left), None],
            Bind::Right => [Some(Right), None],
            Bind::SoftDrop => [Some(Down), None],
            Bind::HardDrop => [Some(Space), None],
            Bind::RotateCw => [Some(Up), Some(X)],
            Bind::RotateCcw => [Some(Z), Some(LeftControl)],
            Bind::Hold => [Some(C), Some(LeftShift)],
        }
    }
}

/// Keys that can be bound (Escape / P / Enter / F-keys stay reserved for the game).
pub const BINDABLE: [KeyCode; 62] = {
    use KeyCode::*;
    [
        A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, Q, R, S, T, U, V, W, X, Y, Z, Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
        Left, Right, Up, Down, Space, LeftShift, RightShift, LeftControl, RightControl, LeftAlt, RightAlt, Tab, Backslash, Slash, Comma, Period,
        Semicolon, Apostrophe, LeftBracket, RightBracket, Minus, Equal, GraveAccent, Kp0, Kp1, Kp2, Kp3,
    ]
};

/// A short label for a key ("Left", "Shift", "A", ...).
pub fn key_name(k: KeyCode) -> String {
    use KeyCode::*;
    let s = match k {
        LeftShift => "L-Shift",
        RightShift => "R-Shift",
        LeftControl => "L-Ctrl",
        RightControl => "R-Ctrl",
        LeftAlt => "L-Alt",
        RightAlt => "R-Alt",
        GraveAccent => "`",
        Backslash => "\\",
        Slash => "/",
        Comma => ",",
        Period => ".",
        Semicolon => ";",
        Apostrophe => "'",
        LeftBracket => "[",
        RightBracket => "]",
        Minus => "-",
        Equal => "=",
        _ => return format!("{k:?}").trim_start_matches("Key").to_owned(),
    };
    s.to_owned()
}

fn key_id(k: KeyCode) -> String {
    format!("{k:?}")
}

fn parse_key(s: &str) -> Option<KeyCode> {
    BINDABLE.into_iter().find(|&k| key_id(k) == s)
}

/// Two keys per action.
#[derive(Clone, Debug, PartialEq)]
pub struct Keys {
    pub slots: [[Option<KeyCode>; 2]; 7],
}

impl Default for Keys {
    fn default() -> Self {
        Keys { slots: Bind::ALL.map(Bind::defaults) }
    }
}

impl Keys {
    pub fn get(&self, b: Bind) -> [Option<KeyCode>; 2] {
        self.slots[b as usize]
    }

    pub fn keys(&self, b: Bind) -> impl Iterator<Item = KeyCode> + '_ {
        self.slots[b as usize].into_iter().flatten()
    }

    /// Binds `key` to `b`'s `slot`, taking it off anything else that had it.
    pub fn set(&mut self, b: Bind, slot: usize, key: Option<KeyCode>) {
        if let Some(k) = key {
            for s in self.slots.iter_mut().flatten() {
                if *s == Some(k) {
                    *s = None;
                }
            }
        }
        self.slots[b as usize][slot.min(1)] = key;
    }

    /// Some action uses this key (so global hotkeys like M stay out of its way).
    pub fn bound(&self, k: KeyCode) -> bool {
        self.slots.iter().flatten().any(|s| *s == Some(k))
    }
}

/// Everything the settings screen controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Delayed auto shift: ms before a held left/right starts repeating.
    pub das_ms: u32,
    /// Auto repeat rate: ms between repeats (0 = instantly to the wall).
    pub arr_ms: u32,
    /// Soft drop: ms per row while held (0 = instantly to the floor).
    pub soft_ms: u32,
    pub ghost: bool,
    /// Red pulse, spawn outline and heartbeat near the top.
    pub danger: bool,
    pub effects: Effects,
    pub scene: bool,
    /// Volumes, 0..=100.
    pub master: u32,
    pub sfx: u32,
    pub music: u32,
    pub heartbeat: bool,
    pub keys: Keys,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            das_ms: 133,
            arr_ms: 33,
            soft_ms: 33,
            ghost: true,
            danger: true,
            effects: Effects::COMPETITIVE,
            scene: true,
            master: 80,
            sfx: 100,
            music: 70,
            heartbeat: true,
            keys: Keys::default(),
        }
    }
}

pub const DAS_RANGE: (u32, u32) = (0, 300);
pub const ARR_RANGE: (u32, u32) = (0, 100);
pub const SOFT_RANGE: (u32, u32) = (0, 100);

fn flag(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

impl Settings {
    /// `set.*` / `key.*` lines for the save file.
    pub fn to_lines(&self) -> String {
        let e = self.effects;
        let mut out = format!(
            "set.das={}\nset.arr={}\nset.soft={}\nset.ghost={}\nset.danger={}\n",
            self.das_ms,
            self.arr_ms,
            self.soft_ms,
            flag(self.ghost),
            flag(self.danger)
        );
        out += &format!(
            "set.flight={}\nset.collapse={}\nset.glide={}\nset.shake={}\nset.clear_fx={}\nset.scene={}\n",
            flag(e.flight),
            flag(e.collapse),
            flag(e.glide),
            flag(e.shake),
            flag(e.clears),
            flag(self.scene)
        );
        out += &format!(
            "set.master={}\nset.sfx={}\nset.music={}\nset.heartbeat={}\n",
            self.master,
            self.sfx,
            self.music,
            flag(self.heartbeat)
        );
        for b in Bind::ALL {
            let [a, c] = self.keys.get(b);
            let k = |k: Option<KeyCode>| k.map_or("-".to_owned(), key_id);
            out += &format!("key.{}={},{}\n", b.id(), k(a), k(c));
        }
        out
    }

    /// Applies one `key=value` save line, if it's a setting. Returns whether it was.
    pub fn read_line(&mut self, key: &str, value: &str) -> bool {
        let num = |range: (u32, u32), cur: u32| value.parse::<u32>().map(|v| v.clamp(range.0, range.1)).unwrap_or(cur);
        let on = |cur: bool| match value {
            "1" => true,
            "0" => false,
            _ => cur,
        };
        match key {
            "set.das" => self.das_ms = num(DAS_RANGE, self.das_ms),
            "set.arr" => self.arr_ms = num(ARR_RANGE, self.arr_ms),
            "set.soft" => self.soft_ms = num(SOFT_RANGE, self.soft_ms),
            "set.ghost" => self.ghost = on(self.ghost),
            "set.danger" => self.danger = on(self.danger),
            "set.flight" => self.effects.flight = on(self.effects.flight),
            "set.collapse" => self.effects.collapse = on(self.effects.collapse),
            "set.glide" => self.effects.glide = on(self.effects.glide),
            "set.shake" => self.effects.shake = on(self.effects.shake),
            "set.clear_fx" => self.effects.clears = on(self.effects.clears),
            "set.scene" => self.scene = on(self.scene),
            "set.master" => self.master = num((0, 100), self.master),
            "set.sfx" => self.sfx = num((0, 100), self.sfx),
            "set.music" => self.music = num((0, 100), self.music),
            "set.heartbeat" => self.heartbeat = on(self.heartbeat),
            _ => {
                let Some(id) = key.strip_prefix("key.") else {
                    return false;
                };
                let Some(b) = Bind::ALL.into_iter().find(|b| b.id() == id) else {
                    return true;
                };
                let mut parts = value.split(',');
                let slots = [parts.next(), parts.next()].map(|p| p.and_then(|s| parse_key(s.trim())));
                // An empty binding would make the action unplayable: keep the defaults.
                if slots.iter().any(Option::is_some) {
                    for (i, k) in slots.into_iter().enumerate() {
                        self.keys.set(b, i, k);
                    }
                }
            }
        }
        true
    }

    /// 0..1 gains for the mixer.
    pub fn sfx_gain(&self) -> f32 {
        (self.master as f32 / 100.0) * (self.sfx as f32 / 100.0)
    }

    pub fn music_gain(&self) -> f32 {
        (self.master as f32 / 100.0) * (self.music as f32 / 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_save_lines() {
        let mut s = Settings { das_ms: 90, arr_ms: 0, soft_ms: 0, ghost: false, master: 55, music: 0, ..Settings::default() };
        s.effects.flight = true;
        s.keys.set(Bind::HardDrop, 1, Some(KeyCode::W));
        let mut back = Settings::default();
        for line in s.to_lines().lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(back.read_line(k, v), "{line}");
        }
        assert_eq!(back, s);
    }

    #[test]
    fn bad_values_keep_the_defaults() {
        let mut s = Settings::default();
        s.read_line("set.das", "banana");
        s.read_line("set.arr", "9999");
        s.read_line("key.hold", "-,-");
        s.read_line("key.hold", "NotAKey");
        assert_eq!(s.das_ms, 133);
        assert_eq!(s.arr_ms, ARR_RANGE.1, "clamped");
        assert_eq!(s.keys.get(Bind::Hold), Bind::Hold.defaults(), "an action is never left unbound");
        assert!(!s.read_line("xp", "5"), "not a setting");
    }

    #[test]
    fn binding_a_key_moves_it_off_its_old_action() {
        let mut k = Keys::default();
        k.set(Bind::Hold, 0, Some(KeyCode::Up));
        assert_eq!(k.get(Bind::RotateCw)[0], None, "Up left rotate cw");
        assert_eq!(k.get(Bind::Hold)[0], Some(KeyCode::Up));
        assert!(k.bound(KeyCode::Up) && !k.bound(KeyCode::M));
    }

    #[test]
    fn key_names_are_short() {
        assert_eq!(key_name(KeyCode::Key5), "5");
        assert_eq!(key_name(KeyCode::LeftShift), "L-Shift");
        assert_eq!(key_name(KeyCode::Space), "Space");
    }
}
