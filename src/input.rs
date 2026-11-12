//! Keyboard -> game actions, with DAS/ARR auto-repeat for sideways movement
//! and a fixed repeat rate for soft drop.

use macroquad::prelude::*;

use crate::game::Action;

/// Delay before a held left/right starts repeating.
const DAS: f32 = 0.133;
/// Interval between repeats once DAS has charged.
const ARR: f32 = 0.033;
const SOFT_DROP_INTERVAL: f32 = 0.033;

#[derive(Default)]
pub struct Input {
    dir: i32,
    das_timer: f32,
    arr_timer: f32,
    soft_timer: f32,
}

fn pressed(keys: &[KeyCode]) -> bool {
    keys.iter().any(|&k| is_key_pressed(k))
}

impl Input {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn start_shift(&mut self, dir: i32) {
        self.dir = dir;
        self.das_timer = 0.0;
        self.arr_timer = 0.0;
    }

    pub fn poll(&mut self, dt: f32) -> Vec<Action> {
        let mut out = Vec::new();

        if pressed(&[KeyCode::C, KeyCode::LeftShift, KeyCode::RightShift]) {
            out.push(Action::Hold);
        }
        if pressed(&[KeyCode::Up, KeyCode::X]) {
            out.push(Action::RotateCw);
        }
        if pressed(&[KeyCode::Z, KeyCode::LeftControl, KeyCode::RightControl]) {
            out.push(Action::RotateCcw);
        }

        // Sideways: the most recently pressed direction wins.
        let left = is_key_down(KeyCode::Left);
        let right = is_key_down(KeyCode::Right);
        if is_key_pressed(KeyCode::Left) {
            self.start_shift(-1);
            out.push(Action::MoveLeft);
        }
        if is_key_pressed(KeyCode::Right) {
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
            if self.das_timer >= DAS {
                self.arr_timer += dt;
                while self.arr_timer >= ARR {
                    self.arr_timer -= ARR;
                    out.push(if self.dir < 0 {
                        Action::MoveLeft
                    } else {
                        Action::MoveRight
                    });
                }
            }
        }

        if is_key_pressed(KeyCode::Down) {
            self.soft_timer = 0.0;
            out.push(Action::SoftDrop);
        } else if is_key_down(KeyCode::Down) {
            self.soft_timer += dt;
            while self.soft_timer >= SOFT_DROP_INTERVAL {
                self.soft_timer -= SOFT_DROP_INTERVAL;
                out.push(Action::SoftDrop);
            }
        }

        if is_key_pressed(KeyCode::Space) {
            out.push(Action::HardDrop);
        }

        out
    }
}
