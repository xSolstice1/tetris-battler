//! Scene effects: everything that lives OFF the 10x20 field. A living backdrop
//! tinted by the mode and the opponent (and by danger), garbage flying between the
//! fields in a 1v1, callouts beside the board, an attack meter, and score / lines
//! that tick up instead of jumping.
//!
//! This is a separate setting from board `Motion` (src/fx): pros keep the field
//! perfectly still while the world around it stays alive. With scene effects off
//! the backdrop is a still, themed picture and the same information is shown
//! statically (callouts as plain text, numbers without ticking, no shots).
//!
//! Nothing here ever draws on the field rectangle, and nothing feeds back into
//! game logic.

mod backdrop;
mod feedback;

use macroquad::prelude::*;

use crate::bot::Profile;
use crate::game::{Event, Game};
use crate::render::View;
use crate::versus::VsEvent;
use feedback::{Card, Shot};

pub use backdrop::draw_menu_backdrop;

/// The two colours a scene is lit with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
}

impl Theme {
    /// Classic: calm blue and purple.
    pub fn classic() -> Self {
        Theme { primary: crate::render::hex(0x3d6dff), secondary: crate::render::hex(0x8a4dff) }
    }

    /// Shadow 1v1: the shadow's own colour, with a cool counter-light.
    pub fn shadow(p: &Profile) -> Self {
        let c = crate::render::hex(p.color);
        Theme { primary: c, secondary: mix(c, crate::render::hex(0x5a4dff), 0.55) }
    }

    /// Battler: the monster's colour, with a warmer counter-light.
    pub fn monster(color: u32) -> Self {
        let c = crate::render::hex(color);
        Theme { primary: c, secondary: mix(c, crate::render::hex(0xff4f6d), 0.4) }
    }
}

pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

pub(crate) fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// Attack lines the meter can show before it's full.
const METER_MAX: f32 = 12.0;
/// The meter holds a fresh attack this long, then drains...
const METER_HOLD: f32 = 0.8;
/// ...at this many lines per second.
const METER_DRAIN: f32 = 1.6;
/// The backdrop's energy (a clear livens it up) fades at this rate per second.
const ENERGY_DECAY: f32 = 0.9;

pub struct Scene {
    on: bool,
    time: f32,
    /// Drift phase of the floating shapes; runs faster while energy is high.
    drift: f32,
    theme: Theme,
    target: Theme,
    /// Danger (0..1), eased so the scene reddens and calms smoothly.
    danger: f32,
    /// Seconds into the current heartbeat, for the danger pulse.
    beat: f32,
    energy: f32,
    shots: Vec<Shot>,
    cards: Vec<Card>,
    attack: f32,
    attack_hold: f32,
    attack_shown: f32,
    score_shown: f64,
    lines_shown: f32,
}

impl Scene {
    pub fn new(on: bool, theme: Theme) -> Self {
        Scene {
            on,
            time: 0.0,
            drift: 0.0,
            theme,
            target: theme,
            danger: 0.0,
            beat: 0.0,
            energy: 0.0,
            shots: Vec::new(),
            cards: Vec::new(),
            attack: 0.0,
            attack_hold: 0.0,
            attack_shown: 0.0,
            score_shown: 0.0,
            lines_shown: 0.0,
        }
    }

    /// Scene effects animate (otherwise the scene is a still picture).
    pub fn on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        if !on {
            self.shots.clear();
        }
    }

    /// The colours the scene eases toward (e.g. a new monster).
    pub fn set_theme(&mut self, theme: Theme) {
        self.target = theme;
    }

    /// A field event, at the moment it was shown (feed `Fx::take_presented`).
    pub fn on_event(&mut self, event: &Event) {
        if let Event::LinesCleared(c) = event {
            if c.attack > 0 {
                self.attack = (self.attack + c.attack as f32).min(METER_MAX * 1.5);
                self.attack_hold = METER_HOLD;
            }
            let lines = c.lines() as f32;
            self.energy = (self.energy + 0.12 * lines + 0.1 * c.attack as f32).min(1.0);
            if let Some(card) = Card::for_clear(c) {
                self.cards.insert(0, card);
                self.cards.truncate(feedback::MAX_CARDS);
            }
        }
    }

    /// Garbage sent between the fields of a 1v1.
    pub fn on_versus(&mut self, event: &VsEvent) {
        if !self.on {
            return;
        }
        let (lines, to_shadow) = match *event {
            VsEvent::Sent { lines } => (lines, true),
            VsEvent::Received { lines } => (lines, false),
            _ => return,
        };
        let color = if to_shadow { crate::render::hex(0xffd23f) } else { crate::render::hex(0xff4f6d) };
        for i in 0..lines.min(feedback::MAX_VOLLEY) {
            self.shots.push(Shot::new(to_shadow, i as f32 * feedback::VOLLEY_GAP, color));
        }
    }

    /// `danger` is the player's danger level (0 when not in play).
    pub fn update(&mut self, dt: f32, game: &Game, danger: f32) {
        self.time += dt;
        self.drift += dt * (1.0 + 2.5 * self.energy);
        self.energy = (self.energy - ENERGY_DECAY * dt * self.energy.max(0.15)).max(0.0);
        // A still scene changes colour at once instead of fading.
        let k = if self.on { 1.0 - (-2.0 * dt).exp() } else { 1.0 };
        self.theme = Theme { primary: mix(self.theme.primary, self.target.primary, k), secondary: mix(self.theme.secondary, self.target.secondary, k) };
        self.danger += (danger - self.danger) * if self.on { 1.0 - (-3.0 * dt).exp() } else { 1.0 };
        self.beat += dt;
        if self.beat >= crate::danger::beat_interval(self.danger) {
            self.beat = 0.0;
        }

        // The meter holds a fresh attack, then drains.
        if self.attack_hold > 0.0 {
            self.attack_hold -= dt;
        } else {
            self.attack = (self.attack - METER_DRAIN * dt).max(0.0);
        }
        let ease = 1.0 - (-14.0 * dt).exp();
        self.attack_shown += (self.attack - self.attack_shown) * ease;

        // Numbers tick up: fast enough to keep up, never past the real value.
        let score = game.score as f64;
        let lines = game.lines as f32;
        if self.on && score >= self.score_shown {
            let step = ((score - self.score_shown) * (1.0 - (-9.0 * dt as f64).exp())).max(dt as f64 * 400.0);
            self.score_shown = (self.score_shown + step).min(score);
            let step = ((lines - self.lines_shown) * (1.0 - (-9.0 * dt).exp())).max(dt * 12.0);
            self.lines_shown = (self.lines_shown + step).min(lines);
        } else {
            self.score_shown = score;
            self.lines_shown = lines;
        }

        for card in &mut self.cards {
            card.age += dt;
        }
        self.cards.retain(|c| c.age < feedback::CARD_LIFE);
        feedback::settle_cards(&mut self.cards, dt, self.on);
        for shot in &mut self.shots {
            shot.t += dt;
        }
        self.shots.retain(|s| !s.done());
    }

    /// Score and lines as drawn (ticking up toward the real values).
    pub fn score(&self) -> u64 {
        self.score_shown.floor() as u64
    }

    pub fn lines(&self) -> u32 {
        self.lines_shown.floor() as u32
    }

    /// The numbers are still catching up (drawn a little brighter).
    pub fn ticking(&self, game: &Game) -> bool {
        self.score() < game.score
    }

    fn clock(&self) -> f32 {
        if self.on { self.time } else { 0.0 }
    }

    /// Behind everything: call right after clearing the screen. `width` is the
    /// virtual canvas width.
    pub fn draw_back(&self, v: &View, width: f32) {
        let beat = if self.on { 1.0 - (self.beat / crate::danger::beat_interval(self.danger)).min(1.0) } else { 0.5 };
        backdrop::draw(
            v,
            &backdrop::Look {
                width,
                time: self.clock(),
                drift: if self.on { self.drift } else { 0.0 },
                theme: self.theme,
                danger: self.danger,
                beat: beat * beat,
                energy: if self.on { self.energy } else { 0.0 },
                halo: backdrop::field_halo(),
            },
        );
    }

    /// Callouts and the attack meter in the strip under the next queue.
    pub fn draw_side(&self, v: &View) {
        feedback::draw_cards(v, &self.cards, self.on, self.time);
        feedback::draw_meter(v, if self.on { self.attack_shown } else { self.attack }, self.attack / METER_MAX, self.on, self.time);
    }

    /// Garbage in flight between the fields, above the panels.
    pub fn draw_shots(&self, v: &View) {
        feedback::draw_shots(v, &self.shots);
    }
}
