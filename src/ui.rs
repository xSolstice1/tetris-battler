//! The interface kit: rounded gradient panels with soft shadows and hairline edges,
//! buttons, progress bars, pills, section headers, keycaps and a type scale, shared by
//! every screen so they read as one game.
//!
//! Everything is drawn as small vertex-coloured meshes (a fan for fills, a strip for
//! rings, glows and shadows) into one reused buffer, so nothing allocates per frame.

use std::cell::RefCell;

use macroquad::prelude::*;

use crate::render::{View, hex};

// ───────────── Palette and type scale ─────────────

/// Text, brightest to faintest (weight comes from colour, the font has one weight).
pub const INK: u32 = 0xeef1f8;
pub const TEXT2: u32 = 0x9aa3b8;
pub const TEXT3: u32 = 0x5d6680;
pub const TEXT4: u32 = 0x3d4458;
pub const GOLD: u32 = 0xffd23f;
pub const PINK: u32 = 0xff4f6d;
pub const CYAN: u32 = 0x7fd8ff;
pub const GREEN: u32 = 0x5cdb5c;
pub const ORANGE: u32 = 0xff9a3c;
pub const BLUE: u32 = 0x4f8bff;

/// Font sizes (virtual px).
pub const DISPLAY: f32 = 76.0;
/// Screen titles.
pub const TITLE: f32 = 46.0;
/// Row labels and body copy.
pub const BODY: f32 = 18.0;
pub const CAPS: f32 = 14.0;
pub const NUM: f32 = 30.0;

pub fn c(rgb: u32) -> Color {
    hex(rgb)
}

pub fn a(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

pub fn mix(x: Color, y: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(x.r + (y.r - x.r) * t, x.g + (y.g - x.g) * t, x.b + (y.b - x.b) * t, x.a + (y.a - x.a) * t)
}

fn clear(color: Color) -> Color {
    a(color, 0.0)
}

// ───────────── Geometry ─────────────

/// Segments per rounded corner.
const SEG: usize = 8;
const PERIM: usize = 4 * (SEG + 1);

thread_local! {
    static MESH: RefCell<Mesh> = RefCell::new(Mesh { vertices: Vec::with_capacity(2 * PERIM + 8), indices: Vec::with_capacity(6 * PERIM), texture: None });
}

/// Fills the shared mesh with `build` and draws it.
fn with_mesh(build: impl FnOnce(&mut Vec<Vertex>, &mut Vec<u16>)) {
    MESH.with(|m| {
        let mut m = m.borrow_mut();
        let Mesh { vertices, indices, .. } = &mut *m;
        vertices.clear();
        indices.clear();
        build(vertices, indices);
        if !m.indices.is_empty() {
            draw_mesh(&m);
        }
    });
}

fn vert(v: &View, p: Vec2, color: Color) -> Vertex {
    let s = v.to_screen(p.x, p.y);
    Vertex::new(s.x, s.y, 0.0, 0.0, 0.0, color)
}

/// Point `i` (of `PERIM`, clockwise from the top-left corner) on the outline of a
/// rounded rectangle grown by `d` (negative: shrunk).
fn perim(r: Rect, rad: f32, d: f32, i: usize) -> Vec2 {
    let rad = rad.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let corner = i / (SEG + 1);
    let k = (i % (SEG + 1)) as f32 / SEG as f32;
    let (cx, cy, start) = match corner {
        0 => (r.x + rad, r.y + rad, 180.0f32),
        1 => (r.x + r.w - rad, r.y + rad, 270.0),
        2 => (r.x + r.w - rad, r.y + r.h - rad, 0.0),
        _ => (r.x + rad, r.y + r.h - rad, 90.0),
    };
    let ang = (start + 90.0 * k).to_radians();
    let len = (rad + d).max(0.0);
    vec2(cx + ang.cos() * len, cy + ang.sin() * len)
}

/// A rounded rectangle, each point coloured by `color(point)` (exact for linear ramps).
pub fn fill_with(v: &View, r: Rect, rad: f32, color: impl Fn(Vec2) -> Color) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    with_mesh(|vs, is| {
        let mid = r.center();
        vs.push(vert(v, mid, color(mid)));
        for i in 0..PERIM {
            let p = perim(r, rad, 0.0, i);
            vs.push(vert(v, p, color(p)));
        }
        for i in 0..PERIM {
            is.extend_from_slice(&[0, (i + 1) as u16, ((i + 1) % PERIM + 1) as u16]);
        }
    });
}

/// Vertical gradient fill.
pub fn fill(v: &View, r: Rect, rad: f32, top: Color, bottom: Color) {
    fill_with(v, r, rad, |p| mix(top, bottom, (p.y - r.y) / r.h));
}

/// Horizontal gradient fill.
pub fn fill_h(v: &View, r: Rect, rad: f32, left: Color, right: Color) {
    fill_with(v, r, rad, |p| mix(left, right, (p.x - r.x) / r.w));
}

/// The band between the outline grown by `d_in` and by `d_out`, coloured per point.
pub fn ring_with(v: &View, r: Rect, rad: f32, d_in: f32, d_out: f32, inner: impl Fn(Vec2) -> Color, outer: impl Fn(Vec2) -> Color) {
    with_mesh(|vs, is| {
        for i in 0..PERIM {
            let p = perim(r, rad, d_in, i);
            let q = perim(r, rad, d_out, i);
            vs.push(vert(v, p, inner(p)));
            vs.push(vert(v, q, outer(q)));
        }
        for i in 0..PERIM {
            let j = (i + 1) % PERIM;
            let (pi, qi, pj, qj) = ((2 * i) as u16, (2 * i + 1) as u16, (2 * j) as u16, (2 * j + 1) as u16);
            is.extend_from_slice(&[pi, qi, qj, pi, qj, pj]);
        }
    });
}

/// A hairline (or thicker) border just inside the rectangle.
pub fn stroke(v: &View, r: Rect, rad: f32, t: f32, color: Color) {
    ring_with(v, r, rad, -t, 0.0, |_| color, |_| color);
}

/// A soft glow fading out from the rectangle's edge.
pub fn glow(v: &View, r: Rect, rad: f32, spread: f32, color: Color) {
    ring_with(v, r, rad, 0.0, spread, |_| color, |_| clear(color));
}

/// A soft drop shadow under the rectangle, `dy` below it.
pub fn shadow(v: &View, r: Rect, rad: f32, spread: f32, dy: f32, alpha: f32) {
    let s = Rect::new(r.x, r.y + dy, r.w, r.h);
    let k = Color::new(0.0, 0.0, 0.02, alpha);
    fill_with(v, s, rad, |_| k);
    ring_with(v, s, rad, 0.0, spread, |_| k, |_| clear(k));
}

/// A filled ellipse, `inner` at the centre fading to `outer` at the rim.
pub fn radial(v: &View, cx: f32, cy: f32, rx: f32, ry: f32, inner: Color, outer: Color) {
    with_mesh(|vs, is| {
        const N: usize = 28;
        vs.push(vert(v, vec2(cx, cy), inner));
        for i in 0..N {
            let t = i as f32 / N as f32 * std::f32::consts::TAU;
            vs.push(vert(v, vec2(cx + t.cos() * rx, cy + t.sin() * ry), outer));
        }
        for i in 0..N {
            is.extend_from_slice(&[0, (i + 1) as u16, ((i + 1) % N + 1) as u16]);
        }
    });
}

/// A quad with a colour per corner (top-left, top-right, bottom-right, bottom-left).
pub fn quad(v: &View, p: [Vec2; 4], col: [Color; 4]) {
    with_mesh(|vs, is| {
        for (pt, cl) in p.into_iter().zip(col) {
            vs.push(vert(v, pt, cl));
        }
        is.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    });
}

/// A thin horizontal rule that fades in and out at its ends.
pub fn rule(v: &View, x: f32, y: f32, w: f32, color: Color) {
    let m = x + w / 2.0;
    let h = 1.0;
    quad(v, [vec2(x, y), vec2(m, y), vec2(m, y + h), vec2(x, y + h)], [clear(color), color, color, clear(color)]);
    quad(v, [vec2(m, y), vec2(x + w, y), vec2(x + w, y + h), vec2(m, y + h)], [color, clear(color), clear(color), color]);
}

/// A rule fading out to the right.
pub fn rule_fade(v: &View, x: f32, y: f32, w: f32, color: Color) {
    if w <= 0.0 {
        return;
    }
    quad(v, [vec2(x, y), vec2(x + w, y), vec2(x + w, y + 1.0), vec2(x, y + 1.0)], [color, clear(color), clear(color), color]);
}

// ───────────── Panels ─────────────

/// How a panel looks: corner radius, gradient, hairline edge and drop shadow.
#[derive(Clone, Copy)]
pub struct Panel {
    pub radius: f32,
    pub top: Color,
    pub bottom: Color,
    pub edge: Color,
    pub shadow: f32,
    /// Brightness of the highlight along the top edge.
    pub shine: f32,
}

impl Panel {
    /// The standard card: dark slate, a soft shadow.
    pub fn card() -> Self {
        Panel { radius: 10.0, top: a(c(0x1c2233), 0.94), bottom: a(c(0x11141f), 0.94), edge: Color::new(1.0, 1.0, 1.0, 0.07), shadow: 0.45, shine: 0.10 }
    }

    /// A recessed well (inside a card): darker, no shadow.
    pub fn well() -> Self {
        Panel { radius: 7.0, top: a(c(0x0a0c13), 0.85), bottom: a(c(0x121620), 0.85), edge: Color::new(1.0, 1.0, 1.0, 0.05), shadow: 0.0, shine: 0.0 }
    }

    /// A card in a board theme's panel colours (the in-game hold / next panels).
    pub fn themed(panel: u32, line: u32) -> Self {
        let base = c(panel);
        Panel {
            radius: 10.0,
            top: a(mix(base, WHITE, 0.06), 0.96),
            bottom: a(mix(base, BLACK, 0.25), 0.96),
            edge: a(mix(c(line), WHITE, 0.1), 0.85),
            shadow: 0.4,
            shine: 0.08,
        }
    }

    pub fn radius(self, radius: f32) -> Self {
        Panel { radius, ..self }
    }

    pub fn tint(self, color: Color, k: f32) -> Self {
        Panel { top: a(mix(self.top, color, k), self.top.a), bottom: a(mix(self.bottom, color, k * 0.6), self.bottom.a), ..self }
    }
}

pub fn panel(v: &View, r: Rect, p: &Panel) {
    if p.shadow > 0.0 {
        shadow(v, r, p.radius, 14.0, 5.0, p.shadow);
    }
    fill(v, r, p.radius, p.top, p.bottom);
    if p.edge.a > 0.0 {
        stroke(v, r, p.radius, 1.0, p.edge);
    }
    if p.shine > 0.0 {
        shine(v, r, p.radius, p.shine);
    }
}

/// A highlight along the top edge, fading down the sides.
pub fn shine(v: &View, r: Rect, rad: f32, k: f32) {
    let fade = (rad * 2.0).max(12.0).min(r.h);
    let col = move |p: Vec2| Color::new(1.0, 1.0, 1.0, k * (1.0 - (p.y - r.y) / fade).clamp(0.0, 1.0));
    ring_with(v, r, rad, -1.2, -0.2, col, col);
}

/// A short rounded accent bar down the left edge of `r`.
pub fn accent_bar(v: &View, r: Rect, color: Color) {
    let inset = (r.h * 0.22).min(12.0);
    let b = Rect::new(r.x + 5.0, r.y + inset, 3.0, r.h - 2.0 * inset);
    glow(v, b, 1.5, 6.0, a(color, 0.35));
    fill(v, b, 1.5, mix(color, WHITE, 0.3), color);
}

// ───────────── Buttons ─────────────

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    /// Keyboard focus (the selected item).
    pub focused: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub disabled: bool,
}

impl State {
    pub fn new(focused: bool, hovered: bool) -> Self {
        State { focused, hovered, ..Default::default() }
    }
}

/// A button's background in `state`, with `accent` for focus.
pub fn button_bg(v: &View, r: Rect, rad: f32, st: State, accent: Color) {
    if st.disabled {
        fill(v, r, rad, a(c(0x141721), 0.9), a(c(0x10121a), 0.9));
        stroke(v, r, rad, 1.0, Color::new(1.0, 1.0, 1.0, 0.04));
        return;
    }
    shadow(v, r, rad, 10.0, 4.0, if st.focused { 0.5 } else { 0.35 });
    if st.focused {
        glow(v, r, rad, 10.0, a(accent, 0.22));
    }
    let (top, bottom) = if st.pressed {
        (c(0x10131c), c(0x1a1f2e))
    } else if st.focused {
        (mix(c(0x2a3150), accent, 0.08), c(0x181d2c))
    } else if st.hovered {
        (c(0x242b40), c(0x161a28))
    } else {
        (c(0x1b2030), c(0x11141e))
    };
    fill(v, r, rad, a(top, 0.96), a(bottom, 0.96));
    let edge = if st.focused { a(accent, 0.9) } else if st.hovered { Color::new(1.0, 1.0, 1.0, 0.2) } else { Color::new(1.0, 1.0, 1.0, 0.08) };
    stroke(v, r, rad, if st.focused { 1.5 } else { 1.0 }, edge);
    if !st.pressed {
        shine(v, r, rad, if st.focused { 0.16 } else { 0.09 });
    }
    if st.focused {
        accent_bar(v, r, accent);
    }
}

/// The label colour for a button in `state`.
pub fn label_color(st: State) -> Color {
    if st.disabled {
        c(TEXT4)
    } else if st.focused {
        WHITE
    } else if st.hovered {
        c(INK)
    } else {
        c(TEXT2)
    }
}

/// Baseline for text of `size` centred vertically on `cy`.
pub fn baseline(cy: f32, size: f32) -> f32 {
    cy + size * 0.34
}

/// A button with a centred label.
pub fn button(v: &View, r: Rect, label: &str, size: f32, st: State, accent: Color) {
    button_bg(v, r, (r.h * 0.2).min(10.0), st, accent);
    let dy = if st.pressed { 1.0 } else { 0.0 };
    if st.focused {
        v.text_centered(label, r.x + r.w / 2.0 + 1.0, baseline(r.center().y, size) + 2.0 + dy, size, Color::new(0.0, 0.0, 0.0, 0.45));
    }
    v.text_centered(label, r.x + r.w / 2.0, baseline(r.center().y, size) + dy, size, label_color(st));
}

// ───────────── Progress bars ─────────────

#[derive(Clone, Copy)]
pub struct Bar {
    /// Fill colour at the start and at the end of the track.
    pub from: Color,
    pub to: Color,
    /// Tick marks dividing the track (0: none).
    pub ticks: u32,
    /// Glow at the fill's leading edge (0..1).
    pub glow: f32,
    /// The gradient spans the filled part (the tip is always `to`), not the track.
    pub stretch: bool,
}

impl Bar {
    pub fn new(from: Color, to: Color) -> Self {
        Bar { from, to, ticks: 0, glow: 0.6, stretch: false }
    }

    pub fn stretch(self) -> Self {
        Bar { stretch: true, ..self }
    }

    pub fn ticks(self, ticks: u32) -> Self {
        Bar { ticks, ..self }
    }

    pub fn glow(self, glow: f32) -> Self {
        Bar { glow, ..self }
    }

    /// The XP bar.
    pub fn xp() -> Self {
        Bar::new(c(0xff9a3c), c(GOLD))
    }
}

/// A rounded track and its gradient fill, `frac` full.
pub fn progress(v: &View, r: Rect, frac: f32, bar: &Bar) {
    let rad = r.h / 2.0;
    let frac = frac.clamp(0.0, 1.0);
    // Track: a recessed groove.
    fill(v, r, rad, a(c(0x05070b), 0.9), a(c(0x161a26), 0.9));
    stroke(v, r, rad, 1.0, Color::new(1.0, 1.0, 1.0, 0.06));
    if frac > 0.0 {
        let fw = (r.w * frac).max(r.h.min(r.w));
        let f = Rect::new(r.x, r.y, fw, r.h);
        let (from, to) = (bar.from, bar.to);
        let tw = if bar.stretch { fw } else { r.w };
        fill_with(v, f, rad, |p| {
            let base = mix(from, to, (p.x - r.x) / tw);
            // Lit from above.
            mix(base, BLACK, 0.22 * ((p.y - r.y) / r.h))
        });
        // Gloss on the upper half.
        let g = Rect::new(f.x + 1.0, f.y + 1.0, (f.w - 2.0).max(0.0), f.h * 0.45);
        fill(v, g, g.h / 2.0, Color::new(1.0, 1.0, 1.0, 0.35), Color::new(1.0, 1.0, 1.0, 0.04));
        if bar.glow > 0.0 && frac < 1.0 {
            let tip = if bar.stretch { to } else { mix(from, to, frac) };
            let (ex, ey) = (f.right() - rad * 0.6, r.center().y);
            radial(v, ex, ey, r.h * 1.8, r.h * 1.3, a(mix(tip, WHITE, 0.3), 0.55 * bar.glow), clear(tip));
            v.rect(f.right() - 2.0, f.y + 1.0, 1.5, f.h - 2.0, a(WHITE, 0.6 * bar.glow));
        }
    }
    for i in 1..bar.ticks {
        let x = r.x + r.w * i as f32 / bar.ticks as f32;
        let inside = x < r.x + r.w * frac;
        v.rect(x - 0.5, r.y + 2.0, 1.0, r.h - 4.0, if inside { Color::new(0.0, 0.0, 0.0, 0.3) } else { Color::new(1.0, 1.0, 1.0, 0.07) });
    }
}

// ───────────── Text ─────────────

fn px(v: &View, size: f32) -> f32 {
    (size * v.scale()).max(1.0)
}

/// Width of letter-spaced text.
pub fn caps_w(v: &View, t: &str, size: f32, spacing: f32) -> f32 {
    let p = px(v, size);
    let mut w = 0.0;
    let mut n = 0;
    for (i, ch) in t.char_indices() {
        w += crate::render::text_px_width(&t[i..i + ch.len_utf8()], None, p) / v.scale();
        n += 1;
    }
    w + spacing * (n.max(1) - 1) as f32
}

/// Letter-spaced text (labels, headers, chips) from `x`.
pub fn caps(v: &View, t: &str, x: f32, y: f32, size: f32, color: Color, spacing: f32) {
    let p = px(v, size);
    let mut cx = x;
    for (i, ch) in t.char_indices() {
        let s = &t[i..i + ch.len_utf8()];
        v.text(s, cx, y, size, color);
        cx += crate::render::text_px_width(s, None, p) / v.scale() + spacing;
    }
}

pub fn caps_centered(v: &View, t: &str, cx: f32, y: f32, size: f32, color: Color, spacing: f32) {
    caps(v, t, cx - caps_w(v, t, size, spacing) / 2.0, y, size, color, spacing);
}

pub fn caps_right(v: &View, t: &str, right: f32, y: f32, size: f32, color: Color, spacing: f32) {
    caps(v, t, right - caps_w(v, t, size, spacing), y, size, color, spacing);
}

/// Width of plain text.
pub fn text_w(v: &View, t: &str, size: f32) -> f32 {
    crate::render::text_px_width(t, None, px(v, size)) / v.scale()
}

/// The largest size, up to `size`, at which `t` fits in `width`.
pub fn fit(v: &View, t: &str, size: f32, width: f32) -> f32 {
    let w = text_w(v, t, size);
    if w > width { size * width / w } else { size }
}

/// Text over a soft drop shadow (titles, big numbers).
pub fn text_shadow(v: &View, t: &str, x: f32, y: f32, size: f32, color: Color) {
    let o = (size / 26.0).max(1.0);
    v.text(t, x + o * 0.5, y + o, size, Color::new(0.0, 0.0, 0.03, 0.55 * color.a));
    v.text(t, x, y, size, color);
}

pub fn text_shadow_centered(v: &View, t: &str, cx: f32, y: f32, size: f32, color: Color) {
    text_shadow(v, t, cx - text_w(v, t, size) / 2.0, y, size, color);
}

pub fn text_shadow_right(v: &View, t: &str, right: f32, y: f32, size: f32, color: Color) {
    text_shadow(v, t, right - text_w(v, t, size), y, size, color);
}

/// A section header: letter-spaced small caps and a thin rule running to `x + w`.
pub fn section(v: &View, x: f32, y: f32, w: f32, label: &str, color: Color) {
    caps(v, label, x, y, CAPS, color, 2.0);
    let lw = caps_w(v, label, CAPS, 2.0);
    let ry = y - CAPS * 0.36;
    rule_fade(v, x + lw + 8.0, ry, w - lw - 8.0, a(color, 0.3));
}

// ───────────── Pills, keycaps ─────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pill {
    /// Tinted background, coloured text.
    Soft,
    /// Solid colour, dark text (the loud ones: LEVEL UP, NEW UNLOCK).
    Solid,
}

fn pill_metrics(v: &View, t: &str, size: f32) -> (f32, f32) {
    let h = size + 9.0;
    (caps_w(v, t, size, 1.0) + size * 1.3, h)
}

/// Width of a pill.
pub fn pill_w(v: &View, t: &str, size: f32) -> f32 {
    pill_metrics(v, t, size).0
}

/// A rounded chip from `x`, centred on `cy`; returns its width.
pub fn pill(v: &View, x: f32, cy: f32, t: &str, size: f32, color: Color, style: Pill) -> f32 {
    let (w, h) = pill_metrics(v, t, size);
    let r = Rect::new(x, cy - h / 2.0, w, h);
    match style {
        Pill::Soft => {
            fill(v, r, h / 2.0, a(color, 0.2 * color.a), a(color, 0.1 * color.a));
            stroke(v, r, h / 2.0, 1.0, a(color, 0.5 * color.a));
            caps(v, t, x + size * 0.65, baseline(cy, size), size, a(mix(color, WHITE, 0.25), color.a), 1.0);
        }
        Pill::Solid => {
            glow(v, r, h / 2.0, 7.0, a(color, 0.3 * color.a));
            fill(v, r, h / 2.0, a(mix(color, WHITE, 0.25), color.a), a(mix(color, BLACK, 0.12), color.a));
            shine(v, r, h / 2.0, 0.35);
            caps(v, t, x + size * 0.65, baseline(cy, size), size, a(c(0x14110a), color.a), 1.0);
        }
    }
    w
}

pub fn pill_centered(v: &View, cx: f32, cy: f32, t: &str, size: f32, color: Color, style: Pill) {
    pill(v, cx - pill_w(v, t, size) / 2.0, cy, t, size, color, style);
}

pub fn pill_right(v: &View, right: f32, cy: f32, t: &str, size: f32, color: Color, style: Pill) -> f32 {
    pill(v, right - pill_w(v, t, size), cy, t, size, color, style)
}

fn keycap_metrics(v: &View, t: &str, size: f32) -> (f32, f32) {
    let h = size + 9.0;
    ((text_w(v, t, size) + size * 0.9).max(h), h)
}

pub fn keycap_w(v: &View, t: &str, size: f32) -> f32 {
    keycap_metrics(v, t, size).0
}

/// A key, as on a keyboard: a raised cap with a darker lip. Returns its width.
pub fn keycap(v: &View, x: f32, cy: f32, t: &str, size: f32) -> f32 {
    let (w, h) = keycap_metrics(v, t, size);
    let r = Rect::new(x, cy - h / 2.0, w, h - 2.0);
    fill(v, Rect::new(x, r.y + 2.0, w, h - 2.0), 4.0, c(0x0d0f16), c(0x0a0b10));
    fill(v, r, 4.0, c(0x2c3247), c(0x1d2131));
    stroke(v, r, 4.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.1));
    v.text_centered(t, x + w / 2.0, baseline(r.center().y, size), size, c(INK));
    w
}

/// Footer hints: keycaps and what they do, centred on `cx`. A key may list several
/// caps separated by `/`.
pub fn hints(v: &View, cx: f32, cy: f32, items: &[(&str, &str)], size: f32) {
    let gap = size * 1.3;
    let item_w = |keys: &str, what: &str| {
        let caps: f32 = keys.split('/').map(|k| keycap_w(v, k, size) + 3.0).sum();
        caps + 4.0 + text_w(v, what, size)
    };
    let total: f32 = items.iter().map(|(k, w)| item_w(k, w)).sum::<f32>() + gap * (items.len().max(1) - 1) as f32;
    let mut x = cx - total / 2.0;
    for (keys, what) in items {
        for k in keys.split('/') {
            x += keycap(v, x, cy, k, size) + 3.0;
        }
        x += 4.0;
        v.text(what, x, baseline(cy, size), size, c(TEXT3));
        x += text_w(v, what, size) + gap;
    }
}

/// A level badge: a gold disc with the number, centred on (cx, cy).
pub fn level_badge(v: &View, cx: f32, cy: f32, r: f32, level: u32, color: Color) {
    let rect = Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r);
    glow(v, rect, r, 8.0, a(color, 0.25));
    fill(v, rect, r, mix(color, WHITE, 0.35), mix(color, BLACK, 0.25));
    stroke(v, rect, r, 1.5, a(mix(color, WHITE, 0.6), 0.9));
    let inner = Rect::new(cx - r + 3.0, cy - r + 3.0, 2.0 * r - 6.0, 2.0 * r - 6.0);
    stroke(v, inner, r - 3.0, 1.0, Color::new(0.0, 0.0, 0.0, 0.25));
    let mut buf = [0u8; 4];
    let t = small_number(level, &mut buf);
    let size = if level >= 10 { r * 1.05 } else { r * 1.2 };
    v.text_centered(t, cx, baseline(cy, size), size, c(0x1a1405));
}

/// Formats `n` (< 10000) without allocating.
fn small_number(n: u32, buf: &mut [u8; 4]) -> &str {
    let n = n.min(9999);
    let mut digits = 0;
    let mut m = n;
    loop {
        digits += 1;
        m /= 10;
        if m == 0 {
            break;
        }
    }
    let mut m = n;
    for i in (0..digits).rev() {
        buf[i] = b'0' + (m % 10) as u8;
        m /= 10;
    }
    std::str::from_utf8(&buf[..digits]).unwrap_or("")
}

/// A chevron pointing left (`dir` -1) or right (1), centred on (cx, cy).
pub fn chevron(v: &View, cx: f32, cy: f32, size: f32, dir: f32, color: Color) {
    let (w, h) = (size * 0.5 * dir, size);
    let t = size * 0.2 * dir;
    let p = [vec2(cx - w / 2.0, cy - h / 2.0), vec2(cx + w / 2.0, cy), vec2(cx - w / 2.0, cy + h / 2.0)];
    // A thick "<" / ">" from two quads.
    quad(v, [p[0], vec2(p[0].x + t, p[0].y), vec2(p[1].x + t, p[1].y), p[1]], [color; 4]);
    quad(v, [p[1], vec2(p[1].x + t, p[1].y), vec2(p[2].x + t, p[2].y), p[2]], [color; 4]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_numbers_format() {
        let mut b = [0u8; 4];
        assert_eq!(small_number(0, &mut b), "0");
        assert_eq!(small_number(7, &mut b), "7");
        assert_eq!(small_number(42, &mut b), "42");
        assert_eq!(small_number(1234, &mut b), "1234");
    }

    #[test]
    fn the_outline_runs_clockwise_around_the_corners() {
        let r = Rect::new(0.0, 0.0, 100.0, 40.0);
        let first = perim(r, 10.0, 0.0, 0);
        assert!((first - vec2(0.0, 10.0)).length() < 1e-3);
        for i in 0..PERIM {
            let p = perim(r, 10.0, 0.0, i);
            assert!(p.x >= -1e-3 && p.x <= 100.001 && p.y >= -1e-3 && p.y <= 40.001, "{p:?}");
        }
        // Grown outlines stay outside.
        let p = perim(r, 10.0, 5.0, SEG / 2 + (SEG + 1));
        assert!(p.y < 0.0);
    }
}
