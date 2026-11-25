//! Off-board feedback: callout cards and the attack meter in the strip under the
//! next queue, and garbage flying between the fields of a 1v1. All of it is laid
//! out outside the field rectangle (see the tests).

use macroquad::prelude::*;

use super::{alpha, mix};
use crate::board::VISIBLE_ROWS;
use crate::game::LineClear;
use crate::ui;
use crate::render::{BOARD_H, BOARD_W, BOARD_X, BOARD_Y, OPP_CELL, OPP_X, OPP_Y, PANEL_W, RIGHT_X, View, hex};

pub(super) const MAX_CARDS: usize = 3;
pub(super) const CARD_LIFE: f32 = 2.6;
const CARD_POP: f32 = 0.22;
const CARD_FADE: f32 = 0.5;
const CARD_H: f32 = 44.0;
const CARD_GAP: f32 = 8.0;

/// The strip under the next queue (5 pieces, 80 px each, from `BOARD_Y`).
const STRIP_X: f32 = RIGHT_X;
const STRIP_W: f32 = PANEL_W;
const METER_Y: f32 = BOARD_Y + 450.0;
const METER_H: f32 = 14.0;
const CARDS_Y: f32 = METER_Y + 36.0;

pub(super) const MAX_VOLLEY: u32 = 8;
pub(super) const VOLLEY_GAP: f32 = 0.05;
const SHOT_TIME: f32 = 0.55;
const IMPACT_TIME: f32 = 0.28;
/// How far a shot's glow (or its impact ring) reaches from its centre.
const SHOT_REACH: f32 = 12.0;
const IMPACT_REACH: f32 = 14.0;

/// One clear worth calling out, e.g. "B2B · COMBO 3" over "T-SPIN DOUBLE".
pub(super) struct Card {
    title: &'static str,
    sub: String,
    color: Color,
    rainbow: bool,
    attack: u32,
    pub age: f32,
    /// Drawn offset below the strip's top; eases toward the card's slot.
    y: f32,
}

impl Card {
    /// Only clears worth shouting about get a card (not plain singles to triples).
    pub fn for_clear(c: &LineClear) -> Option<Card> {
        let lines = c.lines();
        let purple = hex(0xc061ff);
        let (title, color) = match (c.t_spin, lines) {
            (true, 0) => ("T-SPIN", purple),
            (true, 1) => ("T-SPIN SINGLE", purple),
            (true, 2) => ("T-SPIN DOUBLE", purple),
            (true, _) => ("T-SPIN TRIPLE", purple),
            (false, 1) => ("SINGLE", hex(0xc8d0e0)),
            (false, 2) => ("DOUBLE", hex(0x5ad1ff)),
            (false, 3) => ("TRIPLE", hex(0x7dff8a)),
            (false, _) => ("QUAD", hex(0xffd23f)),
        };
        let notable = c.t_spin || lines >= 4 || c.back_to_back || c.combo > 0 || c.perfect_clear;
        if !notable {
            return None;
        }
        let mut tags = Vec::new();
        if c.back_to_back {
            tags.push("B2B".to_owned());
        }
        if c.combo > 0 {
            tags.push(format!("COMBO {}", c.combo));
        }
        let (title, color) = if c.perfect_clear { ("PERFECT CLEAR", hex(0xff7ad9)) } else { (title, color) };
        Some(Card { title, sub: tags.join(" · "), color, rainbow: c.perfect_clear, attack: c.attack, age: 0.0, y: 0.0 })
    }
}

/// Cards slide down to their slots (snap when the scene is still).
pub(super) fn settle_cards(cards: &mut [Card], dt: f32, on: bool) {
    for (i, c) in cards.iter_mut().enumerate() {
        let target = i as f32 * (CARD_H + CARD_GAP);
        if on {
            c.y += (target - c.y) * (1.0 - (-16.0 * dt).exp());
        } else {
            c.y = target;
        }
    }
}

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

fn out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + t * t * (2.7 * t + 1.7)
}

/// Largest size (up to `size`) at which `text` fits in `width`.
fn fit(v: &View, text: &str, size: f32, width: f32) -> f32 {
    let s = v.scale();
    let w = measure_text(text, None, (size * s).max(1.0) as u16, 1.0).width / s;
    if w > width { size * width / w } else { size }
}

pub(super) fn draw_cards(v: &View, cards: &[Card], on: bool, time: f32) {
    for (i, c) in cards.iter().enumerate() {
        let pop = if on { (c.age / CARD_POP).min(1.0) } else { 1.0 };
        let fade = ((CARD_LIFE - c.age) / CARD_FADE).clamp(0.0, 1.0);
        let a = if on { fade * pop.min(1.0).sqrt() } else { 1.0 };
        // Slides out from the board's side as it appears.
        let x = STRIP_X - if on { 14.0 * (1.0 - out_back(pop)) } else { 0.0 };
        let y = CARDS_Y + c.y.max(0.0);
        let color = if c.rainbow && on { mix(hue(time * 0.6), WHITE, 0.25) } else { c.color };
        let r = Rect::new(x, y, STRIP_W, CARD_H);
        let rad = 8.0;

        ui::shadow(v, r, rad, 10.0, 4.0, 0.4 * a);
        if i == 0 && on {
            // The newest card glows in its colour, dying down as it settles.
            let k = (1.0 - c.age / 0.9).max(0.0) * fade;
            ui::glow(v, r, rad, 10.0, alpha(color, 0.35 * k));
        }
        ui::fill_h(v, r, rad, alpha(mix(hex(0x1c2233), color, 0.16), 0.95 * a), alpha(hex(0x11141f), 0.95 * a));
        ui::stroke(v, r, rad, 1.0, alpha(color, 0.3 * a));
        ui::shine(v, r, rad, 0.12 * a);
        ui::accent_bar(v, Rect::new(x - 1.0, y, STRIP_W, CARD_H), alpha(color, a));
        if !c.sub.is_empty() {
            let size = fit(v, &c.sub, 14.0, STRIP_W - 56.0);
            v.text(&c.sub, x + 14.0, y + 16.0, size, alpha(mix(color, WHITE, 0.45), 0.9 * a));
        }
        if c.attack > 0 {
            let t = format!("+{}", c.attack);
            ui::pill_right(v, x + STRIP_W - 6.0, y + 12.0, &t, 11.0, alpha(color, a), ui::Pill::Soft);
        }
        let size = fit(v, c.title, 25.0, STRIP_W - 24.0);
        let ty = y + if c.sub.is_empty() && c.attack == 0 { 30.0 } else { 38.0 };
        let scale = if on { 0.7 + 0.3 * out_back(pop) } else { 1.0 };
        ui::text_shadow(v, c.title, x + 14.0, ty, size * scale, alpha(mix(color, WHITE, 0.1), a));
    }
}

/// `shown` lines (eased when animated) and the real fill `frac` for the readout.
pub(super) fn draw_meter(v: &View, shown: f32, frac: f32, on: bool, time: f32) {
    let max = super::METER_MAX;
    let k = (shown / max).clamp(0.0, 1.0);
    let (x, y, w) = (STRIP_X, METER_Y, STRIP_W);
    let lines = (frac * max).ceil() as u32;
    ui::section(v, x, y - 10.0, w - 28.0, "POWER", hex(ui::TEXT2));
    v.text_right(&lines.to_string(), x + w, y - 8.0, 20.0, if lines > 0 { WHITE } else { hex(ui::TEXT3) });

    let fill = if k < 0.5 { mix(hex(0x2ee6d6), hex(0xffd23f), k * 2.0) } else { mix(hex(0xffd23f), hex(0xff4f6d), k * 2.0 - 1.0) };
    let r = Rect::new(x, y, w, METER_H);
    if on && k > 0.4 {
        let pulse = 0.5 + 0.5 * (time * (5.0 + 6.0 * k)).sin();
        ui::glow(v, Rect::new(x, y, w * k, METER_H), METER_H / 2.0, 8.0, alpha(fill, (0.15 + 0.2 * pulse) * k));
    }
    // The fill runs from cyan to its current heat; a notch every 4 lines (a quad's worth).
    let bar = ui::Bar::new(hex(0x2ee6d6), fill).stretch().ticks(max as u32 / 4).glow(if on { 0.8 } else { 0.0 });
    ui::progress(v, r, k, &bar);
}

/// A glowing shot of garbage on its way across.
pub(super) struct Shot {
    to_shadow: bool,
    delay: f32,
    color: Color,
    pub t: f32,
}

impl Shot {
    pub fn new(to_shadow: bool, delay: f32, color: Color) -> Self {
        Shot { to_shadow, delay, color, t: 0.0 }
    }

    pub fn done(&self) -> bool {
        self.t > self.delay + SHOT_TIME + IMPACT_TIME
    }
}

/// Bottom of the shadow's field (where its garbage meter grows from).
const OPP_BOTTOM: f32 = OPP_Y + VISIBLE_ROWS as f32 * OPP_CELL;
/// Shots swoop under the fields, below the canvas's last text.
const LANE: f32 = BOARD_Y + BOARD_H + 62.0;

/// Cubic Bézier control points: from the side of one field, down under the
/// fields and up into the other's garbage meter.
fn path(to_shadow: bool) -> [Vec2; 4] {
    let mine_out = vec2(BOARD_X + BOARD_W + 17.0, BOARD_Y + BOARD_H - 20.0);
    let mine_in = vec2(BOARD_X - 25.0, BOARD_Y + BOARD_H - 4.0);
    let theirs = vec2(OPP_X - 8.0, OPP_BOTTOM - 4.0);
    if to_shadow {
        [mine_out, vec2(mine_out.x, LANE), vec2(theirs.x, LANE), theirs]
    } else {
        [theirs, vec2(theirs.x, LANE), vec2(mine_in.x - 24.0, LANE), mine_in]
    }
}

fn bezier(p: &[Vec2; 4], t: f32) -> Vec2 {
    let u = 1.0 - t;
    p[0] * (u * u * u) + p[1] * (3.0 * u * u * t) + p[2] * (3.0 * u * t * t) + p[3] * (t * t * t)
}

fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

pub(super) fn draw_shots(v: &View, shots: &[Shot]) {
    for s in shots {
        let p = (s.t - s.delay) / SHOT_TIME;
        if p < 0.0 {
            continue;
        }
        let pts = path(s.to_shadow);
        if p <= 1.0 {
            // A tapering trail of fading copies, then the hot head.
            const TRAIL: usize = 26;
            for k in (1..=TRAIL).rev() {
                let q = p - k as f32 * 0.008;
                if q <= 0.0 {
                    continue;
                }
                let at = bezier(&pts, ease(q));
                let f = 1.0 - k as f32 / (TRAIL + 1) as f32;
                v.circle(at.x, at.y, 1.5 + 4.5 * f, alpha(s.color, 0.3 * f));
            }
            let at = bezier(&pts, ease(p));
            v.circle(at.x, at.y, SHOT_REACH, alpha(s.color, 0.14));
            v.circle(at.x, at.y, 7.5, alpha(s.color, 0.35));
            v.circle(at.x, at.y, 5.0, alpha(mix(s.color, WHITE, 0.3), 0.85));
            v.circle(at.x, at.y, 2.8, alpha(WHITE, 0.95));
        } else {
            // Impact: a ring and a flash at the meter it hit.
            let q = ((p - 1.0) * SHOT_TIME / IMPACT_TIME).min(1.0);
            let at = pts[3];
            v.circle(at.x, at.y, 4.0 + 3.0 * q, alpha(mix(s.color, WHITE, 0.5), 0.5 * (1.0 - q)));
            v.circle_lines(at.x, at.y, 4.0 + (IMPACT_REACH - 4.0) * q, 2.0, alpha(s.color, 0.8 * (1.0 - q)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The field (with its 4 px frame), which scene effects must never touch.
    fn field() -> Rect {
        Rect::new(BOARD_X - 4.0, BOARD_Y - 4.0, BOARD_W + 8.0, BOARD_H + 8.0)
    }

    fn distance(r: &Rect, p: Vec2) -> f32 {
        let dx = (r.x - p.x).max(p.x - r.right()).max(0.0);
        let dy = (r.y - p.y).max(p.y - r.bottom()).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }

    #[test]
    fn shots_fly_around_the_field_never_over_it() {
        for to_shadow in [true, false] {
            let pts = path(to_shadow);
            for i in 0..=400 {
                let p = bezier(&pts, i as f32 / 400.0);
                assert!(distance(&field(), p) > SHOT_REACH, "{to_shadow}: {p:?}");
                assert!(p.y < 760.0, "{to_shadow}: off the canvas at {p:?}");
            }
            assert!(distance(&field(), pts[3]) > IMPACT_REACH);
        }
    }

    #[test]
    fn the_strip_sits_beside_the_field_under_the_next_queue() {
        let next_bottom = BOARD_Y + 80.0 * crate::game::NEXT_PREVIEW as f32 + 10.0;
        let meter = Rect::new(STRIP_X - 3.0, METER_Y - 30.0, STRIP_W + 6.0, METER_H + 33.0);
        let last_card = CARDS_Y + (MAX_CARDS - 1) as f32 * (CARD_H + CARD_GAP);
        let cards = Rect::new(STRIP_X - 20.0, CARDS_Y - 6.0, STRIP_W + 26.0, last_card + CARD_H + 12.0 - CARDS_Y);
        for r in [meter, cards] {
            assert!(!r.overlaps(&field()), "{r:?}");
            assert!(r.y > next_bottom && r.right() <= 760.0, "{r:?}");
        }
        assert!(cards.bottom() < LANE - 20.0);
    }

    fn clear(lines: usize, t_spin: bool, b2b: bool, combo: u32) -> LineClear {
        LineClear {
            rows: (0..lines).collect(),
            snapshot: vec![],
            t_spin,
            back_to_back: b2b,
            combo,
            perfect_clear: false,
            attack: 2,
            cancelled: 0,
        }
    }

    #[test]
    fn only_notable_clears_get_a_card() {
        assert!(Card::for_clear(&clear(1, false, false, 0)).is_none());
        assert!(Card::for_clear(&clear(3, false, false, 0)).is_none());
        assert_eq!(Card::for_clear(&clear(4, false, false, 0)).unwrap().title, "QUAD");
        let c = Card::for_clear(&clear(2, true, true, 3)).unwrap();
        assert_eq!((c.title, c.sub.as_str()), ("T-SPIN DOUBLE", "B2B · COMBO 3"));
        assert_eq!(Card::for_clear(&clear(1, false, false, 2)).unwrap().sub, "COMBO 2");
    }
}
