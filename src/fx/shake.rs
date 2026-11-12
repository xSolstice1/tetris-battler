//! Trauma-based screen shake plus a directional "kick" (hard drops push the
//! field down, then it springs back).

use macroquad::prelude::*;
use macroquad::rand::gen_range;

const MAX_OFFSET: f32 = 16.0;
const TRAUMA_DECAY: f32 = 1.8;
const MAX_KICK: f32 = 12.0;
const KICK_RECOVERY: f32 = 20.0;

#[derive(Default)]
pub struct Shake {
    trauma: f32,
    kick: f32,
    offset: Vec2,
}

impl Shake {
    /// Adds trauma in 0..=1. Shake strength is trauma², so small hits stay subtle.
    pub fn add(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    /// Pushes the view down by `px`, recovering exponentially.
    pub fn kick(&mut self, px: f32) {
        self.kick = (self.kick + px).min(MAX_KICK);
    }

    pub fn update(&mut self, dt: f32) {
        self.trauma = (self.trauma - TRAUMA_DECAY * dt).max(0.0);
        self.kick *= (-KICK_RECOVERY * dt).exp();
        let s = self.trauma * self.trauma * MAX_OFFSET;
        self.offset = vec2(gen_range(-1.0, 1.0) * s, gen_range(-1.0, 1.0) * s + self.kick);
    }

    pub fn offset(&self) -> Vec2 {
        self.offset
    }
}
