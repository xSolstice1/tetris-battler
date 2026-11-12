//! Stacked callout text ("T-SPIN DOUBLE", "COMBO 4", ...). Newest sits on top
//! and pops in with an overshoot; older ones slide down and fade.

use macroquad::prelude::*;

use super::tween::out_back;
use crate::render::View;

const MAX_CALLOUTS: usize = 5;
const POP_TIME: f32 = 0.18;
const LIFE: f32 = 1.4;
const FADE_TIME: f32 = 0.35;
const SLIDE_SPEED: f32 = 14.0;

struct Callout {
    text: String,
    color: Color,
    size: f32,
    rainbow: bool,
    age: f32,
    /// Current vertical offset below the anchor; eases toward its slot.
    y: f32,
}

#[derive(Default)]
pub struct Callouts {
    items: Vec<Callout>,
}

/// Fully saturated colour around the hue wheel, `h` in 0..1.
fn hue(h: f32) -> Color {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    Color::new(r, g, b, 1.0)
}

impl Callouts {
    pub fn push(&mut self, text: impl Into<String>, color: Color, size: f32) {
        self.insert(text.into(), color, size, false);
    }

    pub fn push_rainbow(&mut self, text: impl Into<String>, size: f32) {
        self.insert(text.into(), WHITE, size, true);
    }

    fn insert(&mut self, text: String, color: Color, size: f32, rainbow: bool) {
        self.items.insert(
            0,
            Callout {
                text,
                color,
                size,
                rainbow,
                age: 0.0,
                y: 0.0,
            },
        );
        self.items.truncate(MAX_CALLOUTS);
    }

    pub fn update(&mut self, dt: f32) {
        self.items.retain_mut(|c| {
            c.age += dt;
            c.age < LIFE
        });
        let mut target = 0.0;
        for c in &mut self.items {
            c.y += (target - c.y) * (SLIDE_SPEED * dt).min(1.0);
            target += c.size * 1.15;
        }
    }

    /// Draws centred on `cx`, stacking downward from `top`.
    pub fn draw(&self, v: &View, cx: f32, top: f32, time: f32) {
        for c in &self.items {
            let scale = if c.age < POP_TIME {
                0.4 + 0.6 * out_back(c.age / POP_TIME)
            } else {
                1.0
            };
            let alpha = ((LIFE - c.age) / FADE_TIME).min(1.0);
            let base = if c.rainbow { hue(time * 1.5) } else { c.color };
            let y = top + c.y - c.age * 12.0;
            let size = c.size * scale;
            // Drop shadow keeps text readable over busy particles.
            v.text_centered(&c.text, cx + 2.0, y + 2.0, size, Color::new(0.0, 0.0, 0.0, 0.6 * alpha));
            v.text_centered(&c.text, cx, y, size, Color { a: alpha, ..base });
        }
    }
}
