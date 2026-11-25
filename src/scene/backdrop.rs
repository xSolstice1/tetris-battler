//! The living backdrop: a soft two-tone gradient, slow lights, translucent blocks
//! drifting up at three depths, and a vignette. Danger bleeds red in from below.
//! All of it is a handful of meshes and rectangles per frame (cheap on wasm), and
//! it sits behind everything: the field and the panels are drawn opaque over it.

use std::f32::consts::TAU;

use macroquad::prelude::*;

use super::{Theme, alpha, mix};
use crate::piece::PieceKind;
use crate::render::{self, BOARD_H, BOARD_W, BOARD_X, BOARD_Y, View, hex};

pub(super) struct Look {
    /// Virtual canvas width.
    pub width: f32,
    /// Seconds (0 for a still scene).
    pub time: f32,
    /// Drift phase of the floating shapes.
    pub drift: f32,
    pub theme: Theme,
    pub danger: f32,
    /// 1 on a heartbeat, falling to 0 before the next.
    pub beat: f32,
    /// 0..1, briefly raised by clears.
    pub energy: f32,
    /// Centre and radii (virtual) of the light behind the main subject.
    pub halo: (Vec2, f32, f32),
}

const BLOCKS: usize = 22;
/// Depths of the three parallax layers (far to near).
const LAYERS: [f32; 3] = [0.35, 0.6, 1.0];

fn hash01(i: usize, salt: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(2_654_435_761) ^ salt.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 15)).wrapping_mul(2_246_822_507);
    h = (h ^ (h >> 13)).wrapping_mul(3_266_489_909);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

/// A rectangle with one colour on top and another at the bottom (screen px).
fn gradient(x: f32, y: f32, w: f32, h: f32, top: Color, bottom: Color) {
    let v = |x: f32, y: f32, c: Color| Vertex::new(x, y, 0.0, 0.0, 0.0, c);
    draw_mesh(&Mesh {
        vertices: vec![v(x, y, top), v(x + w, y, top), v(x + w, y + h, bottom), v(x, y + h, bottom)],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    });
}

/// Same, left to right.
fn gradient_h(x: f32, y: f32, w: f32, h: f32, left: Color, right: Color) {
    let v = |x: f32, y: f32, c: Color| Vertex::new(x, y, 0.0, 0.0, 0.0, c);
    draw_mesh(&Mesh {
        vertices: vec![v(x, y, left), v(x + w, y, right), v(x + w, y + h, right), v(x, y + h, left)],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    });
}

/// A soft elliptical light (screen px): full colour at the centre, a gentle
/// shoulder, nothing at the rim.
fn glow(c: Vec2, rx: f32, ry: f32, color: Color) {
    const SEG: usize = 28;
    if color.a <= 0.002 {
        return;
    }
    let mut vertices = Vec::with_capacity(1 + 2 * SEG);
    vertices.push(Vertex::new(c.x, c.y, 0.0, 0.0, 0.0, color));
    let mid = alpha(color, color.a * 0.42);
    let rim = alpha(color, 0.0);
    for ring in [(0.45, mid), (1.0, rim)] {
        for i in 0..SEG {
            let a = i as f32 / SEG as f32 * TAU;
            vertices.push(Vertex::new(c.x + a.cos() * rx * ring.0, c.y + a.sin() * ry * ring.0, 0.0, 0.0, 0.0, ring.1));
        }
    }
    let mut indices = Vec::with_capacity(SEG * 9);
    for i in 0..SEG {
        let (a, b) = (1 + i as u16, 1 + ((i + 1) % SEG) as u16);
        indices.extend([0, a, b]);
        let (oa, ob) = (a + SEG as u16, b + SEG as u16);
        indices.extend([a, oa, ob, a, ob, b]);
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

/// Translucent tetrominoes floating up, slower and fainter the farther they are.
fn drifting_blocks(v: &View, look: &Look) {
    let (sw, sh) = (screen_width(), screen_height());
    let s = v.scale();
    let red = hex(0xff3b4f);
    let span = sh + 180.0 * s;
    for i in 0..BLOCKS {
        let depth = LAYERS[i % LAYERS.len()];
        let kind = PieceKind::ALL[(hash01(i, 1) * 7.0) as usize % 7];
        let cell = (8.0 + 12.0 * depth) * s;
        let sway = (look.time * (0.08 + 0.1 * hash01(i, 3)) + hash01(i, 4) * TAU).sin() * 16.0 * depth * s;
        let cx = hash01(i, 2) * (sw + 160.0 * s) - 80.0 * s + sway;
        let speed = (5.0 + 13.0 * depth) * s;
        let cy = sh + 90.0 * s - (hash01(i, 5) * span + look.drift * speed).rem_euclid(span);
        let spin = (hash01(i, 6) - 0.5) * 0.12;
        let rot = hash01(i, 7) * TAU + look.drift * spin;
        let base = if hash01(i, 8) < 0.5 { look.theme.primary } else { look.theme.secondary };
        let tint = mix(mix(base, WHITE, 0.15 * depth), red, 0.7 * look.danger);
        let a = (0.03 + 0.055 * depth) * (1.0 + 0.6 * look.energy);

        let cells = kind.cells(0);
        let (mx, my) = cells.iter().fold((0.0, 0.0), |(x, y), c| (x + c.0 as f32 / 4.0, y + c.1 as f32 / 4.0));
        let (sin, cos) = rot.sin_cos();
        for (x, y) in cells {
            let (ox, oy) = ((x as f32 - mx) * cell, (y as f32 - my) * cell);
            let (px, py) = (cx + ox * cos - oy * sin, cy + ox * sin + oy * cos);
            let params = |size: f32, c: Color| (size, DrawRectangleParams { offset: vec2(0.5, 0.5), rotation: rot, color: c });
            for (size, p) in [params(cell - 2.0 * s, alpha(tint, a)), params(cell * 0.5, alpha(mix(tint, WHITE, 0.4), a * 0.7))] {
                draw_rectangle_ex(px, py, size, size, p);
            }
        }
    }
}

pub(super) fn draw(v: &View, look: &Look) {
    let (sw, sh) = (screen_width(), screen_height());
    let s = v.scale();
    let t = look.time;
    let base = hex(0x0b0d14);
    let red = hex(0xff3b4f);
    let d = look.danger.clamp(0.0, 1.0);
    let Theme { primary, secondary } = look.theme;

    // Sky: the counter-light at the top, the mode's colour pooling at the bottom.
    let top = mix(mix(base, secondary, 0.11), red, 0.08 * d);
    let bottom = mix(mix(base, primary, 0.15), red, 0.3 * d);
    gradient(0.0, 0.0, sw, sh, top, bottom);

    // Slow lights: one far up-right, one low-left, wandering.
    let w = look.width;
    let a = v.to_screen(w * 0.86 + 50.0 * (t * 0.11).sin(), 140.0 + 40.0 * (t * 0.083).cos());
    glow(a, 420.0 * s, 320.0 * s, alpha(secondary, 0.13));
    let b = v.to_screen(w * 0.08 + 40.0 * (t * 0.07).cos(), 660.0 + 30.0 * (t * 0.09).sin());
    glow(b, 360.0 * s, 280.0 * s, alpha(primary, 0.11));

    drifting_blocks(v, look);

    // The light behind the field, brighter after a clear; danger turns it red and
    // a red tide rises from below, swelling on each heartbeat.
    let (hc, hrx, hry) = look.halo;
    let halo = v.to_screen(hc.x, hc.y);
    let breathe = 1.0 + 0.04 * (t * 0.6).sin();
    glow(halo, hrx * s * breathe, hry * s * breathe, alpha(mix(primary, red, d), 0.2 + 0.15 * look.energy + 0.08 * d));
    if d > 0.0 {
        let tide = v.to_screen(w / 2.0, render::VIRTUAL_H + 60.0);
        glow(tide, w * 0.8 * s, 420.0 * s, alpha(red, d * (0.2 + 0.18 * look.beat)));
    }

    // Vignette: the edges fall off into the dark.
    let dark = Color::new(0.0, 0.0, 0.0, 0.5);
    let clear = Color::new(0.0, 0.0, 0.0, 0.0);
    let (vh, vw) = (sh * 0.16, sw * 0.1);
    gradient(0.0, 0.0, sw, vh, dark, clear);
    gradient(0.0, sh - vh, sw, vh, clear, dark);
    gradient_h(0.0, 0.0, vw, sh, dark, clear);
    gradient_h(sw - vw, 0.0, vw, sh, clear, dark);
}

/// The light behind the main field during play.
pub(super) fn field_halo() -> (Vec2, f32, f32) {
    (vec2(BOARD_X + BOARD_W / 2.0, BOARD_Y + BOARD_H / 2.0), 300.0, 440.0)
}

/// The menu and opponent select: the same scene, lit by `theme` (calm when it's
/// the menu), animated when scene effects are on.
pub fn draw_menu_backdrop(v: &View, width: f32, theme: Theme, on: bool) {
    let t = if on { get_time() as f32 } else { 0.0 };
    draw(
        v,
        &Look { width, time: t, drift: t, theme, danger: 0.0, beat: 0.0, energy: 0.0, halo: (vec2(width / 2.0, 360.0), 320.0, 280.0) },
    );
}
