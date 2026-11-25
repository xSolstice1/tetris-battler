//! Drawing. Everything is laid out in a fixed virtual canvas and scaled to fit
//! the window (letterboxed), so resizing just works.

use macroquad::prelude::*;

use crate::board::{Cell, HIDDEN_ROWS, TOTAL_ROWS, VISIBLE_ROWS, WIDTH};
use crate::battle::{self, Ability, Battle, Defeat, Phase};
use crate::monster_art::{self, Pixel};
use crate::fx::{Fx, Motion};
use crate::game::Game;
use crate::piece::PieceKind;
use crate::stats::{self, Stats};
use crate::bot::Profile;
use crate::versus::{Phase as VsPhase, Versus};
use crate::progress::{self, Progress, Reward};
use crate::scene::{self, Scene, Theme};
use crate::skins::{self, BlockStyle, Item, ThemeColors};

/// Virtual canvas width for Classic and the menu; Battler adds the monster panel.
const CLASSIC_W: f32 = 760.0;
const BATTLE_W: f32 = 1100.0;
pub(crate) const VIRTUAL_H: f32 = 760.0;
pub const CELL: f32 = 32.0;
const MINI: f32 = 20.0;
pub const BOARD_X: f32 = 220.0;
pub const BOARD_Y: f32 = 80.0;
pub const BOARD_W: f32 = WIDTH as f32 * CELL;
pub(crate) const BOARD_H: f32 = VISIBLE_ROWS as f32 * CELL;
const LEFT_X: f32 = 40.0;
pub(crate) const RIGHT_X: f32 = BOARD_X + BOARD_W + 30.0;
pub(crate) const PANEL_W: f32 = 150.0;

pub fn hex(rgb: u32) -> Color {
    Color::from_rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn shade(c: Color, f: f32) -> Color {
    Color::new(c.r * f, c.g * f, c.b * f, c.a)
}

pub fn cell_color(cell: Cell) -> Color {
    match cell {
        Cell::Piece(kind) => piece_color(kind),
        Cell::Garbage => hex(0x5a6275),
    }
}

/// A piece's colour in the equipped palette.
pub fn piece_color(kind: PieceKind) -> Color {
    let colors = skins::current().palette.colors();
    let i = match kind {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    };
    hex(colors[i])
}

/// The equipped board theme's colours.
fn theme() -> ThemeColors {
    skins::current().theme.colors()
}

/// Maps virtual coordinates to screen pixels, plus a shake offset (virtual px).
pub struct View {
    scale: f32,
    ox: f32,
    oy: f32,
}

impl View {
    fn fit_window(shake: Vec2, virtual_w: f32) -> Self {
        let scale = (screen_width() / virtual_w).min(screen_height() / VIRTUAL_H);
        View {
            scale,
            ox: (screen_width() - virtual_w * scale) / 2.0 + shake.x * scale,
            oy: (screen_height() - VIRTUAL_H * scale) / 2.0 + shake.y * scale,
        }
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        let s = self.scale;
        draw_rectangle(self.ox + x * s, self.oy + y * s, w * s, h * s, c);
    }

    pub(crate) fn outline(&self, x: f32, y: f32, w: f32, h: f32, thickness: f32, c: Color) {
        let s = self.scale;
        draw_rectangle_lines(self.ox + x * s, self.oy + y * s, w * s, h * s, thickness * s, c);
    }

    pub(crate) fn text(&self, t: &str, x: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        draw_text(t, self.ox + x * s, self.oy + y * s, size * s, c);
    }

    pub(crate) fn text_right(&self, t: &str, right: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let dims = measure_text(t, None, (size * s).max(1.0) as u16, 1.0);
        draw_text(t, self.ox + right * s - dims.width, self.oy + y * s, size * s, c);
    }

    pub fn text_centered(&self, t: &str, cx: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let dims = measure_text(t, None, (size * s).max(1.0) as u16, 1.0);
        draw_text(t, self.ox + cx * s - dims.width / 2.0, self.oy + y * s, size * s, c);
    }

    fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        let s = self.scale;
        draw_ellipse(self.ox + cx * s, self.oy + cy * s, rx * s, ry * s, 0.0, c);
    }

    pub(crate) fn circle(&self, cx: f32, cy: f32, r: f32, c: Color) {
        let s = self.scale;
        draw_circle(self.ox + cx * s, self.oy + cy * s, r * s, c);
    }

    pub(crate) fn circle_lines(&self, cx: f32, cy: f32, r: f32, thickness: f32, c: Color) {
        let s = self.scale;
        draw_circle_lines(self.ox + cx * s, self.oy + cy * s, r * s, thickness * s, c);
    }

    fn texture(&self, t: &Texture2D, x: f32, y: f32, w: f32, h: f32, tint: Color) {
        let s = self.scale;
        draw_texture_ex(t, self.ox + x * s, self.oy + y * s, tint, DrawTextureParams { dest_size: Some(vec2(w * s, h * s)), ..Default::default() });
    }

    /// Text that may mix ASCII (the game's font) and Chinese (a small embedded subset).
    fn text_mixed(&self, t: &str, x: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let px = (size * s).max(1.0) as u16;
        let mut cx = self.ox + x * s;
        for (run, cjk) in runs(t) {
            let font = if cjk { cjk_font() } else { None };
            draw_text_ex(run, cx, self.oy + y * s, TextParams { font: font.as_ref(), font_size: px, color: c, ..Default::default() });
            cx += measure_text(run, font.as_ref(), px, 1.0).width;
        }
    }

    /// Width of `text_mixed` in virtual units.
    fn measure_mixed(&self, t: &str, size: f32) -> f32 {
        let s = self.scale;
        let px = (size * s).max(1.0) as u16;
        runs(t).map(|(run, cjk)| measure_text(run, if cjk { cjk_font() } else { None }.as_ref(), px, 1.0).width).sum::<f32>() / s
    }

    fn text_mixed_centered(&self, t: &str, cx: f32, y: f32, size: f32, c: Color) {
        self.text_mixed(t, cx - self.measure_mixed(t, size) / 2.0, y, size, c);
    }

    fn text_mixed_right(&self, t: &str, right: f32, y: f32, size: f32, c: Color) {
        self.text_mixed(t, right - self.measure_mixed(t, size), y, size, c);
    }

    fn triangle(&self, a: Vec2, b: Vec2, c: Vec2, color: Color) {
        let s = self.scale;
        let o = vec2(self.ox, self.oy);
        draw_triangle(o + a * s, o + b * s, o + c * s, color);
    }

    /// A frame of thickness `t` just inside the rectangle.
    fn frame(&self, x: f32, y: f32, w: f32, h: f32, t: f32, c: Color) {
        self.rect(x, y, w, t, c);
        self.rect(x, y + h - t, w, t, c);
        self.rect(x, y + t, t, h - 2.0 * t, c);
        self.rect(x + w - t, y + t, t, h - 2.0 * t, c);
    }

    /// A rectangle with rounded corners of radius `r`.
    fn rounded(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        self.rect(x + r, y, w - 2.0 * r, h, c);
        self.rect(x, y + r, r, h - 2.0 * r, c);
        self.rect(x + w - r, y + r, r, h - 2.0 * r, c);
        for (cx, cy) in [(x + r, y + r), (x + w - r, y + r), (x + r, y + h - r), (x + w - r, y + h - r)] {
            self.circle(cx, cy, r, c);
        }
    }

    /// One block in the equipped style.
    pub fn block(&self, x: f32, y: f32, size: f32, c: Color) {
        self.block_styled(skins::current().style, x, y, size, c);
    }

    /// One block in `style`. Every style is still: nothing here depends on time.
    pub fn block_styled(&self, style: BlockStyle, x: f32, y: f32, size: f32, c: Color) {
        let a = c.a;
        let lift = |f: f32| with_alpha(mix_colors(c, WHITE, f), a);
        // Edge thickness that stays visible on the small queue / opponent cells.
        let edge = (size / 16.0).max(1.0);
        match style {
            BlockStyle::Classic => {
                self.rect(x, y, size, size, shade(c, 0.55));
                self.rect(x + 2.0, y + 2.0, size - 4.0, size - 4.0, c);
                self.rect(x + 2.0, y + 2.0, size - 4.0, (size - 4.0) * 0.22, shade(c, 1.25));
            }
            BlockStyle::Flat => {
                // A modern flat tile: inset, with a darker lip along the bottom.
                let m = edge * 1.6;
                let (w, h) = (size - 2.0 * m, size - 2.0 * m);
                self.rect(x + m, y + m, w, h, shade(c, 0.7));
                self.rect(x + m, y + m, w, h * 0.8, c);
            }
            BlockStyle::Outline => {
                let t = (size / 11.0).max(1.0);
                self.rect(x + t, y + t, size - 2.0 * t, size - 2.0 * t, with_alpha(c, 0.16 * a));
                self.frame(x + edge, y + edge, size - 2.0 * edge, size - 2.0 * edge, t, c);
            }
            BlockStyle::Glossy => {
                let r = size * 0.22;
                let m = edge * 0.75;
                let (w, h) = (size - 2.0 * m, size - 2.0 * m);
                self.rounded(x + m, y + m, w, h, r, shade(c, 0.72));
                self.rounded(x + m, y + m, w, h * 0.86, r, c);
                // The shine: a soft band near the top and a bright glint.
                let sx = x + m + w * 0.16;
                self.rounded(sx, y + m + h * 0.1, w * 0.68, h * 0.26, (h * 0.13).min(r), with_alpha(WHITE, 0.32 * a));
                self.circle(x + m + w * 0.26, y + m + h * 0.23, size * 0.06, with_alpha(WHITE, 0.75 * a));
            }
            BlockStyle::Pixel => {
                // An 8-bit brick: black outline, flat colour, a checker dither shading the
                // lower right, and a white highlight in the corner, all in whole "pixels".
                let p = (size / 8.0).round().max(1.0);
                let n = (size / p) as i32;
                self.rect(x, y, size, size, with_alpha(BLACK, a));
                self.rect(x + p, y + p, size - 2.0 * p, size - 2.0 * p, c);
                let dark = with_alpha(shade(c, 0.45), a);
                for py in 1..n - 1 {
                    for px in 1..n - 1 {
                        if px + py > n && (px + py) % 2 == 0 {
                            self.rect(x + px as f32 * p, y + py as f32 * p, p, p, dark);
                        }
                    }
                }
                let white = with_alpha(WHITE, a);
                self.rect(x + p, y + p, 2.0 * p, p, white);
                self.rect(x + p, y + 2.0 * p, p, p, white);
            }
            BlockStyle::Neon => {
                let t = (size / 10.0).max(1.0);
                self.rect(x, y, size, size, with_alpha(c, 0.16 * a));
                self.rect(x + t, y + t, size - 2.0 * t, size - 2.0 * t, with_alpha(shade(c, 0.22), a));
                self.frame(x + t, y + t, size - 2.0 * t, size - 2.0 * t, t, lift(0.3));
                let i = 2.5 * t;
                self.frame(x + i, y + i, size - 2.0 * i, size - 2.0 * i, (t * 0.5).max(1.0), with_alpha(c, 0.45 * a));
            }
            BlockStyle::Deon => {
                let green = with_alpha(hex(0x5cdb5c), a);
                self.rect(x, y, size, size, with_alpha(hex(0x081a0e), a));
                self.frame(x + edge * 0.5, y + edge * 0.5, size - edge, size - edge, edge, with_alpha(green, 0.85 * a));
                // Traces from the chip out to the edges, and the chip itself.
                let mid = size / 2.0;
                let tw = (size / 18.0).max(1.0);
                self.rect(x + mid - tw / 2.0, y + edge, tw, size - 2.0 * edge, with_alpha(green, 0.45 * a));
                self.rect(x + edge, y + mid - tw / 2.0, size - 2.0 * edge, tw, with_alpha(green, 0.45 * a));
                let chip = size * 0.5;
                let o = (size - chip) / 2.0;
                self.rect(x + o - edge, y + o - edge, chip + 2.0 * edge, chip + 2.0 * edge, green);
                self.rect(x + o, y + o, chip, chip, c);
                self.rect(x + o, y + o, chip, chip * 0.25, lift(0.3));
            }
            BlockStyle::Crystal => {
                let m = edge * 0.5;
                let (x0, y0, x1, y1) = (x + m, y + m, x + size - m, y + size - m);
                let (i0, i1) = (size * 0.28, size * 0.72);
                let (tl, tr, bl, br) = (vec2(x0, y0), vec2(x1, y0), vec2(x0, y1), vec2(x1, y1));
                let (itl, itr, ibl, ibr) = (vec2(x + i0, y + i0), vec2(x + i1, y + i0), vec2(x + i0, y + i1), vec2(x + i1, y + i1));
                let facets = [
                    ([tl, tr, itr, itl], lift(0.45)),
                    ([tl, itl, ibl, bl], lift(0.15)),
                    ([tr, br, ibr, itr], shade(c, 0.75)),
                    ([bl, ibl, ibr, br], shade(c, 0.55)),
                ];
                for ([p0, p1, p2, p3], col) in facets {
                    self.triangle(p0, p1, p2, col);
                    self.triangle(p0, p2, p3, col);
                }
                self.rect(x + i0, y + i0, i1 - i0, i1 - i0, c);
                self.triangle(itl, itr, ibl, lift(0.12));
            }
        }
    }

    /// Where the falling piece will land, in the equipped style.
    pub fn ghost(&self, x: f32, y: f32, size: f32, c: Color) {
        let style = skins::current().style;
        let t = (size / 16.0).max(1.0);
        let m = t;
        let (gx, gy, g) = (x + m, y + m, size - 2.0 * m);
        match style {
            BlockStyle::Flat => self.rect(gx, gy, g, g, with_alpha(c, 0.24)),
            BlockStyle::Outline | BlockStyle::Pixel => {
                // Corner brackets.
                let k = g * 0.32;
                let w = 2.0 * t;
                let col = with_alpha(c, 0.6);
                self.rect(gx, gy, g, g, with_alpha(c, 0.06));
                for (left, top) in [(true, true), (false, true), (true, false), (false, false)] {
                    let hx = if left { gx } else { gx + g - k };
                    let vx = if left { gx } else { gx + g - w };
                    let hy = if top { gy } else { gy + g - w };
                    let vy = if top { gy } else { gy + g - k };
                    self.rect(hx, hy, k, w, col);
                    self.rect(vx, vy, w, k, col);
                }
            }
            BlockStyle::Neon => {
                self.rect(gx, gy, g, g, with_alpha(c, 0.08));
                self.frame(gx - t, gy - t, g + 2.0 * t, g + 2.0 * t, t, with_alpha(c, 0.22));
                self.frame(gx, gy, g, g, t * 1.5, with_alpha(mix_colors(c, WHITE, 0.3), 0.7));
            }
            BlockStyle::Deon => {
                self.rect(gx, gy, g, g, with_alpha(hex(0x5cdb5c), 0.08));
                self.frame(gx, gy, g, g, t * 1.5, with_alpha(hex(0x5cdb5c), 0.5));
                let chip = size * 0.3;
                self.frame(x + (size - chip) / 2.0, y + (size - chip) / 2.0, chip, chip, t, with_alpha(c, 0.6));
            }
            BlockStyle::Classic | BlockStyle::Glossy | BlockStyle::Crystal => {
                self.rect(gx, gy, g, g, with_alpha(c, 0.12));
                self.outline(gx, gy, g, g, t, with_alpha(c, 0.5));
            }
        }
    }
}

/// A field (frame, background, grid and decoration) in board `theme`, `rows` cells high
/// from (x, y), cells `cell` wide. `frame` overrides the frame colour (clear flashes,
/// danger). Every theme has its own construction, not just its own colours, and all of it
/// is still and faint enough to keep the stack readable.
fn draw_board_theme(v: &View, theme: skins::BoardTheme, x: f32, y: f32, cell: f32, rows: usize, frame: Option<Color>) {
    use skins::BoardTheme as T;
    let c = theme.colors();
    let (w, h) = (WIDTH as f32 * cell, rows as f32 * cell);
    let frame = frame.unwrap_or(hex(c.frame));
    let field = hex(c.field);
    let grid = hex(c.grid);
    let k = cell / CELL; // decoration scale (preview vs game)
    // Vertical gradient in bands.
    let gradient = |top: Color, bottom: Color| {
        const BANDS: usize = 24;
        for i in 0..BANDS {
            let t = i as f32 / (BANDS - 1) as f32;
            v.rect(x, y + h * i as f32 / BANDS as f32, w, h / BANDS as f32 + 0.5, mix_colors(top, bottom, t));
        }
    };
    let grid_lines = |col: Color, vertical: bool, horizontal: bool| {
        if vertical {
            for gx in 1..WIDTH {
                v.rect(x + gx as f32 * cell, y, 1.0, h, col);
            }
        }
        if horizontal {
            for gy in 1..rows {
                v.rect(x, y + gy as f32 * cell, w, 1.0, col);
            }
        }
    };
    let dots = |col: Color, r: f32| {
        for gx in 1..WIDTH {
            for gy in 1..rows {
                v.circle(x + gx as f32 * cell, y + gy as f32 * cell, r, col);
            }
        }
    };
    match theme {
        T::Midnight => {
            v.rect(x - 4.0 * k, y - 4.0 * k, w + 8.0 * k, h + 8.0 * k, frame);
            v.rect(x, y, w, h, field);
            grid_lines(grid, true, true);
        }
        T::Paper => {
            // A notebook page on a dark desk: soft shadow, a card frame, ruled lines and a
            // red margin, no vertical grid.
            v.rect(x - 6.0 * k + 8.0 * k, y - 6.0 * k + 10.0 * k, w + 12.0 * k, h + 12.0 * k, Color::new(0.0, 0.0, 0.0, 0.45));
            v.rect(x - 6.0 * k, y - 6.0 * k, w + 12.0 * k, h + 12.0 * k, frame);
            v.rect(x - 3.0 * k, y - 3.0 * k, w + 6.0 * k, h + 6.0 * k, mix_colors(frame, WHITE, 0.25));
            v.rect(x, y, w, h, field);
            for gy in 1..rows {
                v.rect(x, y + gy as f32 * cell, w, 1.5 * k.max(0.7), Color::new(0.45, 0.6, 0.85, 0.45));
            }
            v.rect(x + 0.35 * cell, y, 1.5 * k.max(0.7), h, Color::new(0.9, 0.3, 0.3, 0.55));
            // Punched holes down the left edge.
            for i in 0..(rows / 4).max(1) {
                v.circle(x - 3.0 * k, y + (i as f32 * 4.0 + 2.0) * cell, 3.5 * k, hex(c.backdrop));
            }
        }
        T::Sakura => {
            // Rounded double frame in blossom pink, a dot grid, scattered petals.
            v.rounded(x - 7.0 * k, y - 7.0 * k, w + 14.0 * k, h + 14.0 * k, 10.0 * k, frame);
            v.rounded(x - 5.0 * k, y - 5.0 * k, w + 10.0 * k, h + 10.0 * k, 8.0 * k, hex(c.backdrop));
            v.rounded(x - 3.0 * k, y - 3.0 * k, w + 6.0 * k, h + 6.0 * k, 6.0 * k, with_alpha(frame, 0.7));
            v.rect(x, y, w, h, field);
            dots(with_alpha(frame, 0.35), 1.6 * k);
            for i in 0..(14.0 * rows as f32 / 20.0) as usize {
                let px = x + hash01(i, 1, 31) * w;
                let py = y + hash01(i, 2, 31) * h;
                let r = (2.5 + 2.5 * hash01(i, 3, 31)) * k;
                for p in 0..5 {
                    let a = p as f32 / 5.0 * std::f32::consts::TAU + hash01(i, 4, 31) * 6.0;
                    v.circle(px + a.cos() * r, py + a.sin() * r, r * 0.75, Color::new(1.0, 0.72, 0.84, 0.10));
                }
                v.circle(px, py, r * 0.4, Color::new(1.0, 0.9, 0.6, 0.12));
            }
        }
        T::Terminal => {
            // A CRT console: scanlines, a dotted grid and bright bracket corners.
            v.rect(x - 3.0 * k, y - 3.0 * k, w + 6.0 * k, h + 6.0 * k, with_alpha(frame, 0.35));
            v.rect(x, y, w, h, field);
            let mut sy = y;
            while sy < y + h {
                v.rect(x, sy, w, 1.0, Color::new(0.2, 1.0, 0.4, 0.045));
                sy += 3.0 * k.max(0.6);
            }
            for gx in 1..WIDTH {
                for gy in 0..rows * 4 {
                    v.rect(x + gx as f32 * cell, y + gy as f32 * cell / 4.0, 1.0, 1.0, with_alpha(grid, 0.9));
                }
            }
            let (l, t) = (18.0 * k, 3.0 * k);
            for (cx, cy, sx, sy) in [(x - 6.0 * k, y - 6.0 * k, 1.0, 1.0), (x + w + 6.0 * k, y - 6.0 * k, -1.0, 1.0), (x - 6.0 * k, y + h + 6.0 * k, 1.0, -1.0), (x + w + 6.0 * k, y + h + 6.0 * k, -1.0, -1.0)] {
                let hx = if sx > 0.0 { cx } else { cx - l };
                let vy = if sy > 0.0 { cy } else { cy - l };
                v.rect(hx, if sy > 0.0 { cy } else { cy - t }, l, t, frame);
                v.rect(if sx > 0.0 { cx } else { cx - t }, vy, t, l, frame);
            }
            v.text(">_", x + 6.0 * k, y + h - 8.0 * k, 16.0 * k, Color::new(0.3, 1.0, 0.5, 0.25));
        }
        T::Burrow => {
            // Packed earth inside a frame of rough wooden planks; pebbles and roots, no grid.
            let plank = 9.0 * k;
            let woods = [hex(0x6b4a2e), hex(0x7d5836), hex(0x5a3d25)];
            let mut i = 0;
            let mut px = x - plank;
            while px < x + w + plank {
                let pw = (20.0 + 14.0 * hash01(i, 0, 41)) * k;
                v.rect(px, y - plank, pw.min(x + w + plank - px), plank, woods[i % 3]);
                v.rect(px, y + h, pw.min(x + w + plank - px), plank, woods[(i + 1) % 3]);
                px += pw;
                i += 1;
            }
            let mut py = y;
            while py < y + h {
                let ph = (26.0 + 18.0 * hash01(i, 1, 41)) * k;
                v.rect(x - plank, py, plank, ph.min(y + h - py), woods[i % 3]);
                v.rect(x + w, py, plank, ph.min(y + h - py), woods[(i + 2) % 3]);
                py += ph;
                i += 1;
            }
            gradient(field, mix_colors(field, BLACK, 0.35));
            for i in 0..(70.0 * rows as f32 / 20.0) as usize {
                let sx = x + hash01(i, 5, 43) * w;
                let sy = y + hash01(i, 6, 43) * h;
                let r = (1.0 + 2.5 * hash01(i, 7, 43)) * k;
                let col = if i % 3 == 0 { Color::new(0.75, 0.6, 0.4, 0.14) } else { Color::new(0.0, 0.0, 0.0, 0.18) };
                v.circle(sx, sy, r, col);
            }
        }
        T::Jade => {
            // Polished jade in a gold double frame, with pale veins through the stone.
            let gold = hex(0xd4af37);
            v.rect(x - 7.0 * k, y - 7.0 * k, w + 14.0 * k, h + 14.0 * k, frame);
            v.rect(x - 5.0 * k, y - 5.0 * k, w + 10.0 * k, h + 10.0 * k, gold);
            v.rect(x - 3.0 * k, y - 3.0 * k, w + 6.0 * k, h + 6.0 * k, mix_colors(gold, BLACK, 0.55));
            v.rect(x - 1.5 * k, y - 1.5 * k, w + 3.0 * k, h + 3.0 * k, gold);
            v.rect(x, y, w, h, field);
            for vein in 0..6 {
                let mut p = vec2(x + hash01(vein, 0, 53) * w, y);
                let step = 10.0 * k;
                while p.y < y + h {
                    let q = vec2((p.x + (hash01(vein, (p.y / step) as usize, 54) - 0.45) * 14.0 * k).clamp(x, x + w), p.y + step);
                    v.triangle(p, q, q + vec2(1.4 * k, 0.0), Color::new(0.7, 1.0, 0.85, 0.09));
                    p = q;
                }
            }
            grid_lines(with_alpha(grid, 0.6), true, true);
        }
        T::Glacier => {
            // Ice: light at the top fading into deep blue, frost crystals, icicles on the frame.
            let ice = hex(0xbfe8ff);
            v.rect(x - 3.0 * k, y - 3.0 * k, w + 6.0 * k, h + 6.0 * k, frame);
            gradient(mix_colors(field, ice, 0.22), mix_colors(field, BLACK, 0.3));
            grid_lines(with_alpha(grid, 0.8), true, true);
            for i in 0..(10.0 * rows as f32 / 20.0) as usize {
                let cx = x + hash01(i, 1, 61) * w;
                let cy = y + hash01(i, 2, 61).powi(2) * h * 0.6;
                let r = (3.0 + 5.0 * hash01(i, 3, 61)) * k;
                for a in 0..3 {
                    let ang = a as f32 * std::f32::consts::PI / 3.0;
                    let d = vec2(ang.cos(), ang.sin()) * r;
                    let n = vec2(-d.y, d.x).normalize_or_zero() * 0.6 * k;
                    v.triangle(vec2(cx, cy) - d - n, vec2(cx, cy) + d - n, vec2(cx, cy) + d + n, Color::new(0.85, 0.95, 1.0, 0.16));
                }
            }
            for i in 0..9 {
                let ix = x + (i as f32 + 0.5) / 9.0 * w;
                let len = (6.0 + 12.0 * hash01(i, 9, 62)) * k;
                v.triangle(vec2(ix - 4.0 * k, y - 3.0 * k), vec2(ix + 4.0 * k, y - 3.0 * k), vec2(ix, y - 3.0 * k + len), with_alpha(ice, 0.55));
            }
        }
        T::Sunset => {
            // Synthwave: a dusk gradient, a striped sun low on the horizon, a two-tone frame.
            v.rect(x - 6.0 * k, y - 6.0 * k, w + 12.0 * k, h + 12.0 * k, hex(0xff7a45));
            v.rect(x - 3.0 * k, y - 3.0 * k, w + 6.0 * k, h + 6.0 * k, hex(0xc0306b));
            if frame != hex(c.frame) {
                v.rect(x - 6.0 * k, y - 6.0 * k, w + 12.0 * k, 3.0 * k, frame);
            }
            gradient(hex(0x1c0b2e), hex(0x4a1630));
            let (sx, sy, r) = (x + w / 2.0, y + h * 0.78, w * 0.36);
            v.circle(sx, sy, r, Color::new(1.0, 0.45, 0.35, 0.18));
            v.circle(sx, sy, r * 0.82, Color::new(1.0, 0.75, 0.3, 0.14));
            for i in 0..6 {
                let by = sy - r * 0.1 + i as f32 * r * 0.16;
                v.rect(sx - r, by, 2.0 * r, (2.0 + i as f32 * 1.2) * k, mix_colors(hex(0x1c0b2e), hex(0x4a1630), (by - y) / h));
            }
            for gx in 1..WIDTH {
                v.rect(x + gx as f32 * cell, y + h * 0.82, 1.0, h * 0.18, Color::new(1.0, 0.4, 0.7, 0.22));
            }
            v.rect(x, y + h * 0.82, w, 1.0, Color::new(1.0, 0.5, 0.75, 0.4));
            grid_lines(Color::new(1.0, 0.5, 0.8, 0.06), true, true);
        }
        T::Inferno => {
            // Hellfire: a heavy scorched frame with a glowing inner edge, embers rising from a
            // lava glow at the bottom, flames along the floor and cracks of light.
            v.rect(x - 8.0 * k, y - 8.0 * k, w + 16.0 * k, h + 16.0 * k, hex(0x2a0805));
            v.rect(x - 4.0 * k, y - 4.0 * k, w + 8.0 * k, h + 8.0 * k, frame);
            v.rect(x - 2.0 * k, y - 2.0 * k, w + 4.0 * k, h + 4.0 * k, hex(0xffb347));
            gradient(hex(0x0a0202), hex(0x3a0a05));
            v.rect(x, y + h * 0.85, w, h * 0.15, Color::new(1.0, 0.35, 0.05, 0.14));
            for i in 0..12 {
                let fx = x + (i as f32 + 0.5) / 12.0 * w;
                let fh = (20.0 + 46.0 * hash01(i, 1, 71)) * k;
                let fw = (10.0 + 8.0 * hash01(i, 2, 71)) * k;
                v.triangle(vec2(fx - fw, y + h), vec2(fx + fw, y + h), vec2(fx + (hash01(i, 3, 71) - 0.5) * 8.0 * k, y + h - fh), Color::new(1.0, 0.4, 0.05, 0.16));
            }
            for i in 0..(18.0 * rows as f32 / 20.0) as usize {
                v.circle(x + hash01(i, 4, 72) * w, y + h * (0.3 + 0.7 * hash01(i, 5, 72)), (1.0 + 1.5 * hash01(i, 6, 72)) * k, Color::new(1.0, 0.6, 0.2, 0.22));
            }
            grid_lines(with_alpha(grid, 0.55), true, true);
        }
    }
}

/// Splits text into runs of ASCII and of other (Chinese) characters, flagging the latter.
fn runs(t: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut rest = t;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let cjk = !first.is_ascii();
        let end = rest.char_indices().find(|&(_, ch)| ch.is_ascii() == cjk).map_or(rest.len(), |(i, _)| i);
        let (run, tail) = rest.split_at(end);
        rest = tail;
        Some((run, cjk))
    })
}

/// The characters in the embedded subset (keep in step with assets/fonts/README.md).
#[cfg(test)]
const CJK_CHARS: &str = "哥";

/// The embedded Chinese subset (see assets/fonts/README.md), loaded once.
fn cjk_font() -> Option<Font> {
    thread_local! {
        static FONT: std::cell::OnceCell<Option<Font>> = const { std::cell::OnceCell::new() };
    }
    FONT.with(|f| f.get_or_init(|| load_ttf_font_from_bytes(include_bytes!("../assets/fonts/cjk-subset.ttf")).ok()).clone())
}

/// Virtual position of board cell (x, y). Row `HIDDEN_ROWS` is the top visible row.
pub fn cell_pos(x: i32, y: i32) -> (f32, f32) {
    (
        BOARD_X + x as f32 * CELL,
        BOARD_Y + (y - HIDDEN_ROWS as i32) as f32 * CELL,
    )
}

/// Rows of the buffer above the field that are drawn (pieces spawn and can rest there).
pub const BUFFER_SHOWN: usize = 3;

/// The spawn rows and the buffer just above the field are drawn too, so a stack that has
/// grown into the buffer (and freshly spawned pieces) can be seen.
fn drawable_row(y: i32) -> bool {
    y >= (HIDDEN_ROWS - BUFFER_SHOWN) as i32
}

fn draw_mini_piece(v: &View, kind: PieceKind, cx: f32, cy: f32, color: Color) {
    draw_piece_sized(v, kind, cx, cy, MINI, color);
}

/// A piece in its spawn orientation, centred on (cx, cy), cells `MINI` wide.
fn draw_piece_sized(v: &View, kind: PieceKind, cx: f32, cy: f32, mini: f32, color: Color) {
    let cells = kind.cells(0);
    let (min_x, max_x) = cells.iter().fold((i32::MAX, i32::MIN), |(lo, hi), c| (lo.min(c.0), hi.max(c.0)));
    let (min_y, max_y) = cells.iter().fold((i32::MAX, i32::MIN), |(lo, hi), c| (lo.min(c.1), hi.max(c.1)));
    let w = (max_x - min_x + 1) as f32 * mini;
    let h = (max_y - min_y + 1) as f32 * mini;
    for (x, y) in cells {
        v.block(
            cx - w / 2.0 + (x - min_x) as f32 * mini,
            cy - h / 2.0 + (y - min_y) as f32 * mini,
            mini,
            color,
        );
    }
}

fn panel(v: &View, label: &str, x: f32, y: f32, w: f32, h: f32) {
    v.text(label, x, y - 8.0, 22.0, hex(0x9aa3b8));
    let t = theme();
    v.rect(x, y, w, h, hex(t.panel));
    v.outline(x, y, w, h, 2.0, hex(t.panel_line));
}

/// A play session. `reward` is what the finished run earned (shown on the results).
pub fn draw(game: &Game, paused: bool, fx: &Fx, scene: &Scene, sound_on: bool, music_on: bool, battle: Option<&Battle>, versus: Option<&Versus>, reward: Option<&Reward>) {
    let theme = theme();
    clear_background(hex(theme.backdrop));
    let width = if battle.is_some() || versus.is_some() { BATTLE_W } else { CLASSIC_W };
    let v = View::fit_window(fx.shake_offset(), width);
    scene.draw_back(&v, width);
    // Scene effects off: nothing off the field moves on its own.
    let scene_still = !scene.on();
    let field_bg = hex(theme.field);
    // Motion off: nothing on screen moves on its own.
    let still = !fx.motion().animated();

    // Danger: the closer the stack is to the top, the stronger and faster the field pulses
    // red (motion off: a steady tint).
    let danger = crate::danger::level(game);
    let pulse = if still { 0.6 } else { 0.5 + 0.5 * (get_time() as f32 * (4.0 + 7.0 * danger)).sin() };
    let red = hex(0xff3b4f);

    // Field frame (flashes with the last clear's colour, red in danger) and background.
    let frame = mix_colors(fx.border_color(hex(theme.frame)), red, danger * (0.45 + 0.55 * pulse));
    if danger > 0.0 {
        // A soft red glow around the field.
        for (i, a) in [(14.0, 0.06), (9.0, 0.1), (5.0, 0.16)] {
            v.rect(BOARD_X - i, BOARD_Y - i, BOARD_W + 2.0 * i, BOARD_H + 2.0 * i, with_alpha(red, a * danger * (0.4 + 0.6 * pulse)));
        }
    }
    let _ = field_bg;
    draw_board_theme(&v, skins::current().theme, BOARD_X, BOARD_Y, CELL, VISIBLE_ROWS, Some(frame));
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, with_alpha(red, 0.12 * danger * (0.5 + 0.5 * pulse)));

    // Settled stack, offset while it collapses after a clear. During a hard drop's
    // flight, show the field as it was before the lock.
    let board = fx.frozen_board().unwrap_or(&game.board);
    let death = fx.death_progress();
    // The buffer above the field: a faint band once anything is up there, so it reads as
    // "still alive, but over the top".
    if (HIDDEN_ROWS - BUFFER_SHOWN..HIDDEN_ROWS).any(|y| (0..WIDTH).any(|x| board.get(x, y).is_some())) {
        let h = BUFFER_SHOWN as f32 * CELL;
        v.rect(BOARD_X, BOARD_Y - h, BOARD_W, h, with_alpha(hex(0xff3b4f), 0.10));
        v.rect(BOARD_X, BOARD_Y - h, BOARD_W, 1.0, with_alpha(hex(0xff3b4f), 0.35));
    }
    for y in HIDDEN_ROWS - BUFFER_SHOWN..TOTAL_ROWS {
        let offset = fx.row_offset(y);
        for x in 0..WIDTH {
            if let Some(cell) = board.get(x, y) {
                let (px, py) = cell_pos(x as i32, y as i32);
                if let Some(k) = death {
                    draw_crumbling_cell(&v, px, py, x, y, cell, k);
                    continue;
                }
                let c = if game.game_over { hex(0x4a5064) } else { cell_color(cell) };
                v.block(px, py + offset, CELL, c);
            }
        }
    }

    fx.draw_field(&v);

    // Incoming garbage meter, left of the field: one cell per queued line.
    let pending = game.pending_garbage();
    if pending > 0 {
        let rows = pending.min(VISIBLE_ROWS as u32) as f32;
        let h = rows * CELL;
        let danger = pending >= 4;
        let pulse = if danger && !still { 0.75 + 0.25 * (get_time() as f32 * 12.0).sin() } else { 1.0 };
        v.rect(BOARD_X - 14.0, BOARD_Y + BOARD_H - h, 8.0, h, with_alpha(hex(0xff4f6d), pulse));
    }

    if !game.game_over {
        let a = game.active;
        let color = piece_color(a.kind);
        // The ghost is computed on the real board, so hide it while the last hard
        // drop is still falling over the pre-lock board (it would float or overlap).
        let ghost_y = game.ghost_y();
        let show_ghost = fx.frozen_board().is_none();
        for (cx, cy) in a.kind.cells(a.rot) {
            if show_ghost && drawable_row(ghost_y + cy) {
                let (px, py) = cell_pos(a.x + cx, ghost_y + cy);
                v.ghost(px, py, CELL, color);
            }
        }
        // Dims as the lock delay runs out, so "about to stick" is readable.
        let lock = game.lock_progress();
        let active_color = shade(color, 1.0 - 0.45 * lock);
        let glide = fx.active_offset(a.y);
        for (x, y) in a.cells() {
            if drawable_row(y) {
                let (px, py) = cell_pos(x, y);
                v.block(px, py + glide, CELL, active_color);
            }
        }
    }

    // In danger, an outline shows where the next piece will appear (red if it can't).
    if danger >= 0.5 && !game.game_over {
        let kind = game.next[0];
        let blocked = crate::danger::next_spawn_blocked(game);
        let c = if blocked { red } else { mix_colors(piece_color(kind), red, 0.5) };
        let a = if blocked { 0.5 + 0.5 * pulse } else { 0.35 + 0.35 * pulse };
        for (cx, cy) in kind.cells(0) {
            let (px, py) = cell_pos(3 + cx, HIDDEN_ROWS as i32 - 1 + cy);
            v.outline(px + 3.0, py + 3.0, CELL - 6.0, CELL - 6.0, 3.0, with_alpha(c, a));
        }
    }

    // Title + hold + stats on the left.
    v.text("BLOCK", LEFT_X, 40.0, 34.0, hex(0xffd23f));
    v.text("BRAWL", LEFT_X + 88.0, 40.0, 34.0, hex(0xff4f6d));

    panel(&v, "HOLD", LEFT_X, BOARD_Y, PANEL_W, 100.0);
    if let Some(kind) = game.hold {
        let c = if game.hold_used { hex(0x4a5064) } else { piece_color(kind) };
        draw_mini_piece(&v, kind, LEFT_X + PANEL_W / 2.0, BOARD_Y + 50.0, c);
    }

    // Score and lines tick up (scene effects), brighter while they're counting.
    let counting = if scene.ticking(game) { mix_colors(WHITE, hex(0xffd23f), 0.45) } else { WHITE };
    let stats = [
        ("SCORE", scene.score().to_string(), counting),
        ("LINES", scene.lines().to_string(), WHITE),
        ("LEVEL", game.level.to_string(), WHITE),
    ];
    for (i, (label, value, color)) in stats.iter().enumerate() {
        let y = BOARD_Y + 160.0 + i as f32 * 70.0;
        v.text(label, LEFT_X, y, 22.0, hex(0x9aa3b8));
        v.text(value, LEFT_X, y + 32.0, 34.0, *color);
    }

    // Live performance stats.
    let s = &game.stats;
    let live = [
        ("TIME", stats::clock(s.time)),
        ("PIECES", s.pieces.to_string()),
        ("PPS", stats::fmt_rate(s.pps(), 2)),
        ("APM", stats::fmt_rate(s.apm(), 1)),
        ("VS", stats::fmt_rate(s.vs(), 1)),
        ("KPP", stats::fmt_rate(s.kpp(), 2)),
    ];
    stat_rows(&v, LEFT_X, LEFT_X + PANEL_W, BOARD_Y + 370.0, &live, 28.0, 20.0);

    let motion_y = BOARD_Y + 570.0;
    v.text("M  motion:", LEFT_X, motion_y, 18.0, hex(0x5d6680));
    v.text(fx.motion().label(), LEFT_X + 84.0, motion_y, 18.0, hex(0x9aa3b8));
    v.text("E  scene:", LEFT_X, motion_y + 22.0, 18.0, hex(0x5d6680));
    v.text(if scene.on() { "on" } else { "off" }, LEFT_X + 84.0, motion_y + 22.0, 18.0, hex(0x9aa3b8));
    v.text("N  sound:", LEFT_X, motion_y + 44.0, 18.0, hex(0x5d6680));
    v.text(if sound_on { "on" } else { "off" }, LEFT_X + 84.0, motion_y + 44.0, 18.0, hex(0x9aa3b8));
    v.text("J  music:", LEFT_X, motion_y + 66.0, 18.0, hex(0x5d6680));
    v.text(if music_on { "on" } else { "off" }, LEFT_X + 84.0, motion_y + 66.0, 18.0, hex(0x9aa3b8));

    // Next queue on the right.
    let slot = 80.0;
    let queue_h = slot * game.next.len() as f32 + 10.0;
    panel(&v, "NEXT", RIGHT_X, BOARD_Y, PANEL_W, queue_h);
    if battle.is_some_and(Battle::next_hidden) {
        // Fog ability: the queue is hidden behind drifting murk.
        let t = if scene_still { 0.0 } else { get_time() as f32 };
        for i in 0..6 {
            let y = BOARD_Y + 8.0 + i as f32 * (queue_h - 16.0) / 6.0;
            let drift = (t * 1.5 + i as f32).sin() * 10.0;
            v.rect(RIGHT_X + 6.0 + drift.max(0.0), y, PANEL_W - 12.0 - drift.abs(), queue_h / 6.0, hex(0x2a2f40));
        }
        v.text_centered("? ? ?", RIGHT_X + PANEL_W / 2.0, BOARD_Y + queue_h / 2.0, 34.0, hex(0x9aa3b8));
    } else {
        for (i, &kind) in game.next.iter().enumerate() {
            draw_mini_piece(
                &v,
                kind,
                RIGHT_X + PANEL_W / 2.0,
                BOARD_Y + 45.0 + i as f32 * slot,
                piece_color(kind),
            );
        }
    }

    scene.draw_side(&v);

    if let Some(b) = battle {
        draw_battle_panel(&v, b, scene_still);
        if let Phase::Intermission { .. } = b.phase {
            let next = battle::monster(b.monster.tier + 1);
            v.rect(BOARD_X, BOARD_Y + 250.0, BOARD_W, 120.0, Color::new(0.0, 0.0, 0.0, 0.6));
            v.text_centered("DEFEATED!", BOARD_X + BOARD_W / 2.0, BOARD_Y + 305.0, 48.0, hex(0xffd23f));
            v.text_centered(
                &format!("next: {}", next.title()),
                BOARD_X + BOARD_W / 2.0,
                BOARD_Y + 345.0,
                22.0,
                hex(0x9aa3b8),
            );
        }
    }

    if let Some(vs) = versus {
        draw_versus_panel(&v, vs, scene_still);
        scene.draw_shots(&v);
    }

    fx.draw_top(&v);

    // Overlays.
    if let Some(vs) = versus {
        match vs.phase {
            VsPhase::Over { player_won } if fx.death_done() => draw_versus_results(&v, vs, player_won, &game.stats, reward),
            VsPhase::Over { .. } => {}
            VsPhase::Countdown { left } => draw_countdown(&v, vs, left, still),
            VsPhase::Fighting if paused => draw_pause(&v),
            VsPhase::Fighting if still => {}
            VsPhase::Fighting => draw_go(&v, vs),
        }
        return;
    }
    match battle {
        _ if !fx.death_done() => {}
        Some(b) if b.over() || game.game_over => draw_battle_results(&v, b, &game.stats, reward),
        _ if game.game_over => draw_classic_results(&v, game, reward),
        _ if paused => draw_pause(&v),
        _ => {}
    }
}

const PANEL_X: f32 = RIGHT_X + PANEL_W + 30.0;
const MONSTER_PANEL_W: f32 = 320.0;

/// Horizontal bar: background, fill fraction, border.
fn bar(v: &View, x: f32, y: f32, w: f32, h: f32, frac: f32, fill: Color) {
    v.rect(x, y, w, h, hex(0x10131c));
    v.rect(x, y, w * frac.clamp(0.0, 1.0), h, fill);
    v.outline(x, y, w, h, 2.0, hex(0x262c3d));
}

/// Seconds before a monster attack during which it visibly winds up.
const ATTACK_WARN: f32 = 1.0;
const MONSTER_PIXEL: f32 = 12.0;

/// Deterministic 0..1 noise per sprite pixel (for the shatter on death).
fn hash01(x: usize, y: usize, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(668_265_263)
        ^ salt.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

fn pixel_color(p: Pixel, body: Color, accent: Color, blink: bool) -> Color {
    let bg = hex(0x0b0d14);
    let outline = mix_colors(body, bg, 0.78);
    let shadow = mix_colors(body, bg, 0.38);
    match p {
        Pixel::Outline => outline,
        Pixel::Body => body,
        Pixel::Highlight => mix_colors(body, WHITE, 0.4),
        Pixel::Shadow => shadow,
        Pixel::Dark => mix_colors(body, bg, 0.7),
        Pixel::Accent => accent,
        // Closed eyes: the white becomes skin, the pupil an eyelid line.
        Pixel::EyeWhite if blink => shadow,
        Pixel::Pupil if blink => outline,
        Pixel::EyeWhite => hex(0xf4f6ff),
        Pixel::Pupil => hex(0x10131c),
    }
}

/// Ability auras, drawn behind the monster.
fn draw_aura(v: &View, b: &Battle, cx: f32, mid_y: f32, t: f32) {
    if b.shield > 0 {
        let r = 100.0 + (t * 2.0).sin() * 3.0;
        v.circle(cx, mid_y, r, Color::new(0.31, 0.55, 1.0, 0.10));
        v.circle_lines(cx, mid_y, r, 3.0, Color::new(0.45, 0.65, 1.0, 0.55));
    }
    if !b.ability_in_effect() && !b.regenerating() {
        return;
    }
    match b.monster.ability {
        Some(Ability::Haste) => {
            // Speed lines streaking past.
            for i in 0..7 {
                let y = mid_y - 70.0 + i as f32 * 22.0;
                let x = cx + 130.0 - (t * 650.0 + i as f32 * 97.0) % 280.0;
                v.rect(x, y, 44.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.45));
            }
        }
        Some(Ability::Fog) => {
            for i in 0..6 {
                let a = t * 0.7 + i as f32 * 1.05;
                let (x, y) = (cx + a.cos() * 95.0, mid_y + (a * 1.3).sin() * 55.0);
                v.circle(x, y, 30.0 + (i % 3) as f32 * 8.0, Color::new(0.6, 0.64, 0.72, 0.18));
            }
        }
        Some(Ability::Regen) => {
            // Little green pluses drifting up.
            for i in 0..5 {
                let phase = (t * 0.8 + i as f32 * 0.2) % 1.0;
                let (x, y) = (cx - 80.0 + i as f32 * 40.0, mid_y + 70.0 - phase * 150.0);
                let c = Color::new(0.36, 0.86, 0.36, 1.0 - phase);
                v.rect(x - 6.0, y - 2.0, 12.0, 4.0, c);
                v.rect(x - 2.0, y - 6.0, 4.0, 12.0, c);
            }
        }
        Some(Ability::Shield) | None => {}
    }
}

/// Draws the monster standing with its feet at `feet_y`.
fn draw_monster(v: &View, b: &Battle, cx: f32, feet_y: f32, still: bool) {
    use monster_art::{H, W};
    let t = if still { 0.0 } else { get_time() as f32 };
    let m = &b.monster;
    let body = hex(m.color);
    let accent = hex(m.art.accent);
    let fighting = b.phase == Phase::Fighting;
    let sprite_h = H as f32 * MONSTER_PIXEL;

    // Death: 0 -> 1 over the break between monsters.
    let dying = match b.phase {
        Phase::Intermission { left } => Some(1.0 - left / battle::INTERMISSION),
        _ => None,
    };
    // Motion off: no hit flash or knockback, no breathing, lunging, trembling or blinking,
    // and a defeated monster simply disappears instead of shattering.
    if still && dying.is_some() {
        return;
    }
    let recent_hit = if still { f32::MAX } else { b.hits.last().map_or(f32::MAX, |&(_, age)| age) };
    let flash = (1.0 - recent_hit / 0.12).max(0.0);
    let knock = (1.0 - recent_hit / 0.25).max(0.0);
    // Winding up for a garbage attack: tint red and tremble.
    let warn = if fighting && b.attack_timer < ATTACK_WARN { 1.0 - b.attack_timer / ATTACK_WARN } else { 0.0 };
    let warn_pulse = if still { 0.8 } else { 0.6 + 0.4 * (t * 18.0).sin() };
    // Just attacked (or just arrived): a quick lunge.
    let since_attack = m.attack_every - b.attack_timer;
    let lunge = if fighting && !still { (1.0 - since_attack / 0.25).max(0.0) } else { 0.0 };

    let breathe = if still { 0.0 } else { (t * 3.0).sin() };
    let sx = 1.0 - 0.03 * breathe + 0.12 * lunge;
    let sy = 1.0 + 0.035 * breathe + 0.08 * lunge;
    let x0 = cx + warn * (t * 55.0).sin() * 3.0 + knock * knock * 12.0;
    let blink = !still && ((t % 3.7) < 0.12 || recent_hit < 0.2);

    if dying.is_none() {
        draw_aura(v, b, cx, feet_y - sprite_h / 2.0, t);
        v.ellipse(cx, feet_y + 4.0, 70.0 * sx, 9.0, Color::new(0.0, 0.0, 0.0, 0.35));
    }

    let (pw, ph) = (MONSTER_PIXEL * sx, MONSTER_PIXEL * sy);
    for y in 0..H {
        for x in 0..W {
            let Some(p) = m.art.pixel(x, y) else {
                continue;
            };
            let mut c = pixel_color(p, body, accent, blink);
            c = mix_colors(c, hex(0xff3b4f), 0.45 * warn * warn_pulse);
            c = mix_colors(c, WHITE, flash);
            let mut px = x0 + (x as f32 - W as f32 / 2.0) * pw;
            let mut py = feet_y - (H - y) as f32 * ph;
            let mut alpha = 1.0;
            if let Some(d) = dying {
                // Shatter: every pixel flies off and falls, fading out.
                c = mix_colors(c, WHITE, (1.0 - d / 0.08).max(0.0));
                let secs = d * battle::INTERMISSION * 0.7;
                let vx = (hash01(x, y, 1) - 0.5) * 260.0;
                let vy = -(60.0 + hash01(x, y, 2) * 220.0);
                px += vx * secs;
                py += vy * secs + 500.0 * secs * secs;
                alpha = (1.0 - d * 2.0).max(0.0);
            }
            v.rect(px, py, pw + 0.6, ph + 0.6, Color { a: alpha, ..c });
        }
    }

    // Damage numbers float up from the monster (motion off: they hold still, then vanish).
    for &(damage, age) in &b.hits {
        let k = if still { 0.0 } else { age / battle::HIT_POPUP_TIME };
        let alpha = 1.0 - k * k;
        let size = 30.0 + damage.min(20) as f32;
        let y = feet_y - sprite_h - 10.0 - k * 60.0;
        v.text_centered(&format!("-{damage}"), cx + 80.0, y, size, Color { a: alpha, ..hex(0xff4f6d) });
    }
}

fn mix_colors(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a)
}

fn draw_battle_panel(v: &View, b: &Battle, still: bool) {
    let (x, w) = (PANEL_X, MONSTER_PANEL_W);
    let cx = x + w / 2.0;
    let m = &b.monster;
    let muted = hex(0x9aa3b8);

    v.text(&format!("MONSTER {}", m.tier), x, BOARD_Y - 8.0, 22.0, muted);
    // Backdrop tinted toward the monster's colour, deepest at the floor.
    const BANDS: usize = 40;
    let band_h = 300.0 / BANDS as f32;
    for i in 0..BANDS {
        let k = i as f32 / (BANDS - 1) as f32;
        let c = mix_colors(hex(0x141824), hex(m.color), 0.02 + 0.10 * k * k);
        v.rect(x, BOARD_Y + i as f32 * band_h, w, band_h + 0.5, c);
    }
    let feet_y = BOARD_Y + 238.0;
    v.rect(x + 20.0, feet_y + 8.0, w - 40.0, 2.0, with_alpha(hex(m.color), 0.15));
    v.outline(x, BOARD_Y, w, 300.0, 2.0, hex(0x262c3d));
    v.text_centered(&m.title(), cx, BOARD_Y + 40.0, 34.0, hex(m.color));
    draw_monster(v, b, cx, feet_y, still);
    if let Some(ability) = m.ability {
        let active = b.ability_in_effect();
        let pulse = if still { 1.0 } else { 0.6 + 0.4 * (get_time() as f32 * 8.0).sin().abs() };
        let color = if active { with_alpha(hex(0xff4f6d), pulse) } else { hex(0x7fd8ff) };
        let label = if active { format!("{}!", ability.name()) } else { ability.name().to_owned() };
        v.text_centered(&label, cx, BOARD_Y + 262.0, 24.0, color);
        v.text_centered(ability.description(), cx, BOARD_Y + 286.0, 18.0, hex(0x5d6680));
    }

    // HP.
    let hp_y = BOARD_Y + 320.0;
    v.text("HP", x, hp_y + 18.0, 22.0, muted);
    bar(v, x + 44.0, hp_y, w - 44.0, 24.0, b.hp as f32 / m.max_hp as f32, hex(0x5cdb5c));
    v.text_centered(&format!("{} / {}", b.hp, m.max_hp), x + 44.0 + (w - 44.0) / 2.0, hp_y + 18.0, 20.0, WHITE);
    if b.shield > 0 {
        let frac = b.shield as f32 / m.max_hp as f32;
        bar(v, x + 44.0, hp_y + 30.0, w - 44.0, 10.0, frac, hex(0x4f8bff));
        v.text(&format!("shield {}", b.shield), x + 44.0, hp_y + 58.0, 18.0, hex(0x4f8bff));
    }

    // Time left.
    let time_y = hp_y + 80.0;
    let secs = b.time_left.ceil() as u32;
    let urgent = b.time_left < 10.0 && b.phase == Phase::Fighting;
    let pulse = if urgent && !still { 0.6 + 0.4 * (get_time() as f32 * 10.0).sin().abs() } else { 1.0 };
    let time_color = if urgent { with_alpha(hex(0xff4f6d), pulse) } else { WHITE };
    v.text("TIME", x, time_y, 22.0, muted);
    v.text(&format!("{}:{:02}", secs / 60, secs % 60), x + 70.0, time_y + 4.0, 44.0, time_color);

    // Next garbage attack.
    let atk_y = time_y + 40.0;
    v.text("ATTACK", x, atk_y + 18.0, 22.0, muted);
    let charge = 1.0 - b.attack_timer / m.attack_every;
    let fighting = b.phase == Phase::Fighting;
    bar(v, x + 90.0, atk_y, w - 90.0, 24.0, if fighting { charge } else { 0.0 }, hex(0xff9a3c));
    v.text(
        &format!(
            "{} row{} every {:.1}s",
            m.attack_lines,
            if m.attack_lines == 1 { "" } else { "s" },
            m.attack_every
        ),
        x,
        atk_y + 50.0,
        18.0,
        hex(0x5d6680),
    );

    let stats_y = atk_y + 100.0;
    v.text("DEFEATED", x, stats_y, 22.0, muted);
    v.text(&b.defeated.to_string(), x, stats_y + 32.0, 34.0, WHITE);
    v.text("DAMAGE", x + 160.0, stats_y, 22.0, muted);
    v.text(&b.total_damage.to_string(), x + 160.0, stats_y + 32.0, 34.0, WHITE);
}

fn draw_battle_results(v: &View, b: &Battle, stats: &Stats, reward: Option<&Reward>) {
    let cx = BOARD_X + BOARD_W / 2.0;
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.78));
    let title = match b.phase {
        Phase::Over(Defeat::Timeout) => "TIME UP",
        _ => "TOPPED OUT",
    };
    v.text_centered(title, cx, BOARD_Y + 110.0, 52.0, hex(0xff4f6d));
    let lines = [
        ("monsters defeated", b.defeated.to_string()),
        ("total damage", b.total_damage.to_string()),
        ("fell to", format!("{} (lv {})", b.monster.title(), b.monster.tier)),
    ];
    for (i, (label, value)) in lines.iter().enumerate() {
        let y = BOARD_Y + 160.0 + i as f32 * 54.0;
        v.text_centered(label, cx, y, 20.0, hex(0x9aa3b8));
        v.text_centered(value, cx, y + 28.0, 28.0, WHITE);
    }
    let rows = summary_rows(stats);
    stat_rows(v, BOARD_X + 40.0, BOARD_X + BOARD_W - 40.0, BOARD_Y + 340.0, &rows[2..8], 24.0, 18.0);
    if let Some(r) = reward {
        draw_reward(v, r, BOARD_Y + 490.0);
    }
    v.text_centered("Enter retry   ·   Esc menu", cx, BOARD_Y + 628.0, 20.0, hex(0x9aa3b8));
}

/// XP earned, the level bar, records and new unlocks, in a block starting at `top`
/// (about 100 tall) across the field.
fn draw_reward(v: &View, r: &Reward, top: f32) {
    let cx = BOARD_X + BOARD_W / 2.0;
    let gold = hex(0xffd23f);
    let (left, w) = (BOARD_X + 30.0, BOARD_W - 60.0);
    v.rect(left - 10.0, top - 4.0, w + 20.0, 1.0, hex(0x3d4458));
    v.text(&format!("+{} XP", r.xp), left, top + 22.0, 24.0, gold);
    let level = if r.leveled_up() { format!("LEVEL UP!  {}", r.level) } else { format!("LEVEL {}", r.level) };
    v.text_right(&level, left + w, top + 22.0, 22.0, if r.leveled_up() { gold } else { WHITE });
    bar(v, left, top + 30.0, w, 8.0, r.into as f32 / r.need.max(1) as f32, gold);
    let mut y = top + 60.0;
    if !r.records.is_empty() {
        v.text_centered(&r.records.join("  ·  "), cx, y, 18.0, hex(0x7fd8ff));
        y += 24.0;
    }
    match r.unlocks.as_slice() {
        [] => {}
        [one] => {
            v.text_centered("NEW UNLOCK", cx, y, 18.0, hex(0xff9a3c));
            v.text_centered(&format!("{} {}", one.name(), one.kind()), cx, y + 24.0, 22.0, WHITE);
        }
        many => {
            v.text_centered(&format!("{} NEW UNLOCKS", many.len()), cx, y, 18.0, hex(0xff9a3c));
            let names: Vec<&str> = many.iter().take(3).map(|i| i.name()).collect();
            let more = if many.len() > 3 { format!(" +{}", many.len() - 3) } else { String::new() };
            v.text_centered(&format!("{}{more}", names.join(", ")), cx, y + 24.0, 20.0, WHITE);
        }
    }
}

/// Label on the left, value right-aligned: a compact table.
fn stat_rows(v: &View, left: f32, right: f32, top: f32, rows: &[(&str, String)], row_h: f32, size: f32) {
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = top + i as f32 * row_h;
        v.text(label, left, y, size, hex(0x9aa3b8));
        v.text_right(value, right, y, size, WHITE);
    }
}

/// End-of-run numbers, shared by both result screens.
fn summary_rows(s: &Stats) -> Vec<(&'static str, String)> {
    vec![
        ("time", stats::clock(s.time)),
        ("lines", s.lines.to_string()),
        ("pieces", s.pieces.to_string()),
        ("PPS", stats::fmt_rate(s.pps(), 2)),
        ("APM", stats::fmt_rate(s.apm(), 1)),
        ("VS", stats::fmt_rate(s.vs(), 1)),
        ("KPP", stats::fmt_rate(s.kpp(), 2)),
        ("max combo", s.max_combo.to_string()),
        ("max back-to-back", s.max_back_to_back.to_string()),
        ("quads", s.quads.to_string()),
        ("T-spins", s.t_spins.to_string()),
        ("perfect clears", s.perfect_clears.to_string()),
    ]
}

fn draw_classic_results(v: &View, game: &Game, reward: Option<&Reward>) {
    let cx = BOARD_X + BOARD_W / 2.0;
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.8));
    v.text_centered("GAME OVER", cx, BOARD_Y + 90.0, 52.0, WHITE);
    v.text_centered("score", cx, BOARD_Y + 140.0, 20.0, hex(0x9aa3b8));
    v.text_centered(&game.score.to_string(), cx, BOARD_Y + 178.0, 38.0, hex(0xffd23f));
    let rows = summary_rows(&game.stats);
    stat_rows(v, BOARD_X + 30.0, BOARD_X + BOARD_W - 30.0, BOARD_Y + 214.0, &rows, 24.0, 19.0);
    if let Some(r) = reward {
        draw_reward(v, r, BOARD_Y + 494.0);
    }
    v.text_centered("Enter retry   ·   Esc menu", cx, BOARD_Y + 628.0, 20.0, hex(0x9aa3b8));
}

fn draw_pause(v: &View) {
    let cx = BOARD_X + BOARD_W / 2.0;
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.8));
    v.text_centered("PAUSED", cx, BOARD_Y + 120.0, 52.0, WHITE);
    let controls = [
        ("move", "Left / Right"),
        ("soft drop", "Down"),
        ("hard drop", "Space"),
        ("rotate cw", "Up / X"),
        ("rotate ccw", "Z / Ctrl"),
        ("hold", "C / Shift"),
        ("motion (field)", "M"),
        ("scene effects", "E"),
        ("sound", "N"),
        ("music", "J"),
    ];
    let rows: Vec<(&str, String)> = controls.iter().map(|&(a, k)| (a, k.to_owned())).collect();
    stat_rows(v, BOARD_X + 40.0, BOARD_X + BOARD_W - 40.0, BOARD_Y + 180.0, &rows, 30.0, 20.0);
    v.text_centered("Esc resume   ·   Q menu", cx, BOARD_Y + 180.0 + rows.len() as f32 * 30.0 + 30.0, 22.0, hex(0x9aa3b8));
}

// ───────────── Shadow 1v1 ─────────────

/// A shadow's portrait, decoded once and kept for the session.
fn portrait(key: &str) -> Option<Texture2D> {
    thread_local! {
        static CACHE: std::cell::RefCell<Vec<(String, Texture2D)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let bytes: &[u8] = match key {
        "jianjie" => include_bytes!("../assets/portraits/jianjie.png"),
        "luoyi_angel" => include_bytes!("../assets/portraits/luoyi_angel.png"),
        "zhuang_hao" => include_bytes!("../assets/portraits/zhuang_hao.png"),
        "luoyi" => include_bytes!("../assets/portraits/luoyi.png"),
        "deon" => include_bytes!("../assets/portraits/deon.png"),
        "luoyi_extreme" => include_bytes!("../assets/portraits/luoyi_extreme.png"),
        "deon_extreme" => include_bytes!("../assets/portraits/deon_extreme.png"),
        "deon_nightmare" => include_bytes!("../assets/portraits/deon_nightmare.png"),
        _ => return None,
    };
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((_, t)) = c.iter().find(|(k, _)| k == key) {
            return Some(t.clone());
        }
        let t = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
        t.set_filter(FilterMode::Linear);
        c.push((key.to_owned(), t.clone()));
        Some(t)
    })
}

/// A portrait in a frame of the shadow's colour (nothing if it has none).
fn draw_portrait(v: &View, p: &Profile, x: f32, y: f32, size: f32, tint: Color) {
    if let Some(t) = portrait(p.portrait) {
        v.rect(x - 2.0, y - 2.0, size + 4.0, size + 4.0, with_alpha(hex(p.color), 0.8 * tint.a));
        v.texture(&t, x, y, size, size, tint);
    }
}

pub(crate) const OPP_CELL: f32 = 22.0;
pub(crate) const OPP_X: f32 = PANEL_X + 14.0;
pub(crate) const OPP_Y: f32 = BOARD_Y + 78.0;
const OPP_NEXT_X: f32 = OPP_X + WIDTH as f32 * OPP_CELL + 14.0;

/// The shadow's field, live: name, board, hold / next, its numbers.
fn draw_versus_panel(v: &View, vs: &Versus, still: bool) {
    let g = &vs.shadow;
    let p = vs.profile;
    let accent = hex(p.color);
    let muted = hex(0x9aa3b8);
    let (bw, bh) = (WIDTH as f32 * OPP_CELL, VISIBLE_ROWS as f32 * OPP_CELL);

    v.text("SHADOW", PANEL_X, BOARD_Y - 8.0, 22.0, muted);
    let face = 62.0;
    draw_reacting_portrait(v, vs, OPP_X, BOARD_Y + 4.0, face, still);
    let tx = OPP_X + face + 14.0;
    v.text_mixed(p.name, tx, BOARD_Y + 34.0, if p.name.chars().count() > 8 { 30.0 } else { 40.0 }, accent);
    v.text(&format!("{}  ·  {:.0}-{:.0} PPM", p.rank, p.ppm.0, p.ppm.1), tx, BOARD_Y + 60.0, 18.0, hex(0x5d6680));

    // Field.
    let lost = vs.player_won();
    // The shadow's own danger shows too: its frame pulses red near the top.
    let their_danger = crate::danger::level(g);
    let beat = if still { 0.6 } else { 0.5 + 0.5 * (get_time() as f32 * (4.0 + 7.0 * their_danger)).sin() };
    let frame = if lost { hex(0x4a5064) } else { mix_colors(with_alpha(accent, 0.55), hex(0xff3b4f), their_danger * (0.45 + 0.55 * beat)) };
    v.rect(OPP_X - 3.0, OPP_Y - 3.0, bw + 6.0, bh + 6.0, frame);
    v.rect(OPP_X, OPP_Y, bw, bh, hex(theme().field));
    let cell = |x: i32, y: i32| (OPP_X + x as f32 * OPP_CELL, OPP_Y + (y - HIDDEN_ROWS as i32) as f32 * OPP_CELL);
    for y in HIDDEN_ROWS..TOTAL_ROWS {
        for x in 0..WIDTH {
            if let Some(c) = g.board.get(x, y) {
                let (px, py) = cell(x as i32, y as i32);
                v.block(px, py, OPP_CELL, if g.game_over { hex(0x4a5064) } else { cell_color(c) });
            }
        }
    }
    if !g.game_over && vs.fighting() {
        let a = g.active;
        let color = piece_color(a.kind);
        let ghost = g.ghost_y();
        for (cx, cy) in a.kind.cells(a.rot) {
            if ghost + cy >= HIDDEN_ROWS as i32 {
                let (px, py) = cell(a.x + cx, ghost + cy);
                v.ghost(px, py, OPP_CELL, color);
            }
        }
        for (x, y) in a.cells() {
            if y >= HIDDEN_ROWS as i32 {
                let (px, py) = cell(x, y);
                v.block(px, py, OPP_CELL, color);
            }
        }
    }
    // Garbage on its way to the shadow.
    let pending = g.pending_garbage();
    if pending > 0 {
        let h = pending.min(VISIBLE_ROWS as u32) as f32 * OPP_CELL;
        v.rect(OPP_X - 10.0, OPP_Y + bh - h, 5.0, h, hex(0xff4f6d));
    }

    // Hold and next, small.
    let mini = 10.0;
    v.text("HOLD", OPP_NEXT_X, OPP_Y + 12.0, 16.0, muted);
    if let Some(k) = g.hold {
        draw_piece_sized(v, k, OPP_NEXT_X + 26.0, OPP_Y + 38.0, mini, piece_color(k));
    }
    v.text("NEXT", OPP_NEXT_X, OPP_Y + 82.0, 16.0, muted);
    for (i, &k) in g.next.iter().take(4).enumerate() {
        draw_piece_sized(v, k, OPP_NEXT_X + 26.0, OPP_Y + 108.0 + i as f32 * 40.0, mini, piece_color(k));
    }

    // Its numbers.
    let s = &g.stats;
    let rows = [
        ("PPS", stats::fmt_rate(s.pps(), 2)),
        ("APM", stats::fmt_rate(s.apm(), 1)),
        ("SENT", vs.received.to_string()),
    ];
    for (i, (label, value)) in rows.iter().enumerate() {
        let x = OPP_X + i as f32 * 80.0;
        v.text(label, x, OPP_Y + bh + 28.0, 18.0, muted);
        v.text(value, x, OPP_Y + bh + 56.0, 26.0, WHITE);
    }
    if lost {
        v.rect(OPP_X, OPP_Y + bh / 2.0 - 40.0, bw, 70.0, Color::new(0.0, 0.0, 0.0, 0.7));
        v.text_centered("TOPPED OUT", OPP_X + bw / 2.0, OPP_Y + bh / 2.0 + 6.0, 30.0, hex(0x9aa3b8));
    }
}

/// A block of the topped-out stack: it flashes, greys, then breaks loose and falls away
/// (each block on its own delay and drift), like the field giving out under you.
fn draw_crumbling_cell(v: &View, px: f32, py: f32, x: usize, y: usize, cell: Cell, k: f32) {
    let t = k * crate::fx::DEATH_TIME;
    let grey = hex(0x4a5064);
    let base = cell_color(cell);
    // First a white flash, then the colour drains to grey.
    let c = if t < 0.12 { mix_colors(base, WHITE, 1.0 - t / 0.12) } else { mix_colors(base, grey, ((t - 0.12) / 0.3).min(1.0)) };
    let delay = 0.35 + hash01(x, y, 7) * 0.45 + (TOTAL_ROWS - y) as f32 * 0.008;
    let fall = (t - delay).max(0.0);
    let dy = 1500.0 * fall * fall;
    let dx = (hash01(x, y, 8) - 0.5) * 140.0 * fall;
    let alpha = (1.0 - fall * 1.6).clamp(0.0, 1.0);
    if alpha > 0.0 {
        v.block(px + dx, py + dy, CELL, Color { a: alpha, ..c });
    }
}

/// Seconds a portrait reaction lasts.
const OUCH_TIME: f32 = 0.7;
const TAUNT_TIME: f32 = 0.6;

/// What the shadow says when hit with `lines` of garbage, and when sending `lines`.
fn ouch_text(lines: u32) -> &'static str {
    match lines {
        0..=1 => "OUCH!",
        2..=3 => "OOF!!",
        4..=5 => "NOOO!",
        _ => "AAARGH!!",
    }
}

fn taunt_text(lines: u32) -> &'static str {
    match lines {
        0..=1 => "here!",
        2..=3 => "take that!",
        _ => "EAT THIS!",
    }
}

/// The shadow's face, reacting: it flinches (shake, squash, red flash) when your garbage
/// hits it, lunges with a glow when it sends you garbage, and greys out when it loses.
/// With motion off the words still show, but nothing moves.
fn draw_reacting_portrait(v: &View, vs: &Versus, x: f32, y: f32, size: f32, still: bool) {
    let p = &vs.profile;
    let Some(tex) = portrait(p.portrait) else {
        return;
    };
    let t = get_time() as f32;
    let hurt = (1.0 - vs.since_hit / OUCH_TIME).max(0.0);
    let taunt = (1.0 - vs.since_sent / TAUNT_TIME).max(0.0);
    let lost = vs.player_won();
    let motion = if still { 0.0 } else { 1.0 };

    // Flinch: a quick shake that dies out, squashed down; lunge: a bounce up in size.
    let shake = motion * hurt * hurt * (t * 70.0).sin() * 7.0;
    let squash = 1.0 - motion * 0.1 * hurt;
    let lunge = 1.0 + motion * 0.14 * taunt * taunt;
    let (w, h) = (size * lunge / squash.sqrt(), size * lunge * squash);
    let (cx, base) = (x + size / 2.0 + shake, y + size);
    let (px, py) = (cx - w / 2.0, base - h);

    // Frame: the shadow's colour, flaring white on a taunt, red on a hit.
    let mut frame = with_alpha(hex(p.color), 0.85);
    frame = mix_colors(frame, WHITE, 0.7 * taunt);
    frame = mix_colors(frame, hex(0xff3b4f), hurt);
    if taunt > 0.0 && !still {
        v.rect(px - 6.0 * taunt, py - 6.0 * taunt, w + 12.0 * taunt, h + 12.0 * taunt, with_alpha(hex(p.color), 0.35 * taunt));
    }
    v.rect(px - 2.0, py - 2.0, w + 4.0, h + 4.0, if lost { hex(0x4a5064) } else { frame });
    let tint = if lost { Color::new(0.4, 0.4, 0.45, 1.0) } else { mix_colors(WHITE, hex(0xff8080), 0.6 * hurt) };
    v.texture(&tex, px, py, w, h, tint);
    if hurt > 0.0 {
        v.rect(px, py, w, h, Color::new(1.0, 0.15, 0.2, 0.35 * hurt));
    }

    // The words pop beside the face and drift up.
    let say = |text: &str, k: f32, color: Color, size_px: f32| {
        let rise = motion * (1.0 - k) * 18.0;
        let pop = 1.0 + motion * 0.5 * (k - 0.75).max(0.0) * 4.0;
        // Over the top of the shadow's field (clear of its name), where the stack rarely reaches.
        let field_mid = OPP_X + WIDTH as f32 * OPP_CELL / 2.0;
        v.text_centered(text, field_mid, OPP_Y + 52.0 - rise, size_px * pop, with_alpha(color, k.min(1.0) * 1.4));
    };
    if hurt > 0.0 {
        let big = vs.last_hit >= 4;
        say(ouch_text(vs.last_hit), hurt, hex(0xff4f6d), if big { 30.0 } else { 24.0 });
    } else if taunt > 0.0 && !lost {
        say(taunt_text(vs.last_sent), taunt, hex(p.color), 20.0);
    }
}

/// "3, 2, 1" over both fields, each number popping in.
fn draw_countdown(v: &View, vs: &Versus, left: f32, still: bool) {
    let cx = BOARD_X + BOARD_W / 2.0;
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.45));
    let n = left.ceil().max(1.0) as u32;
    // Motion off: the number just changes, without the pop.
    let k = if still { 0.6 } else { left.fract() };
    let size = 90.0 + 50.0 * k * k;
    let face = 150.0;
    draw_portrait(v, &vs.profile, cx - face / 2.0, BOARD_Y + 70.0, face, WHITE);
    v.text_centered(&n.to_string(), cx, BOARD_Y + BOARD_H / 2.0 + 40.0 + size * 0.3, size, with_alpha(WHITE, 0.4 + 0.6 * k));
    v.text_mixed_centered(&format!("VS {}", vs.profile.name), cx, BOARD_Y + BOARD_H / 2.0 + 150.0, 28.0, hex(vs.profile.color));
}

/// A quick "GO!" as the match starts.
fn draw_go(v: &View, vs: &Versus) {
    if vs.time > 0.6 {
        return;
    }
    let k = vs.time / 0.6;
    let cx = BOARD_X + BOARD_W / 2.0;
    v.text_centered("GO!", cx, BOARD_Y + BOARD_H / 2.0 + 30.0, 90.0 + 40.0 * k, with_alpha(hex(0xffd23f), 1.0 - k));
}

fn draw_versus_results(v: &View, vs: &Versus, won: bool, mine: &Stats, reward: Option<&Reward>) {
    let cx = BOARD_X + BOARD_W / 2.0;
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.8));
    let (title, color) = if won { ("VICTORY", hex(0xffd23f)) } else { ("DEFEAT", hex(0xff4f6d)) };
    let face = 64.0;
    draw_portrait(v, &vs.profile, cx - face / 2.0, BOARD_Y + 14.0, face, if won { Color::new(0.5, 0.5, 0.55, 1.0) } else { WHITE });
    v.text_centered(title, cx, BOARD_Y + 130.0, 58.0, color);
    v.text_mixed_centered(&format!("vs {} ({})", vs.profile.name, vs.profile.rank.to_lowercase()), cx, BOARD_Y + 162.0, 22.0, hex(vs.profile.color));
    v.text_centered(&stats::clock(vs.time), cx, BOARD_Y + 192.0, 22.0, hex(0x9aa3b8));

    // You vs them, side by side.
    let theirs = &vs.shadow.stats;
    let (left, mid, right) = (BOARD_X + 26.0, BOARD_X + 200.0, BOARD_X + BOARD_W - 26.0);
    let top = BOARD_Y + 230.0;
    v.text_right("YOU", mid, top, 20.0, WHITE);
    v.text_mixed_right(vs.profile.name, right, top, 20.0, hex(vs.profile.color));
    let rows = [
        ("PPS", stats::fmt_rate(mine.pps(), 2), stats::fmt_rate(theirs.pps(), 2)),
        ("APM", stats::fmt_rate(mine.apm(), 1), stats::fmt_rate(theirs.apm(), 1)),
        ("VS", stats::fmt_rate(mine.vs(), 1), stats::fmt_rate(theirs.vs(), 1)),
        ("pieces", mine.pieces.to_string(), theirs.pieces.to_string()),
        ("lines", mine.lines.to_string(), theirs.lines.to_string()),
        ("garbage sent", vs.sent.to_string(), vs.received.to_string()),
        ("quads", mine.quads.to_string(), theirs.quads.to_string()),
        ("max combo", mine.max_combo.to_string(), theirs.max_combo.to_string()),
    ];
    for (i, (label, a, b)) in rows.iter().enumerate() {
        let y = top + 36.0 + i as f32 * 30.0;
        v.text(label, left, y, 20.0, hex(0x9aa3b8));
        v.text_right(a, mid, y, 20.0, WHITE);
        v.text_right(b, right, y, 20.0, WHITE);
    }
    if let Some(r) = reward {
        draw_reward(v, r, BOARD_Y + 494.0);
    }
    v.text_centered("Enter rematch   ·   Esc menu", cx, BOARD_Y + 628.0, 20.0, hex(0x9aa3b8));
}

/// The opponent select screen: one row per shadow (keyboard or mouse), details below.
pub fn draw_select(shadows: &[Profile], selected: usize, hover: Option<MenuHit>, scene_on: bool, progress: &Progress) {
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    let theme = shadows.get(selected).map_or_else(Theme::classic, Theme::shadow);
    scene::draw_menu_backdrop(&v, CLASSIC_W, theme, scene_on);
    let cx = CLASSIC_W / 2.0;
    v.text_centered("CHOOSE YOUR SHADOW", cx, 118.0, 52.0, WHITE);
    v.text_centered("a true 1v1 against a bot at its own pace", cx, 156.0, 22.0, hex(0x5d6680));
    for (i, p) in shadows.iter().enumerate() {
        let r = select_item_rect(i);
        let active = i == selected;
        let hovered = hover == Some(MenuHit::Item(i));
        let accent = hex(p.color);
        v.rect(r.x, r.y, r.w, r.h, if active { hex(0x1c2234) } else { hex(0x141824) });
        v.outline(r.x, r.y, r.w, r.h, 2.0, if active { accent } else if hovered { hex(0x9aa3b8) } else { hex(0x262c3d) });
        v.rect(r.x, r.y, 6.0, r.h, accent);
        let face = r.h - 16.0;
        draw_portrait(&v, p, r.x + 18.0, r.y + 8.0, face, if active { WHITE } else { Color::new(0.7, 0.7, 0.75, 1.0) });
        let name_x = r.x + 30.0 + face;
        v.text_mixed(p.name, name_x, r.y + 27.0, if p.name.chars().count() > 8 { 24.0 } else { 28.0 }, if active { WHITE } else { hex(0x9aa3b8) });
        // Your record against them, under the name.
        let rec = progress.record(p.portrait);
        let (line, color) = match (rec.wins + rec.losses, rec.best_time) {
            (0, _) => ("not fought yet".to_owned(), hex(0x5d6680)),
            (_, Some(t)) => (format!("{}W-{}L  ·  best {}", rec.wins, rec.losses, stats::clock(t)), hex(0xffd23f)),
            (_, None) => (format!("{}W-{}L", rec.wins, rec.losses), hex(0x9aa3b8)),
        };
        v.text(&line, name_x, r.y + 44.0, 16.0, color);
        v.text_right(p.rank, r.right() - 20.0, r.y + 21.0, 19.0, accent);
        v.text_right(&format!("{:.0}-{:.0} PPM", p.ppm.0, p.ppm.1), r.right() - 20.0, r.y + 40.0, 17.0, hex(0x9aa3b8));
    }
    if let Some(p) = shadows.get(selected) {
        let y = select_item_rect(shadows.len()).y + 14.0;
        v.text_centered(p.blurb, cx, y, 20.0, hex(0x9aa3b8));
        // Your bests against them, and what beating them unlocks.
        let rec = progress.record(p.portrait);
        let mut bits = Vec::new();
        if let Some(apm) = rec.best_apm {
            bits.push(format!("best APM {apm:.1}"));
        }
        if let Some(pps) = rec.best_pps {
            bits.push(format!("best PPS {pps:.2}"));
        }
        if let Some(item) = Item::all().find(|&i| i.unlock() == skins::Unlock::Beat(p.portrait)) {
            let got = if progress.unlocked(item) { "unlocked" } else { "beat them to unlock" };
            bits.push(format!("{} {}: {got}", item.name(), item.kind()));
        }
        v.text_centered(&bits.join("  ·  "), cx, y + 24.0, 18.0, hex(0x7fd8ff));
    }
    v.text_centered("Up/Down or mouse to choose  ·  Enter or click to fight  ·  Esc back", cx, 712.0, 18.0, hex(0x5d6680));
}

fn select_item_rect(i: usize) -> Rect {
    Rect::new(CLASSIC_W / 2.0 - 230.0, 182.0 + i as f32 * 56.0, 460.0, 50.0)
}

/// Which shadow (if any) is under `screen_pos` on the select screen.
pub fn select_hit(screen_pos: Vec2, items: usize) -> Option<MenuHit> {
    let p = View::fit_window(Vec2::ZERO, CLASSIC_W).to_virtual(screen_pos);
    (0..items).find(|&i| select_item_rect(i).contains(p)).map(MenuHit::Item)
}

pub struct MenuItem {
    pub title: &'static str,
    pub blurb: &'static str,
}

// Menu layout, shared by drawing and mouse hit-testing (virtual coordinates).
const MENU_ITEM_W: f32 = 420.0;
const MENU_ITEM_H: f32 = 56.0;
const MENU_ITEM_GAP: f32 = 14.0;
const MENU_TOP: f32 = 270.0;
const TOGGLE_W: f32 = 200.0;
const TOGGLE_H: f32 = 40.0;
const TOGGLE_GAP: f32 = 20.0;

/// Something clickable on the main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuHit {
    Item(usize),
    Motion,
    Scene,
    Sound,
}

fn menu_item_rect(i: usize) -> Rect {
    Rect::new(
        CLASSIC_W / 2.0 - MENU_ITEM_W / 2.0,
        MENU_TOP + i as f32 * (MENU_ITEM_H + MENU_ITEM_GAP),
        MENU_ITEM_W,
        MENU_ITEM_H,
    )
}

/// The blurb sits under the items; the toggles sit in a row under the blurb.
fn menu_toggle_rect(items: usize, which: MenuHit) -> Rect {
    let y = MENU_TOP + items as f32 * (MENU_ITEM_H + MENU_ITEM_GAP) + 50.0;
    let left = CLASSIC_W / 2.0 - 1.5 * TOGGLE_W - TOGGLE_GAP;
    let slot = match which {
        MenuHit::Motion => 0.0,
        MenuHit::Scene => 1.0,
        _ => 2.0,
    };
    Rect::new(left + slot * (TOGGLE_W + TOGGLE_GAP), y, TOGGLE_W, TOGGLE_H)
}

impl View {
    /// Screen pixels per virtual unit (scene effects draw meshes in screen space).
    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// Virtual coordinates -> screen pixels.
    pub(crate) fn to_screen(&self, x: f32, y: f32) -> Vec2 {
        vec2(self.ox + x * self.scale, self.oy + y * self.scale)
    }

    /// Screen pixels -> virtual coordinates.
    fn to_virtual(&self, p: Vec2) -> Vec2 {
        vec2((p.x - self.ox) / self.scale, (p.y - self.oy) / self.scale)
    }
}

/// What, if anything, is under `screen_pos` (e.g. `mouse_position()`) on a menu
/// with `items` entries.
pub fn menu_hit(screen_pos: Vec2, items: usize) -> Option<MenuHit> {
    let p = View::fit_window(Vec2::ZERO, CLASSIC_W).to_virtual(screen_pos);
    (0..items)
        .map(MenuHit::Item)
        .chain([MenuHit::Motion, MenuHit::Scene, MenuHit::Sound])
        .find(|&hit| {
            let r = match hit {
                MenuHit::Item(i) => menu_item_rect(i),
                toggle => menu_toggle_rect(items, toggle),
            };
            r.contains(p)
        })
}

pub fn draw_menu(items: &[MenuItem], selected: usize, hover: Option<MenuHit>, sound_on: bool, motion: Motion, scene_on: bool, progress: &Progress) {
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    scene::draw_menu_backdrop(&v, CLASSIC_W, Theme::classic(), scene_on);
    let cx = CLASSIC_W / 2.0;

    // A few static blocks as backdrop, one of each piece colour.
    for (i, kind) in PieceKind::ALL.iter().enumerate() {
        // Down both sides, clear of the menu column.
        let (x, y) = if i < 4 { (82.0, 290.0 + i as f32 * 88.0) } else { (678.0, 334.0 + (i - 4) as f32 * 88.0) };
        draw_mini_piece(&v, *kind, x, y, with_alpha(piece_color(*kind), 0.25));
    }

    v.text_centered("BLOCK", cx - 95.0, 170.0, 76.0, hex(0xffd23f));
    v.text_centered("BRAWL", cx + 105.0, 170.0, 76.0, hex(0xff4f6d));
    v.text_centered("a falling-block battler", cx, 210.0, 22.0, hex(0x5d6680));
    draw_level_bar(&v, progress, cx, 242.0);

    for (i, item) in items.iter().enumerate() {
        let r = menu_item_rect(i);
        let active = i == selected;
        v.rect(r.x, r.y, r.w, r.h, if active { hex(0x1c2234) } else { hex(0x141824) });
        v.outline(r.x, r.y, r.w, r.h, 2.0, if active { hex(0xffd23f) } else { hex(0x262c3d) });
        let color = if active { WHITE } else { hex(0x9aa3b8) };
        v.text_centered(item.title, cx, r.y + r.h / 2.0 + 12.0, 34.0, color);
    }
    if let Some(item) = items.get(selected) {
        let y = menu_item_rect(items.len()).y + 20.0;
        v.text_centered(item.blurb, cx, y, 20.0, hex(0x9aa3b8));
    }

    let toggles = [
        (MenuHit::Motion, format!("MOTION: {}", motion.label())),
        (MenuHit::Scene, format!("SCENE: {}", if scene_on { "on" } else { "off" })),
        (MenuHit::Sound, format!("SOUND: {}", if sound_on { "on" } else { "off" })),
    ];
    for (which, label) in toggles {
        let r = menu_toggle_rect(items.len(), which);
        let hovered = hover == Some(which);
        v.rect(r.x, r.y, r.w, r.h, if hovered { hex(0x1c2234) } else { hex(0x10131c) });
        v.outline(r.x, r.y, r.w, r.h, 2.0, if hovered { hex(0x9aa3b8) } else { hex(0x262c3d) });
        let color = if hovered { WHITE } else { hex(0x9aa3b8) };
        v.text_centered(&label, r.x + r.w / 2.0, r.y + r.h / 2.0 + 7.0, 20.0, color);
    }

    v.text_centered(
        "Up/Down or mouse to choose  ·  Enter or click to play  ·  M / E / N / J toggle",
        cx,
        712.0,
        18.0,
        hex(0x5d6680),
    );
    v.text_centered("made by xSolstice1  ·  github.com/xSolstice1", cx, 742.0, 16.0, hex(0x3d4458));
}

/// "LEVEL 4  ·  120 / 250 XP" over a slim bar, centred on `cx`.
fn draw_level_bar(v: &View, progress: &Progress, cx: f32, y: f32) {
    let (level, into, need) = progress::level_of(progress.xp);
    let label = if level >= progress::MAX_LEVEL { format!("LEVEL {level}") } else { format!("LEVEL {level}  ·  {into} / {need} XP") };
    v.text_centered(&label, cx, y, 18.0, hex(0x9aa3b8));
    bar(v, cx - 110.0, y + 7.0, 220.0, 6.0, into as f32 / need.max(1) as f32, hex(0xffd23f));
}

// ───────────── Skins ─────────────

/// The skins screen's columns: block styles, board themes, palettes.
pub const SKIN_COLUMNS: [&str; 3] = ["BLOCKS", "BOARD", "COLOURS"];

/// The items in column `col` of the skins screen.
pub fn skin_column(col: usize) -> Vec<Item> {
    match col {
        0 => skins::BlockStyle::ALL.into_iter().map(Item::Style).collect(),
        1 => skins::BoardTheme::ALL.into_iter().map(Item::Theme).collect(),
        _ => skins::Palette::ALL.into_iter().map(Item::Palette).collect(),
    }
}

/// Something clickable on the skins screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinsHit {
    Item { col: usize, row: usize },
    Back,
}

const SKIN_COL_X: f32 = 28.0;
const SKIN_COL_W: f32 = 128.0;
const SKIN_COL_GAP: f32 = 10.0;
const SKIN_TOP: f32 = 164.0;
const SKIN_ROW_H: f32 = 34.0;
const SKIN_ROW_GAP: f32 = 6.0;
const SKIN_PREVIEW_X: f32 = 462.0;
const PREVIEW_CELL: f32 = 18.0;
const PREVIEW_ROWS: usize = 14;

fn skin_item_rect(col: usize, row: usize) -> Rect {
    Rect::new(
        SKIN_COL_X + col as f32 * (SKIN_COL_W + SKIN_COL_GAP),
        SKIN_TOP + row as f32 * (SKIN_ROW_H + SKIN_ROW_GAP),
        SKIN_COL_W,
        SKIN_ROW_H,
    )
}

fn skin_back_rect() -> Rect {
    Rect::new(SKIN_COL_X, 664.0, 120.0, 34.0)
}

/// What's under `screen_pos` on the skins screen.
pub fn skins_hit(screen_pos: Vec2) -> Option<SkinsHit> {
    let p = View::fit_window(Vec2::ZERO, CLASSIC_W).to_virtual(screen_pos);
    if skin_back_rect().contains(p) {
        return Some(SkinsHit::Back);
    }
    (0..SKIN_COLUMNS.len())
        .flat_map(|col| (0..skin_column(col).len()).map(move |row| (col, row)))
        .find(|&(col, row)| skin_item_rect(col, row).contains(p))
        .map(|(col, row)| SkinsHit::Item { col, row })
}

/// A sample stack for the preview: rows from the bottom up, `.` empty, `G` garbage.
const PREVIEW_STACK: [&str; 6] = ["GGGG.GGGGG", "ILLSS.JZZ.", "IL.TTTJJZZ", "IL..T..OO.", "I......OO.", ".........."];

fn preview_cell(ch: char) -> Option<Cell> {
    let kind = match ch {
        'I' => PieceKind::I,
        'O' => PieceKind::O,
        'T' => PieceKind::T,
        'S' => PieceKind::S,
        'Z' => PieceKind::Z,
        'J' => PieceKind::J,
        'L' => PieceKind::L,
        'G' => return Some(Cell::Garbage),
        _ => return None,
    };
    Some(Cell::Piece(kind))
}

/// A small still field in the current skin: a stack, a falling piece and its ghost, and a queue.
fn draw_skin_preview(v: &View, x: f32, y: f32) {
    let t = theme();
    let bw = WIDTH as f32 * PREVIEW_CELL;
    draw_board_theme(v, skins::current().theme, x, y, PREVIEW_CELL, PREVIEW_ROWS, None);
    let at = |cx: i32, row_from_top: i32| (x + cx as f32 * PREVIEW_CELL, y + row_from_top as f32 * PREVIEW_CELL);
    for (i, row) in PREVIEW_STACK.iter().enumerate() {
        let r = PREVIEW_ROWS as i32 - 1 - i as i32;
        for (cx, ch) in row.chars().enumerate() {
            if let Some(cell) = preview_cell(ch) {
                let (px, py) = at(cx as i32, r);
                v.block(px, py, PREVIEW_CELL, cell_color(cell));
            }
        }
    }
    // A T falling into the slot, and where it will land.
    let tee = [(1, 0), (0, 1), (1, 1), (2, 1)];
    let color = piece_color(PieceKind::T);
    let ghost_row = PREVIEW_ROWS as i32 - 6;
    for (dx, dy) in tee {
        let (px, py) = at(2 + dx, ghost_row + dy);
        v.ghost(px, py, PREVIEW_CELL, color);
        let (px, py) = at(2 + dx, 2 + dy);
        v.block(px, py, PREVIEW_CELL, color);
    }
    // Hold and next, beside the field.
    let side = x + bw + 16.0;
    let (pw, ph) = (72.0, 54.0);
    let muted = hex(0x9aa3b8);
    v.text("HOLD", side, y + 12.0, 16.0, muted);
    v.rect(side, y + 18.0, pw, ph, hex(t.panel));
    v.outline(side, y + 18.0, pw, ph, 1.5, hex(t.panel_line));
    draw_piece_sized(v, PieceKind::L, side + pw / 2.0, y + 18.0 + ph / 2.0, 12.0, piece_color(PieceKind::L));
    let next_y = y + 96.0;
    v.text("NEXT", side, next_y, 16.0, muted);
    v.rect(side, next_y + 6.0, pw, 150.0, hex(t.panel));
    v.outline(side, next_y + 6.0, pw, 150.0, 1.5, hex(t.panel_line));
    for (i, kind) in [PieceKind::I, PieceKind::S, PieceKind::O].into_iter().enumerate() {
        draw_piece_sized(v, kind, side + pw / 2.0, next_y + 34.0 + i as f32 * 46.0, 12.0, piece_color(kind));
    }
}

/// The skins screen: three columns to choose from, a live preview of the focused item
/// on top of what's equipped, and what the focused item needs if it's locked.
pub fn draw_skins(progress: &Progress, col: usize, row: usize, hover: Option<SkinsHit>) {
    let equipped = progress.skin;
    let focused = skin_column(col).get(row).copied();
    let preview = focused.map_or(equipped, |i| i.apply(equipped));
    clear_background(hex(preview.theme.colors().backdrop));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    let cx = CLASSIC_W / 2.0;
    v.text_centered("SKINS", cx, 66.0, 52.0, WHITE);
    draw_level_bar(&v, progress, cx, 100.0);
    let total = Item::all().count();
    let owned = Item::all().filter(|&i| progress.unlocked(i)).count();
    v.text_centered(&format!("{owned} / {total} unlocked"), cx, 136.0, 16.0, hex(0x5d6680));

    for (c, title) in SKIN_COLUMNS.iter().enumerate() {
        let head = skin_item_rect(c, 0);
        v.text(title, head.x, head.y - 10.0, 20.0, if c == col { hex(0xffd23f) } else { hex(0x9aa3b8) });
        for (r, item) in skin_column(c).into_iter().enumerate() {
            let rect = skin_item_rect(c, r);
            let active = c == col && r == row;
            let hovered = hover == Some(SkinsHit::Item { col: c, row: r });
            let unlocked = progress.unlocked(item);
            let on = item.equipped_in(equipped);
            v.rect(rect.x, rect.y, rect.w, rect.h, if active { hex(0x1c2234) } else { hex(0x141824) });
            let line = if active { hex(0xffd23f) } else if hovered { hex(0x9aa3b8) } else if on { hex(0x5cdb5c) } else { hex(0x262c3d) };
            v.outline(rect.x, rect.y, rect.w, rect.h, 2.0, line);
            // A swatch of the item drawn in its own look.
            let sw = rect.h - 14.0;
            let (sx, sy) = (rect.x + 8.0, rect.y + 7.0);
            skins::with(item.apply(equipped), || match item {
                Item::Style(st) => v.block_styled(st, sx, sy, sw, piece_color(PieceKind::T)),
                Item::Theme(th) => {
                    let tc = th.colors();
                    v.rect(sx, sy, sw, sw, hex(tc.frame));
                    v.rect(sx + 2.0, sy + 2.0, sw - 4.0, sw - 4.0, hex(tc.field));
                    v.rect(sx + sw / 2.0, sy + 2.0, 1.0, sw - 4.0, hex(tc.grid));
                }
                Item::Palette(_) => {
                    for (k, kind) in [PieceKind::I, PieceKind::T, PieceKind::Z, PieceKind::O].into_iter().enumerate() {
                        let h = sw / 2.0;
                        v.rect(sx + (k % 2) as f32 * h, sy + (k / 2) as f32 * h, h, h, piece_color(kind));
                    }
                }
            });
            if !unlocked {
                v.rect(sx, sy, sw, sw, Color::new(0.04, 0.05, 0.08, 0.7));
                draw_lock(&v, sx + sw / 2.0, sy + sw / 2.0);
            }
            let text = if unlocked { if active { WHITE } else { hex(0x9aa3b8) } } else { hex(0x4a5064) };
            v.text(item.name(), sx + sw + 8.0, rect.y + rect.h / 2.0 + 6.0, 17.0, text);
            if on {
                v.rect(rect.right() - 9.0, rect.y + rect.h / 2.0 - 3.0, 5.0, 6.0, hex(0x5cdb5c));
            }
        }
    }

    // The preview.
    v.text("PREVIEW", SKIN_PREVIEW_X, SKIN_TOP - 10.0, 20.0, hex(0x9aa3b8));
    skins::with(preview, || draw_skin_preview(&v, SKIN_PREVIEW_X, SKIN_TOP));

    // About the focused item.
    if let Some(item) = focused {
        let top = 560.0;
        v.rect(SKIN_COL_X, top, CLASSIC_W - 2.0 * SKIN_COL_X, 84.0, hex(0x141824));
        v.outline(SKIN_COL_X, top, CLASSIC_W - 2.0 * SKIN_COL_X, 84.0, 2.0, hex(0x262c3d));
        v.text(item.name(), SKIN_COL_X + 18.0, top + 36.0, 30.0, WHITE);
        let name_w = measure_text(item.name(), None, (30.0 * v.scale).max(1.0) as u16, 1.0).width / v.scale;
        v.text(item.kind(), SKIN_COL_X + 30.0 + name_w, top + 36.0, 20.0, hex(0x5d6680));
        let (status, color) = if item.equipped_in(equipped) {
            ("EQUIPPED".to_owned(), hex(0x5cdb5c))
        } else if progress.unlocked(item) {
            ("Enter or click to equip".to_owned(), hex(0xffd23f))
        } else {
            (format!("LOCKED  ·  {} to unlock", progress::unlock_hint(item)), hex(0xff9a3c))
        };
        v.text_mixed(&status, SKIN_COL_X + 18.0, top + 66.0, 20.0, color);
    }

    let back = skin_back_rect();
    let hb = hover == Some(SkinsHit::Back);
    v.rect(back.x, back.y, back.w, back.h, if hb { hex(0x1c2234) } else { hex(0x10131c) });
    v.outline(back.x, back.y, back.w, back.h, 2.0, if hb { hex(0x9aa3b8) } else { hex(0x262c3d) });
    v.text_centered("BACK", back.x + back.w / 2.0, back.y + back.h / 2.0 + 7.0, 20.0, if hb { WHITE } else { hex(0x9aa3b8) });
    v.text_centered("Arrows or mouse to browse  ·  Enter or click to equip  ·  Esc back", cx, 724.0, 18.0, hex(0x5d6680));
}

/// A small padlock.
fn draw_lock(v: &View, cx: f32, cy: f32) {
    let c = hex(0x9aa3b8);
    v.circle_lines(cx, cy - 2.0, 4.0, 1.5, c);
    v.rect(cx - 5.5, cy - 1.0, 11.0, 8.0, c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skins_screen_targets_never_overlap_and_fit() {
        let mut rects = vec![skin_back_rect()];
        for col in 0..SKIN_COLUMNS.len() {
            for row in 0..skin_column(col).len() {
                rects.push(skin_item_rect(col, row));
            }
        }
        for (i, a) in rects.iter().enumerate() {
            assert!(a.x >= 0.0 && a.right() <= SKIN_PREVIEW_X - 10.0 && a.bottom() < 700.0, "{a:?}");
            // Lists stay clear of the info box.
            assert!(a.bottom() < 560.0 || *a == skin_back_rect(), "{a:?}");
            for b in &rects[i + 1..] {
                assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
            }
        }
        let preview_right = SKIN_PREVIEW_X + WIDTH as f32 * PREVIEW_CELL + 16.0 + 72.0;
        assert!(preview_right <= CLASSIC_W, "preview ends at {preview_right}");
        assert!(SKIN_TOP + PREVIEW_ROWS as f32 * PREVIEW_CELL < 560.0);
    }

    #[test]
    fn preview_rows_are_full_width() {
        for row in PREVIEW_STACK {
            assert_eq!(row.len(), WIDTH);
        }
    }

    #[test]
    fn menu_targets_never_overlap_and_fit_the_canvas() {
        for items in 1..=4 {
            let mut rects: Vec<Rect> = (0..items).map(menu_item_rect).collect();
            rects.push(menu_toggle_rect(items, MenuHit::Motion));
            rects.push(menu_toggle_rect(items, MenuHit::Scene));
            rects.push(menu_toggle_rect(items, MenuHit::Sound));
            for (i, a) in rects.iter().enumerate() {
                assert!(a.x >= 0.0 && a.right() <= CLASSIC_W && a.bottom() < 700.0, "{items} items: {a:?}");
                for b in &rects[i + 1..] {
                    assert!(!a.overlaps(b), "{items} items: {a:?} overlaps {b:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;

    #[test]
    fn text_splits_into_ascii_and_chinese_runs() {
        let r: Vec<_> = runs("ZHUANG HAO 哥哥!").collect();
        assert_eq!(r, vec![("ZHUANG HAO ", false), ("哥哥", true), ("!", false)]);
        assert_eq!(runs("").count(), 0);
    }

    /// Every non-ASCII character in a shadow's name must be in the embedded font subset.
    #[test]
    fn the_font_subset_covers_every_shadow_name() {
        for p in crate::bot::SHADOWS {
            for ch in p.name.chars().filter(|c| !c.is_ascii()) {
                assert!(CJK_CHARS.contains(ch), "{ch} in {} is missing from cjk-subset.ttf (see assets/fonts/README.md)", p.name);
            }
        }
    }

    #[test]
    fn every_shadow_row_fits_above_the_hints() {
        let n = crate::bot::SHADOWS.len();
        // The blurb and the records line under it.
        let blurb_y = select_item_rect(n).y + 14.0 + 24.0;
        assert!(blurb_y < 700.0, "blurb at {blurb_y}");
        for i in 0..n {
            assert!(!select_item_rect(i).overlaps(&select_item_rect(i + 1)));
        }
    }
}
