//! Keyboard -> game actions, with DAS/ARR auto-repeat for sideways movement and a
//! repeat rate for soft drop, all from the player's settings (bound keys included).

use macroquad::prelude::*;

use crate::game::Action;
use crate::settings::{Bind, Keys, Settings};

pub struct Input {
    dir: i32,
    das_timer: f32,
    arr_timer: f32,
    soft_timer: f32,
    /// Real presses of game keys so far (auto-repeat excluded), for KPP.
    presses: u32,
    /// Delay before a held left/right repeats, and the repeat interval (0 = to the wall).
    das: f32,
    arr: f32,
    /// Seconds per soft-drop row (0 = to the floor).
    soft: f32,
    keys: Keys,
}

impl Default for Input {
    fn default() -> Self {
        Input::new(&Settings::default())
    }
}

/// The furthest a single frame of instant repeat can move (the field's width is plenty).
const INSTANT: usize = 20;

impl Input {
    pub fn new(s: &Settings) -> Self {
        Input {
            dir: 0,
            das_timer: 0.0,
            arr_timer: 0.0,
            soft_timer: 0.0,
            presses: 0,
            das: s.das_ms as f32 / 1000.0,
            arr: s.arr_ms as f32 / 1000.0,
            soft: s.soft_ms as f32 / 1000.0,
            keys: s.keys.clone(),
        }
    }

    /// Takes new handling and keys (from the settings screen), keeping the press count.
    pub fn apply(&mut self, s: &Settings) {
        let presses = self.presses;
        *self = Input::new(s);
        self.presses = presses;
    }

    fn pressed(&self, b: Bind) -> bool {
        self.keys.keys(b).any(is_key_pressed)
    }

    fn down(&self, b: Bind) -> bool {
        self.keys.keys(b).any(is_key_down)
    }

    /// Held from before this frame (not just pressed): initial rotation / hold.
    pub fn held(&self, b: Bind) -> bool {
        self.keys.keys(b).any(|k| is_key_down(k) && !is_key_pressed(k))
    }

    /// Clears held-key state (e.g. on pause) but keeps the press count and settings.
    pub fn reset(&mut self) {
        self.dir = 0;
        self.das_timer = 0.0;
        self.arr_timer = 0.0;
        self.soft_timer = 0.0;
    }

    pub fn presses(&self) -> u32 {
        self.presses
    }

    fn start_shift(&mut self, dir: i32) {
        self.dir = dir;
        self.das_timer = 0.0;
        self.arr_timer = 0.0;
    }

    pub fn poll(&mut self, dt: f32) -> Vec<Action> {
        let mut out = Vec::new();
        self.presses += Bind::ALL.iter().flat_map(|&b| self.keys.keys(b)).filter(|&k| is_key_pressed(k)).count() as u32;

        if self.pressed(Bind::Hold) {
            out.push(Action::Hold);
        }
        if self.pressed(Bind::RotateCw) {
            out.push(Action::RotateCw);
        }
        if self.pressed(Bind::RotateCcw) {
            out.push(Action::RotateCcw);
        }

        // Sideways: the most recently pressed direction wins.
        let left = self.down(Bind::Left);
        let right = self.down(Bind::Right);
        if self.pressed(Bind::Left) {
            self.start_shift(-1);
            out.push(Action::MoveLeft);
        }
        if self.pressed(Bind::Right) {
            self.start_shift(1);
            out.push(Action::MoveRight);
        }
        let still_held = match self.dir {
            -1 => left,
            1 => right,
            _ => false,
        };
        if !still_held {
            if left {
                self.start_shift(-1);
            } else if right {
                self.start_shift(1);
            } else {
                self.dir = 0;
            }
        }
        if self.dir != 0 {
            self.das_timer += dt;
            if self.das_timer >= self.das {
                let step = if self.dir < 0 { Action::MoveLeft } else { Action::MoveRight };
                if self.arr <= 0.0 {
                    out.extend(std::iter::repeat_n(step, INSTANT));
                } else {
                    self.arr_timer += dt;
                    while self.arr_timer >= self.arr {
                        self.arr_timer -= self.arr;
                        out.push(step);
                    }
                }
            }
        }

        if self.pressed(Bind::SoftDrop) {
            self.soft_timer = 0.0;
            out.push(Action::SoftDrop);
            if self.soft <= 0.0 {
                out.extend(std::iter::repeat_n(Action::SoftDrop, 40));
            }
        } else if self.down(Bind::SoftDrop) {
            if self.soft <= 0.0 {
                out.extend(std::iter::repeat_n(Action::SoftDrop, 40));
            } else {
                self.soft_timer += dt;
                while self.soft_timer >= self.soft {
                    self.soft_timer -= self.soft;
                    out.push(Action::SoftDrop);
                }
            }
        }

        if self.pressed(Bind::HardDrop) {
            out.push(Action::HardDrop);
        }

        out
    }
}
