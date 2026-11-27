//! Drawing. Everything is laid out in a fixed virtual canvas and scaled to fit
//! the window (letterboxed), so resizing just works.

use macroquad::prelude::*;

use crate::board::{Cell, HIDDEN_ROWS, TOTAL_ROWS, VISIBLE_ROWS, WIDTH};
use crate::battle::{self, Ability, Battle, Defeat, Phase};
use crate::monster_art::{self, Pixel};
use crate::fx::Fx;
use crate::settings::{Bind, Settings};
use crate::settings_ui::{self, Extra, Kind};
use crate::game::Game;
use crate::piece::PieceKind;
use crate::stats::{self, Stats};
use crate::bot::Profile;
use crate::versus::{Phase as VsPhase, Versus};
use crate::progress::{self, Progress, Reward};
use crate::scene::{self, Scene, Theme};
use crate::skins::{self, BlockStyle, Item, ThemeColors};
use crate::ui;

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
        draw_glyphs(t, None, self.ox + x * s, self.oy + y * s, size * s, c);
    }

    pub(crate) fn text_right(&self, t: &str, right: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let w = text_px_width(t, None, size * s);
        draw_glyphs(t, None, self.ox + right * s - w, self.oy + y * s, size * s, c);
    }

    pub fn text_centered(&self, t: &str, cx: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let w = text_px_width(t, None, size * s);
        draw_glyphs(t, None, self.ox + cx * s - w / 2.0, self.oy + y * s, size * s, c);
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
    pub(crate) fn text_mixed(&self, t: &str, x: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let mut cx = self.ox + x * s;
        for (run, cjk) in runs(t) {
            let font = if cjk { cjk_font() } else { None };
            draw_glyphs(run, font.as_ref(), cx, self.oy + y * s, size * s, c);
            cx += text_px_width(run, font.as_ref(), size * s);
        }
    }

    /// Width of `text_mixed` in virtual units.
    pub(crate) fn measure_mixed(&self, t: &str, size: f32) -> f32 {
        let s = self.scale;
        runs(t).map(|(run, cjk)| text_px_width(run, if cjk { cjk_font() } else { None }.as_ref(), size * s)).sum::<f32>() / s
    }

    pub(crate) fn text_mixed_right(&self, t: &str, right: f32, y: f32, size: f32, c: Color) {
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

/// Glyph sizes text is rasterised at (screen px), about 19% apart. Every other size is
/// drawn from the next one up, scaled down: the font's glyph atlas stays small, instead of
/// rasterising (and re-uploading) a new set for every size an animation passes through.
const GLYPH_SIZES: [u16; 22] = [8, 10, 12, 14, 16, 19, 22, 26, 31, 37, 44, 52, 62, 74, 88, 104, 124, 148, 176, 208, 248, 296];

/// The glyph size to rasterise for `px`, and the scale from it to `px`.
pub(crate) fn glyph_size(px: f32) -> (u16, f32) {
    let px = px.max(1.0);
    let size = GLYPH_SIZES.iter().copied().find(|&g| g as f32 >= px).unwrap_or(GLYPH_SIZES[GLYPH_SIZES.len() - 1]);
    (size, px / size as f32)
}

/// Draws `t` at `px` screen pixels (baseline at `y`).
fn draw_glyphs(t: &str, font: Option<&Font>, x: f32, y: f32, px: f32, c: Color) {
    let (size, scale) = glyph_size(px);
    draw_text_ex(t, x, y, TextParams { font, font_size: size, font_scale: scale, color: c, ..Default::default() });
}

/// Width of `t` at `px` screen pixels.
pub(crate) fn text_px_width(t: &str, font: Option<&Font>, px: f32) -> f32 {
    let (size, scale) = glyph_size(px);
    measure_text(t, font, size, scale).width
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

/// A side panel in the board theme's colours, under a section header.
fn panel(v: &View, label: &str, x: f32, y: f32, w: f32, h: f32) {
    ui::section(v, x, y - 10.0, w, label, hex(ui::TEXT2));
    let t = theme();
    ui::panel(v, Rect::new(x, y, w, h), &ui::Panel::themed(t.panel, t.panel_line));
}

/// Height of a row in the score / lines / level card.
const STAT_ROW_H: f32 = 68.0;

/// A play session. `reward` is what the finished run earned (shown on the results).
pub fn draw(game: &Game, paused: bool, fx: &Fx, scene: &Scene, sound_on: bool, music_on: bool, opts: &Settings, battle: Option<&Battle>, versus: Option<&Versus>, reward: Option<&Reward>) {
    let theme = theme();
    clear_background(hex(theme.backdrop));
    let width = if battle.is_some() || versus.is_some() { BATTLE_W } else { CLASSIC_W };
    let v = View::fit_window(fx.shake_offset(), width);
    scene.draw_back(&v, width);
    // Scene effects off: nothing off the field moves on its own.
    let scene_still = !scene.on();
    let field_bg = hex(theme.field);
    // Motion off: nothing on screen moves on its own.
    let still = !fx.effects().clears;

    // Danger: the closer the stack is to the top, the stronger and faster the field pulses
    // red (motion off: a steady tint).
    let danger = if opts.danger { crate::danger::level(game) } else { 0.0 };
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
        let show_ghost = opts.ghost && fx.frozen_board().is_none();
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
    if opts.danger && danger >= 0.5 && !game.game_over {
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
    ui::text_shadow(&v, "BLOCK", LEFT_X, 40.0, 34.0, hex(ui::GOLD));
    ui::text_shadow(&v, "BRAWL", LEFT_X + 88.0, 40.0, 34.0, hex(ui::PINK));

    panel(&v, "HOLD", LEFT_X, BOARD_Y, PANEL_W, 100.0);
    if let Some(kind) = game.hold {
        let c = if game.hold_used { hex(0x4a5064) } else { piece_color(kind) };
        draw_mini_piece(&v, kind, LEFT_X + PANEL_W / 2.0, BOARD_Y + 50.0, c);
    }

    // Score, lines and level in one card; score and lines tick up (scene effects),
    // brighter while they're counting.
    let counting = if scene.ticking(game) { mix_colors(WHITE, hex(ui::GOLD), 0.45) } else { WHITE };
    let card = Rect::new(LEFT_X, BOARD_Y + 118.0, PANEL_W, 3.0 * STAT_ROW_H);
    let t = theme;
    ui::panel(&v, card, &ui::Panel::themed(t.panel, t.panel_line));
    for (i, label) in ["SCORE", "LINES", "LEVEL"].iter().enumerate() {
        let y = card.y + i as f32 * STAT_ROW_H;
        if i > 0 {
            ui::rule(&v, card.x + 10.0, y, card.w - 20.0, Color::new(1.0, 1.0, 1.0, 0.08));
        }
        ui::caps(&v, label, card.x + 12.0, y + 22.0, ui::CAPS, hex(ui::TEXT2), 2.0);
    }
    let values = [scene.score().to_string(), scene.lines().to_string(), game.level.to_string()];
    for (i, value) in values.iter().enumerate() {
        let y = card.y + i as f32 * STAT_ROW_H;
        let color = if i == 0 { counting } else { WHITE };
        ui::text_shadow_right(&v, value, card.right() - 12.0, y + 56.0, ui::NUM + 4.0, color);
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
    let table = Rect::new(LEFT_X, BOARD_Y + 352.0, PANEL_W, live.len() as f32 * 25.0 + 12.0);
    ui::section(&v, LEFT_X, table.y - 10.0, PANEL_W, "LIVE", hex(ui::TEXT2));
    ui::panel(&v, table, &ui::Panel::themed(t.panel, t.panel_line));
    for (i, (label, value)) in live.iter().enumerate() {
        let y = table.y + 6.0 + i as f32 * 25.0;
        if i % 2 == 1 {
            v.rect(table.x + 4.0, y, table.w - 8.0, 25.0, Color::new(1.0, 1.0, 1.0, 0.025));
        }
        ui::caps(&v, label, table.x + 12.0, ui::baseline(y + 12.5, 13.0), 13.0, hex(ui::TEXT2), 1.5);
        v.text_right(value, table.right() - 12.0, ui::baseline(y + 12.5, 19.0), 19.0, WHITE);
    }

    // Quick toggles, as keys.
    let legend = [
        ("M", "motion", fx.effects().label(), true),
        ("E", "scene", if scene.on() { "on" } else { "off" }, scene.on()),
        ("N", "sound", if sound_on { "on" } else { "off" }, sound_on),
        ("J", "music", if music_on { "on" } else { "off" }, music_on),
    ];
    for (i, (key, what, value, on)) in legend.iter().enumerate() {
        let cy = BOARD_Y + 554.0 + i as f32 * 24.0;
        let kw = ui::keycap(&v, LEFT_X, cy, key, 13.0);
        v.text(what, LEFT_X + kw + 7.0, ui::baseline(cy, 15.0), 15.0, hex(ui::TEXT3));
        v.text_right(value, LEFT_X + PANEL_W, ui::baseline(cy, 15.0), 15.0, if *on { hex(ui::TEXT2) } else { hex(ui::TEXT4) });
    }

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
            let band = Rect::new(BOARD_X + 16.0, BOARD_Y + 252.0, BOARD_W - 32.0, 116.0);
            ui::panel(&v, band, &ui::Panel::card().radius(14.0).tint(hex(ui::GOLD), 0.06));
            ui::radial(&v, band.center().x, BOARD_Y + 290.0, 140.0, 36.0, with_alpha(hex(ui::GOLD), 0.18), with_alpha(hex(ui::GOLD), 0.0));
            ui::text_shadow_centered(&v, "DEFEATED!", BOARD_X + BOARD_W / 2.0, BOARD_Y + 308.0, 48.0, hex(ui::GOLD));
            ui::caps_centered(&v, "NEXT UP", BOARD_X + BOARD_W / 2.0, BOARD_Y + 334.0, 12.0, hex(ui::TEXT3), 2.5);
            v.text_centered(&next.title(), BOARD_X + BOARD_W / 2.0, BOARD_Y + 356.0, 20.0, hex(next.color));
        }
    }

    if let Some(vs) = versus {
        match &vs.party {
            Some(party) => draw_party_panel(&v, vs, party, still),
            None => draw_versus_panel(&v, vs, scene_still),
        }
        scene.draw_shots(&v);
    }

    fx.draw_top(&v);

    // Overlays.
    if let Some(vs) = versus {
        match vs.phase {
            VsPhase::Over { .. } if fx.death_done() && vs.party.is_some() => draw_party_results(&v, vs, &game.stats, reward),
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
    let muted = hex(ui::TEXT2);
    let accent = hex(m.color);

    ui::section(v, x, BOARD_Y - 10.0, w, &format!("MONSTER {}", m.tier), muted);
    // The stage: tinted toward the monster's colour, deepest at the floor.
    let stage = Rect::new(x, BOARD_Y, w, 300.0);
    let feet_y = BOARD_Y + 238.0;
    ui::shadow(v, stage, 12.0, 14.0, 5.0, 0.45);
    ui::fill(v, stage, 12.0, mix_colors(hex(0x161b28), accent, 0.03), mix_colors(hex(0x141824), accent, 0.14));
    ui::radial(v, cx, feet_y + 8.0, 140.0, 30.0, with_alpha(accent, 0.12), with_alpha(accent, 0.0));
    ui::rule(v, x + 20.0, feet_y + 8.0, w - 40.0, with_alpha(accent, 0.3));
    ui::stroke(v, stage, 12.0, 1.0, with_alpha(accent, 0.22));
    ui::shine(v, stage, 12.0, 0.1);
    ui::text_shadow_centered(v, &m.title(), cx, BOARD_Y + 42.0, 34.0, accent);
    draw_monster(v, b, cx, feet_y, still);
    if let Some(ability) = m.ability {
        let active = b.ability_in_effect();
        let pulse = if still { 1.0 } else { 0.6 + 0.4 * (get_time() as f32 * 8.0).sin().abs() };
        let label = if active { format!("{}!", ability.name()) } else { ability.name().to_owned() };
        if active {
            ui::pill_centered(v, cx, BOARD_Y + 262.0, &label.to_uppercase(), 13.0, with_alpha(hex(ui::PINK), pulse), ui::Pill::Solid);
        } else {
            ui::pill_centered(v, cx, BOARD_Y + 262.0, &label.to_uppercase(), 13.0, hex(ui::CYAN), ui::Pill::Soft);
        }
        v.text_centered(ability.description(), cx, BOARD_Y + 290.0, 16.0, hex(ui::TEXT3));
    }

    // HP, the clock, the next attack and the tallies in one card.
    let card = Rect::new(x, BOARD_Y + 314.0, w, 266.0);
    ui::panel(v, card, &ui::Panel::card());
    let (l, r) = (card.x + 16.0, card.right() - 16.0);

    let hp_y = card.y + 18.0;
    ui::caps(v, "HP", l, ui::baseline(hp_y + 11.0, ui::CAPS), ui::CAPS, muted, 2.0);
    let hp = Rect::new(l + 40.0, hp_y, r - l - 40.0, 22.0);
    ui::progress(v, hp, b.hp as f32 / m.max_hp as f32, &ui::Bar::new(hex(0x2fae55), hex(0x8dff8f)).glow(0.4));
    ui::text_shadow_centered(v, &format!("{} / {}", b.hp, m.max_hp), hp.center().x, ui::baseline(hp.center().y, 18.0), 18.0, WHITE);
    if b.shield > 0 {
        let frac = b.shield as f32 / m.max_hp as f32;
        ui::progress(v, Rect::new(hp.x, hp.bottom() + 6.0, hp.w, 8.0), frac, &ui::Bar::new(hex(0x2f5fd0), hex(0x8fb8ff)).glow(0.3));
        ui::caps_right(v, &format!("SHIELD {}", b.shield), r, hp.bottom() + 30.0, 12.0, hex(ui::BLUE), 1.5);
    }

    // Time left.
    let time_y = card.y + 92.0;
    let secs = b.time_left.ceil() as u32;
    let urgent = b.time_left < 10.0 && b.phase == Phase::Fighting;
    let pulse = if urgent && !still { 0.6 + 0.4 * (get_time() as f32 * 10.0).sin().abs() } else { 1.0 };
    let time_color = if urgent { with_alpha(hex(ui::PINK), pulse) } else { WHITE };
    ui::caps(v, "TIME", l, time_y, ui::CAPS, muted, 2.0);
    ui::text_shadow_right(v, &format!("{}:{:02}", secs / 60, secs % 60), r, time_y + 6.0, 44.0, time_color);

    // Next garbage attack.
    let atk_y = card.y + 118.0;
    ui::caps(v, "ATTACK", l, ui::baseline(atk_y + 8.0, ui::CAPS), ui::CAPS, muted, 2.0);
    let charge = 1.0 - b.attack_timer / m.attack_every;
    let fighting = b.phase == Phase::Fighting;
    let atk = Rect::new(l + 80.0, atk_y, r - l - 80.0, 16.0);
    let warn = fighting && b.attack_timer < ATTACK_WARN;
    ui::progress(v, atk, if fighting { charge } else { 0.0 }, &ui::Bar::new(hex(0xc2541f), hex(0xffb347)).glow(if warn { 1.0 } else { 0.5 }));
    v.text_right(
        &format!("{} row{} every {:.1}s", m.attack_lines, if m.attack_lines == 1 { "" } else { "s" }, m.attack_every),
        r,
        atk_y + 40.0,
        16.0,
        hex(ui::TEXT3),
    );

    ui::rule(v, card.x + 12.0, card.y + 176.0, card.w - 24.0, Color::new(1.0, 1.0, 1.0, 0.1));
    let stats_y = card.y + 204.0;
    let mid = card.x + card.w / 2.0;
    v.rect(mid, card.y + 188.0, 1.0, 64.0, Color::new(1.0, 1.0, 1.0, 0.07));
    ui::caps(v, "DEFEATED", l, stats_y, ui::CAPS, muted, 2.0);
    ui::text_shadow(v, &b.defeated.to_string(), l, stats_y + 40.0, 36.0, WHITE);
    ui::caps(v, "DAMAGE", mid + 16.0, stats_y, ui::CAPS, muted, 2.0);
    ui::text_shadow(v, &b.total_damage.to_string(), mid + 16.0, stats_y + 40.0, 36.0, WHITE);
}

fn draw_battle_results(v: &View, b: &Battle, stats: &Stats, reward: Option<&Reward>) {
    let card = results_card(v, hex(ui::PINK));
    let cx = card.center().x;
    let title = match b.phase {
        Phase::Over(Defeat::Timeout) => "TIME UP",
        _ => "TOPPED OUT",
    };
    ui::text_shadow_centered(v, title, cx, BOARD_Y + 84.0, 48.0, hex(ui::PINK));
    ui::caps_centered(v, "BATTLER", cx, BOARD_Y + 108.0, 13.0, hex(ui::TEXT3), 3.0);

    // The two headline numbers, side by side.
    let (l, r) = (card.x + 16.0, card.right() - 16.0);
    let tile_w = (r - l - 10.0) / 2.0;
    for (i, (label, value)) in [("DEFEATED", b.defeated.to_string()), ("DAMAGE", b.total_damage.to_string())].iter().enumerate() {
        let t = Rect::new(l + i as f32 * (tile_w + 10.0), BOARD_Y + 124.0, tile_w, 72.0);
        ui::panel(v, t, &ui::Panel::well());
        ui::caps_centered(v, label, t.center().x, t.y + 22.0, 13.0, hex(ui::TEXT2), 2.0);
        ui::text_shadow_centered(v, value, t.center().x, t.y + 60.0, 36.0, if i == 0 { hex(ui::GOLD) } else { WHITE });
    }
    ui::caps(v, "FELL TO", l, BOARD_Y + 224.0, 13.0, hex(ui::TEXT2), 2.0);
    let fell = format!("{} (lv {})", b.monster.title(), b.monster.tier);
    v.text_right(&fell, r, BOARD_Y + 224.0, ui::fit(v, &fell, 19.0, r - l - 80.0), hex(b.monster.color));

    let rows = summary_rows(stats);
    stat_table(v, l, r, BOARD_Y + 244.0, &rows[2..8], 26.0);
    if let Some(rw) = reward {
        draw_reward(v, rw, BOARD_Y + 440.0);
    }
    results_hints(v, card, "retry");
}

/// XP earned, the level bar, records and new unlocks, in a block starting at `top`
/// (up to 142 tall) across the results card.
fn draw_reward(v: &View, r: &Reward, top: f32) {
    let gold = hex(ui::GOLD);
    let (left, w) = (BOARD_X + 26.0, BOARD_W - 52.0);
    let right = left + w;
    ui::rule(v, left - 6.0, top, w + 12.0, Color::new(1.0, 1.0, 1.0, 0.14));

    // "+120 XP", glowing, and the level it reached.
    let xp = format!("+{} XP", r.xp);
    ui::radial(v, left + ui::text_w(v, &xp, 28.0) / 2.0, top + 26.0, 80.0, 20.0, with_alpha(gold, 0.14), with_alpha(gold, 0.0));
    ui::text_shadow(v, &xp, left, top + 36.0, 28.0, gold);
    ui::level_badge(v, right - 15.0, top + 25.0, 15.0, r.level, gold);
    if r.leveled_up() {
        ui::pill_right(v, right - 38.0, top + 25.0, "LEVEL UP!", 12.0, gold, ui::Pill::Solid);
    } else {
        ui::caps_right(v, "LEVEL", right - 38.0, ui::baseline(top + 25.0, 13.0), 13.0, hex(ui::TEXT2), 2.0);
    }
    let bar_r = Rect::new(left, top + 50.0, w, 12.0);
    ui::progress(v, bar_r, r.into as f32 / r.need.max(1) as f32, &ui::Bar::xp().ticks(10).glow(0.9));
    ui::caps_right(v, &format!("{} / {} XP", r.into, r.need), right, ui::baseline(top + 78.0, 12.0), 12.0, hex(ui::TEXT3), 1.0);

    // New records as chips on the line under the bar.
    let mut x = left;
    for t in &r.records {
        x += ui::pill(v, x, top + 78.0, t, 11.0, hex(ui::CYAN), ui::Pill::Soft) + 6.0;
    }
    let y = top + 94.0;
    if r.unlocks.is_empty() {
        return;
    }
    // New unlocks: a card with each item's swatch.
    let card = Rect::new(left - 6.0, y, w + 12.0, 48.0);
    let orange = hex(ui::ORANGE);
    ui::glow(v, card, 9.0, 10.0, with_alpha(orange, 0.12));
    ui::panel(v, card, &ui::Panel::card().radius(9.0).tint(orange, 0.12));
    ui::stroke(v, card, 9.0, 1.0, with_alpha(orange, 0.45));
    let equipped = skins::current();
    let shown = r.unlocks.len().min(3);
    let sw = 30.0;
    for (i, &item) in r.unlocks.iter().take(shown).enumerate() {
        let sx = card.x + 9.0 + i as f32 * (sw + 6.0);
        draw_item_swatch(v, item, equipped, sx, card.y + 9.0, sw);
        ui::stroke(v, Rect::new(sx - 1.0, card.y + 8.0, sw + 2.0, sw + 2.0), 4.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.2));
    }
    let tx = card.x + 9.0 + shown as f32 * (sw + 6.0) + 4.0;
    let tw = card.right() - 10.0 - tx;
    let (chip, line) = match r.unlocks.as_slice() {
        [one] => ("NEW UNLOCK".to_owned(), format!("{} {}", one.name(), one.kind())),
        many => {
            let names: Vec<&str> = many.iter().take(3).map(|i| i.name()).collect();
            let more = if many.len() > 3 { format!(" +{}", many.len() - 3) } else { String::new() };
            (format!("{} NEW UNLOCKS", many.len()), format!("{}{more}", names.join(", ")))
        }
    };
    ui::pill(v, tx, card.y + 14.0, &chip, 10.0, orange, ui::Pill::Solid);
    v.text(&line, tx, card.y + 40.0, ui::fit(v, &line, 18.0, tw), WHITE);
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
    let card = results_card(v, hex(ui::GOLD));
    let cx = card.center().x;
    ui::text_shadow_centered(v, "GAME OVER", cx, BOARD_Y + 84.0, 48.0, WHITE);
    ui::caps_centered(v, "SCORE", cx, BOARD_Y + 114.0, 13.0, hex(ui::TEXT2), 3.0);
    let score = game.score.to_string();
    ui::radial(v, cx, BOARD_Y + 142.0, 110.0, 26.0, with_alpha(hex(ui::GOLD), 0.16), with_alpha(hex(ui::GOLD), 0.0));
    ui::text_shadow_centered(v, &score, cx, BOARD_Y + 160.0, 46.0, hex(ui::GOLD));
    let rows = summary_rows(&game.stats);
    stat_table(v, card.x + 18.0, card.right() - 18.0, BOARD_Y + 180.0, &rows, 21.0);
    if let Some(r) = reward {
        draw_reward(v, r, BOARD_Y + 448.0);
    }
    results_hints(v, card, "retry");
}

fn draw_pause(v: &View) {
    let cx = BOARD_X + BOARD_W / 2.0;
    ui::fill(v, Rect::new(BOARD_X, BOARD_Y, BOARD_W, BOARD_H), 0.0, Color::new(0.02, 0.03, 0.05, 0.7), Color::new(0.02, 0.03, 0.05, 0.85));
    let card = Rect::new(BOARD_X + 14.0, BOARD_Y + 74.0, BOARD_W - 28.0, 470.0);
    ui::panel(v, card, &ui::Panel::card().radius(14.0));
    ui::text_shadow_centered(v, "PAUSED", cx, card.y + 62.0, 48.0, WHITE);
    let controls = [
        ("move", "Left/Right"),
        ("soft drop", "Down"),
        ("hard drop", "Space"),
        ("rotate cw", "Up/X"),
        ("rotate ccw", "Z/Ctrl"),
        ("hold", "C/Shift"),
        ("motion (field)", "M"),
        ("scene effects", "E"),
        ("sound", "N"),
        ("music", "J"),
    ];
    let (l, r) = (card.x + 20.0, card.right() - 20.0);
    ui::section(v, l, card.y + 100.0, r - l, "CONTROLS", hex(ui::TEXT2));
    for (i, (action, keys)) in controls.iter().enumerate() {
        let cy = card.y + 124.0 + i as f32 * 29.0;
        v.text(action, l, ui::baseline(cy, 18.0), 18.0, hex(ui::TEXT2));
        // Keycaps, right-aligned.
        let mut x = r;
        for k in keys.rsplit('/') {
            x -= ui::keycap_w(v, k, 14.0);
            ui::keycap(v, x, cy, k, 14.0);
            x -= 5.0;
        }
    }
    ui::hints(v, cx, card.bottom() - 28.0, &[("Esc", "resume"), ("O", "settings"), ("Q", "menu")], 15.0);
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

/// Decodes one not-yet-loaded portrait (call once a frame on the menu), so opening the
/// shadow list never decodes all of them in the same frame.
pub fn warm_portraits() {
    thread_local! {
        static NEXT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    NEXT.with(|n| {
        let i = n.get();
        if let Some(p) = crate::bot::SHADOWS.get(i) {
            let _ = portrait(p.portrait);
            n.set(i + 1);
        }
    });
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
    let muted = hex(ui::TEXT2);
    let (bw, bh) = (WIDTH as f32 * OPP_CELL, VISIBLE_ROWS as f32 * OPP_CELL);

    ui::section(v, PANEL_X, BOARD_Y - 10.0, MONSTER_PANEL_W, if vs.online() { "OPPONENT" } else { "SHADOW" }, muted);
    let face = 62.0;
    draw_reacting_portrait(v, vs, OPP_X, BOARD_Y + 4.0, face, still);
    let tx = if vs.online() { OPP_X } else { OPP_X + face + 14.0 };
    let name = vs.name();
    text_mixed_shadow(v, name, tx, BOARD_Y + 32.0, if name.chars().count() > 8 { 30.0 } else { 40.0 }, accent);
    let w = ui::pill(v, tx, BOARD_Y + 52.0, p.rank, 11.0, accent, ui::Pill::Soft);
    let pace = if vs.online() { "LIVE".to_owned() } else { format!("{:.0}-{:.0} PPM", p.ppm.0, p.ppm.1) };
    v.text(&pace, tx + w + 8.0, ui::baseline(BOARD_Y + 52.0, 15.0), 15.0, hex(ui::TEXT3));

    // Field.
    let lost = vs.player_won();
    // The shadow's own danger shows too: its frame pulses red near the top.
    let their_danger = crate::danger::level(g);
    let beat = if still { 0.6 } else { 0.5 + 0.5 * (get_time() as f32 * (4.0 + 7.0 * their_danger)).sin() };
    let frame = if lost { hex(0x4a5064) } else { mix_colors(with_alpha(accent, 0.55), hex(0xff3b4f), their_danger * (0.45 + 0.55 * beat)) };
    ui::shadow(v, Rect::new(OPP_X - 3.0, OPP_Y - 3.0, bw + 6.0, bh + 6.0), 4.0, 12.0, 4.0, 0.45);
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
    ui::caps(v, "HOLD", OPP_NEXT_X, OPP_Y + 12.0, 12.0, muted, 1.5);
    if let Some(k) = g.hold {
        draw_piece_sized(v, k, OPP_NEXT_X + 26.0, OPP_Y + 38.0, mini, piece_color(k));
    }
    ui::caps(v, "NEXT", OPP_NEXT_X, OPP_Y + 82.0, 12.0, muted, 1.5);
    for (i, &k) in g.next.iter().take(4).enumerate() {
        draw_piece_sized(v, k, OPP_NEXT_X + 26.0, OPP_Y + 108.0 + i as f32 * 40.0, mini, piece_color(k));
    }

    // Its numbers, as tiles under its field.
    let s = &g.stats;
    let rows = [
        ("PPS", stats::fmt_rate(s.pps(), 2)),
        ("APM", stats::fmt_rate(s.apm(), 1)),
        ("SENT", vs.received.to_string()),
    ];
    let tile_w = (bw + 6.0 - 16.0) / 3.0;
    for (i, (label, value)) in rows.iter().enumerate() {
        let t = Rect::new(OPP_X - 3.0 + i as f32 * (tile_w + 8.0), OPP_Y + bh + 12.0, tile_w, 52.0);
        ui::panel(v, t, &ui::Panel::card().radius(8.0));
        ui::caps(v, label, t.x + 10.0, t.y + 18.0, 12.0, muted, 1.5);
        ui::text_shadow_right(v, value, t.right() - 10.0, t.y + 44.0, 24.0, WHITE);
    }
    if lost {
        let band = Rect::new(OPP_X + 10.0, OPP_Y + bh / 2.0 - 34.0, bw - 20.0, 60.0);
        ui::panel(v, band, &ui::Panel::card().radius(10.0));
        ui::caps_centered(v, "TOPPED OUT", OPP_X + bw / 2.0, ui::baseline(band.center().y, 24.0), 24.0, hex(ui::TEXT2), 3.0);
    }
}

/// Columns, rows and cell size for `n` mini fields in the party panel.
pub(crate) fn party_grid(n: usize) -> (usize, usize, f32) {
    let cols = match n {
        0 | 1 => 1,
        2..=4 => 2,
        _ => 3,
    };
    let rows = n.div_ceil(cols).max(1);
    let cell = ((PARTY_W / cols as f32 - 18.0) / WIDTH as f32).min((PARTY_H / rows as f32 - 34.0) / VISIBLE_ROWS as f32).min(OPP_CELL);
    (cols, rows, cell.floor())
}

const PARTY_W: f32 = BATTLE_W - PANEL_X - 10.0;
const PARTY_H: f32 = VIRTUAL_H - BOARD_Y - 20.0;

/// Everyone else in a party: a grid of mini fields with names, incoming garbage, who we
/// just hit (gold) and who just hit us (pink), and a place once they're out.
fn draw_party_panel(v: &View, vs: &Versus, party: &crate::party::Party, still: bool) {
    let n = party.members.len();
    let (cols, rows, cell) = party_grid(n);
    let (bw, bh) = (WIDTH as f32 * cell, VISIBLE_ROWS as f32 * cell);
    let (slot_w, slot_h) = (PARTY_W / cols as f32, PARTY_H / rows as f32);
    let label = format!("PARTY  ·  {} OF {} LEFT", party.alive(), party.players());
    ui::section(v, PANEL_X, BOARD_Y - 10.0, PARTY_W, &label, hex(ui::TEXT2));
    let gold = hex(ui::GOLD);
    let pink = hex(ui::PINK);
    for (i, m) in party.members.iter().enumerate() {
        let (c, r) = (i % cols, i / cols);
        let x = PANEL_X + c as f32 * slot_w + (slot_w - bw) / 2.0;
        let y = BOARD_Y + 14.0 + r as f32 * slot_h + 20.0;
        let g = &m.field;
        let out = !m.alive();
        // Name over the field.
        let size = if cell >= 14.0 { 16.0 } else { 13.0 };
        let name_w = v.measure_mixed(&m.name, size);
        let fit = if name_w > bw + 10.0 { size * (bw + 10.0) / name_w } else { size };
        text_mixed_shadow(v, &m.name, x + bw / 2.0 - name_w * fit / size / 2.0, y - 7.0, fit, if out { hex(ui::TEXT3) } else { WHITE });
        // The frame: gold as our attack lands, pink as theirs reaches us, red in danger.
        let danger = if out { 0.0 } else { crate::danger::level(g) };
        let hit = (1.0 - m.since_hit / crate::party::HIT_SHOW).max(0.0);
        let attacked = (1.0 - m.since_attacked / crate::party::HIT_SHOW).max(0.0);
        let beat = if still { 0.6 } else { 0.5 + 0.5 * (get_time() as f32 * (4.0 + 7.0 * danger)).sin() };
        let mut frame = if out { hex(0x3a3f50) } else { mix_colors(Color::new(1.0, 1.0, 1.0, 0.16), hex(0xff3b4f), danger * (0.45 + 0.55 * beat)) };
        frame = mix_colors(frame, gold, hit);
        frame = mix_colors(frame, pink, attacked * (1.0 - hit));
        let t = 2.0 + 2.0 * hit.max(attacked);
        ui::shadow(v, Rect::new(x - t, y - t, bw + 2.0 * t, bh + 2.0 * t), 3.0, 8.0, 3.0, 0.4);
        v.rect(x - t, y - t, bw + 2.0 * t, bh + 2.0 * t, frame);
        v.rect(x, y, bw, bh, hex(theme().field));
        let at = |cx: i32, cy: i32| (x + cx as f32 * cell, y + (cy - HIDDEN_ROWS as i32) as f32 * cell);
        let grey = hex(0x4a5064);
        for yy in HIDDEN_ROWS..TOTAL_ROWS {
            for xx in 0..WIDTH {
                if let Some(cl) = g.board.get(xx, yy) {
                    let (px, py) = at(xx as i32, yy as i32);
                    let col = if out { grey } else { cell_color(cl) };
                    if cell >= 12.0 {
                        v.block(px, py, cell, col);
                    } else {
                        v.rect(px, py, cell - 1.0, cell - 1.0, col);
                    }
                }
            }
        }
        // (Still drawn once we're out: we're watching.)
        if !out && !matches!(vs.phase, VsPhase::Countdown { .. }) {
            let a = g.active;
            for (ax, ay) in a.cells() {
                if ay >= HIDDEN_ROWS as i32 {
                    let (px, py) = at(ax, ay);
                    v.rect(px, py, cell - 1.0, cell - 1.0, piece_color(a.kind));
                }
            }
        }
        let pending = g.pending_garbage();
        if pending > 0 && !out {
            let h = pending.min(VISIBLE_ROWS as u32) as f32 * cell;
            v.rect(x - t - 5.0, y + bh - h, 3.0, h, pink);
        }
        if let Some(place) = m.place {
            let band = Rect::new(x + 4.0, y + bh / 2.0 - 22.0, bw - 8.0, 44.0);
            ui::panel(v, band, &ui::Panel::card().radius(8.0));
            let (txt, c) = if place == 1 { ("WIN".to_owned(), gold) } else { (format!("#{place}"), hex(ui::TEXT2)) };
            ui::text_shadow_centered(v, &txt, band.center().x, ui::baseline(band.center().y, 26.0), 26.0, c);
        }
    }
}

fn place_suffix(n: u32) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "TH",
        (1, _) => "ST",
        (2, _) => "ND",
        (3, _) => "RD",
        _ => "TH",
    }
}

/// Where we finished, and the standings so far (players still in at the top).
fn draw_party_results(v: &View, vs: &Versus, mine: &Stats, reward: Option<&Reward>) {
    let Some(party) = &vs.party else {
        return;
    };
    let place = party.my_place.unwrap_or(party.alive());
    let won = place == 1;
    let color = if won { hex(ui::GOLD) } else { hex(ui::CYAN) };
    let card = results_card(v, color);
    let cx = card.center().x;
    let title = if won { "WINNER!".to_owned() } else { format!("{place}{}", place_suffix(place)) };
    ui::radial(v, cx, BOARD_Y + 74.0, 150.0, 40.0, with_alpha(color, 0.18), with_alpha(color, 0.0));
    ui::text_shadow_centered(v, &title, cx, BOARD_Y + 92.0, 56.0, color);
    ui::caps_centered(v, &format!("OF {} PLAYERS", party.players()), cx, BOARD_Y + 120.0, 13.0, hex(ui::TEXT2), 2.5);
    let line = format!("{} PPS   ·   {} APM   ·   {} SENT", stats::fmt_rate(mine.pps(), 2), stats::fmt_rate(mine.apm(), 1), vs.sent);
    v.text_centered(&line, cx, BOARD_Y + 150.0, 16.0, hex(ui::TEXT2));

    let (left, right) = (card.x + 22.0, card.right() - 22.0);
    let top = BOARD_Y + 172.0;
    let standings = party.standings("YOU");
    let row_h = (250.0 / standings.len() as f32).min(28.0);
    for (i, (name, place, me)) in standings.iter().enumerate() {
        let y = top + i as f32 * row_h;
        let cy = y + row_h / 2.0;
        if *me {
            ui::fill(v, Rect::new(left - 8.0, y + 1.0, right - left + 16.0, row_h - 2.0), 5.0, with_alpha(hex(ui::CYAN), 0.12), with_alpha(hex(ui::CYAN), 0.08));
        } else if i % 2 == 0 {
            v.rect(left - 8.0, y + 1.0, right - left + 16.0, row_h - 2.0, Color::new(1.0, 1.0, 1.0, 0.03));
        }
        let size = (row_h * 0.62).min(17.0);
        let (p, pc) = match place {
            Some(1) => ("1ST".to_owned(), hex(ui::GOLD)),
            Some(p) => (format!("{p}{}", place_suffix(*p)), hex(ui::TEXT2)),
            None => ("IN".to_owned(), hex(ui::GREEN)),
        };
        v.text(&p, left, ui::baseline(cy, size), size, pc);
        v.text_mixed(name, left + 56.0, ui::baseline(cy, size), size, if *me { hex(ui::CYAN) } else { WHITE });
    }
    if let Some(r) = reward {
        draw_reward(v, r, BOARD_Y + 432.0);
    }
    let again = if !party.finished() { "watching..." } else if crate::net::is_host() { "next match" } else { "wait for host" };
    ui::hints(v, cx, card.bottom() - 18.0, &[("Enter", again), ("Esc", "leave")], 15.0);
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
    ui::fill(v, Rect::new(BOARD_X, BOARD_Y, BOARD_W, BOARD_H), 0.0, Color::new(0.02, 0.03, 0.05, 0.4), Color::new(0.02, 0.03, 0.05, 0.65));
    let n = left.ceil().max(1.0) as u32;
    // Motion off: the number just changes, without the pop.
    let k = if still { 0.6 } else { left.fract() };
    let size = 90.0 + 50.0 * k * k;
    let face = 150.0;
    let accent = hex(vs.profile.color);
    let fy = BOARD_Y + 70.0;
    let frame = Rect::new(cx - face / 2.0 - 3.0, fy - 3.0, face + 6.0, face + 6.0);
    // Online opponents have no portrait: just their name below.
    if !vs.online() {
        ui::glow(v, frame, 6.0, 18.0, with_alpha(accent, 0.3));
        ui::shadow(v, frame, 6.0, 14.0, 6.0, 0.5);
        draw_portrait(v, &vs.profile, cx - face / 2.0, fy, face, WHITE);
        ui::stroke(v, frame, 6.0, 1.5, with_alpha(mix_colors(accent, WHITE, 0.4), 0.9));
    }
    let ny = BOARD_Y + BOARD_H / 2.0 + 40.0 + size * 0.3;
    ui::radial(v, cx, ny - size * 0.35, size * 0.9, size * 0.6, with_alpha(accent, 0.18 * (0.4 + 0.6 * k)), with_alpha(accent, 0.0));
    ui::text_shadow_centered(v, &n.to_string(), cx, ny, size, with_alpha(WHITE, 0.4 + 0.6 * k));
    // "VS NAME" on a banner.
    let label = format!("VS {}", vs.name());
    let lw = v.measure_mixed(&label, 26.0);
    let by = BOARD_Y + BOARD_H / 2.0 + 142.0;
    let banner = Rect::new(cx - lw / 2.0 - 22.0, by - 20.0, lw + 44.0, 40.0);
    ui::panel(v, banner, &ui::Panel::card().radius(20.0).tint(accent, 0.15));
    ui::stroke(v, banner, 20.0, 1.0, with_alpha(accent, 0.5));
    text_mixed_shadow(v, &label, cx - lw / 2.0, ui::baseline(by, 26.0), 26.0, accent);
}

/// A quick "GO!" as the match starts.
fn draw_go(v: &View, vs: &Versus) {
    if vs.time > 0.6 {
        return;
    }
    let k = vs.time / 0.6;
    let cx = BOARD_X + BOARD_W / 2.0;
    let cy = BOARD_Y + BOARD_H / 2.0;
    let gold = hex(ui::GOLD);
    ui::radial(v, cx, cy, 150.0 + 80.0 * k, 70.0 + 30.0 * k, with_alpha(gold, 0.28 * (1.0 - k)), with_alpha(gold, 0.0));
    ui::text_shadow_centered(v, "GO!", cx, cy + 30.0, 90.0 + 40.0 * k, with_alpha(gold, 1.0 - k));
}

fn draw_versus_results(v: &View, vs: &Versus, won: bool, mine: &Stats, reward: Option<&Reward>) {
    let (title, color) = if won { ("VICTORY", hex(ui::GOLD)) } else { ("DEFEAT", hex(ui::PINK)) };
    let card = results_card(v, color);
    let cx = card.center().x;
    let accent = hex(vs.profile.color);
    let face = 64.0;
    let fr = Rect::new(cx - face / 2.0 - 2.0, BOARD_Y + 26.0, face + 4.0, face + 4.0);
    if !vs.online() {
        ui::shadow(v, fr, 6.0, 10.0, 4.0, 0.5);
        draw_portrait(v, &vs.profile, cx - face / 2.0, BOARD_Y + 28.0, face, if won { Color::new(0.5, 0.5, 0.55, 1.0) } else { WHITE });
        ui::stroke(v, fr, 4.0, 1.5, with_alpha(mix_colors(accent, WHITE, 0.3), 0.9));
    }
    ui::radial(v, cx, BOARD_Y + 124.0, 150.0, 40.0, with_alpha(color, 0.18), with_alpha(color, 0.0));
    ui::text_shadow_centered(v, title, cx, BOARD_Y + 142.0, 52.0, color);
    let vs_line = format!("vs {}", vs.name());
    let rank_w = ui::pill_w(v, vs.profile.rank, 11.0);
    let line_w = v.measure_mixed(&vs_line, 20.0) + 8.0 + rank_w;
    let lx = cx - line_w / 2.0;
    v.text_mixed(&vs_line, lx, BOARD_Y + 170.0, 20.0, accent);
    ui::pill(v, lx + line_w - rank_w, BOARD_Y + 164.0, vs.profile.rank, 11.0, accent, ui::Pill::Soft);
    ui::pill_centered(v, cx, BOARD_Y + 190.0, &stats::clock(vs.time), 12.0, hex(ui::TEXT2), ui::Pill::Soft);

    // You vs them: each value in its own column, the better one bright, and a bar for
    // who came out ahead.
    let theirs = &vs.shadow.stats;
    let (left, right) = (card.x + 18.0, card.right() - 18.0);
    let top = BOARD_Y + 214.0;
    ui::caps(v, "YOU", left, top, 13.0, WHITE, 2.0);
    let nw = v.measure_mixed(vs.name(), 16.0);
    let name_size = if nw > 200.0 { 16.0 * 200.0 / nw } else { 16.0 };
    v.text_mixed_right(vs.name(), right, top, name_size, accent);
    let rows = [
        ("PPS", stats::fmt_rate(mine.pps(), 2), stats::fmt_rate(theirs.pps(), 2), mine.pps().unwrap_or(0.0), theirs.pps().unwrap_or(0.0)),
        ("APM", stats::fmt_rate(mine.apm(), 1), stats::fmt_rate(theirs.apm(), 1), mine.apm().unwrap_or(0.0), theirs.apm().unwrap_or(0.0)),
        ("VS", stats::fmt_rate(mine.vs(), 1), stats::fmt_rate(theirs.vs(), 1), mine.vs().unwrap_or(0.0), theirs.vs().unwrap_or(0.0)),
        ("PIECES", mine.pieces.to_string(), theirs.pieces.to_string(), mine.pieces as f32, theirs.pieces as f32),
        ("LINES", mine.lines.to_string(), theirs.lines.to_string(), mine.lines as f32, theirs.lines as f32),
        ("GARBAGE SENT", vs.sent.to_string(), vs.received.to_string(), vs.sent as f32, vs.received as f32),
        ("QUADS", mine.quads.to_string(), theirs.quads.to_string(), mine.quads as f32, theirs.quads as f32),
        ("MAX COMBO", mine.max_combo.to_string(), theirs.max_combo.to_string(), mine.max_combo as f32, theirs.max_combo as f32),
    ];
    let mine_color = hex(ui::CYAN);
    for (i, (label, a, b, fa, fb)) in rows.iter().enumerate() {
        let y = top + 8.0 + i as f32 * 26.0;
        let cy = y + 11.0;
        let (fa, fb) = (if fa.is_finite() { fa.max(0.0) } else { 0.0 }, if fb.is_finite() { fb.max(0.0) } else { 0.0 });
        let (ca, cb) = if fa > fb { (WHITE, hex(ui::TEXT2)) } else if fb > fa { (hex(ui::TEXT2), WHITE) } else { (hex(ui::INK), hex(ui::INK)) };
        v.text(a, left, ui::baseline(cy, 19.0), 19.0, ca);
        v.text_right(b, right, ui::baseline(cy, 19.0), 19.0, cb);
        ui::caps_centered(v, label, cx, ui::baseline(cy, 12.0), 12.0, hex(ui::TEXT2), 1.5);
        // The tug-of-war bar.
        let share = if fa + fb > 0.0 { fa / (fa + fb) } else { 0.5 };
        let (bx, bw, by) = (left, right - left, y + 22.0);
        let split = bx + bw * share;
        v.rect(bx, by, split - bx - 1.0, 2.0, with_alpha(mine_color, if fa >= fb { 0.75 } else { 0.35 }));
        v.rect(split + 1.0, by, bx + bw - split - 1.0, 2.0, with_alpha(accent, if fb >= fa { 0.75 } else { 0.35 }));
    }
    if let Some(r) = reward {
        draw_reward(v, r, BOARD_Y + 432.0);
    }
    results_hints(v, card, "rematch");
}

/// The opponent select screen: one row per shadow (keyboard or mouse), details below.
pub fn draw_select(shadows: &[Profile], selected: usize, hover: Option<MenuHit>, scene_on: bool, progress: &Progress) {
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    let theme = shadows.get(selected).map_or_else(Theme::classic, Theme::shadow);
    scene::draw_menu_backdrop(&v, CLASSIC_W, theme, scene_on);
    let cx = CLASSIC_W / 2.0;
    ui::text_shadow_centered(&v, "CHOOSE YOUR SHADOW", cx, 92.0, ui::TITLE, WHITE);
    v.text_centered("a true 1v1 against a bot at its own pace", cx, 124.0, 18.0, hex(ui::TEXT3));
    for (i, p) in shadows.iter().enumerate() {
        let r = select_item_rect(i);
        let active = i == selected;
        let hovered = hover == Some(MenuHit::Item(i));
        let accent = hex(p.color);
        ui::button_bg(&v, r, 9.0, ui::State::new(active, hovered), accent);
        if !active {
            // A sliver of the shadow's colour, so the list reads at a glance.
            ui::fill(&v, Rect::new(r.x + 5.0, r.y + 14.0, 3.0, r.h - 28.0), 1.5, with_alpha(accent, 0.7), with_alpha(accent, 0.4));
        }
        let face = r.h - 14.0;
        let (fx, fy) = (r.x + 20.0, r.y + 7.0);
        draw_portrait(&v, p, fx, fy, face, if active { WHITE } else { Color::new(0.7, 0.7, 0.75, 1.0) });
        ui::stroke(&v, Rect::new(fx - 2.0, fy - 2.0, face + 4.0, face + 4.0), 3.0, 1.0, with_alpha(mix_colors(accent, WHITE, 0.3), if active { 0.9 } else { 0.5 }));
        let name_x = fx + face + 14.0;
        let size = if p.name.chars().count() > 8 { 22.0 } else { 26.0 };
        text_mixed_shadow(&v, p.name, name_x, r.y + 24.0, size, if active { WHITE } else { hex(ui::INK) });
        // Your record against them, under the name.
        let rec = progress.record(p.portrait);
        let line_y = r.y + 37.0;
        if rec.wins + rec.losses == 0 {
            v.text("not fought yet", name_x, ui::baseline(line_y, 14.0), 14.0, hex(ui::TEXT3));
        } else {
            let wl = format!("{}W-{}L", rec.wins, rec.losses);
            let w = ui::pill(&v, name_x, line_y, &wl, 10.0, hex(if rec.wins > 0 { ui::GOLD } else { ui::TEXT2 }), ui::Pill::Soft);
            if let Some(t) = rec.best_time {
                ui::pill(&v, name_x + w + 6.0, line_y, &format!("BEST {}", stats::clock(t)), 10.0, hex(ui::GOLD), ui::Pill::Soft);
            }
        }
        ui::pill_right(&v, r.right() - 16.0, r.y + 17.0, p.rank, 11.0, accent, if active { ui::Pill::Solid } else { ui::Pill::Soft });
        v.text_right(&format!("{:.0}-{:.0} PPM", p.ppm.0, p.ppm.1), r.right() - 16.0, r.y + 42.0, 15.0, hex(ui::TEXT2));
    }
    if let Some(p) = shadows.get(selected) {
        // About the focused shadow: the blurb, your bests, what beating them unlocks.
        let first = select_item_rect(0);
        let card = Rect::new(first.x, select_item_rect(shadows.len()).y + 6.0, first.w, 86.0);
        let accent = hex(p.color);
        ui::panel(&v, card, &ui::Panel::card().tint(accent, 0.06));
        ui::accent_bar(&v, card, accent);
        v.text_centered(p.blurb, cx, card.y + 28.0, ui::fit(&v, p.blurb, 18.0, card.w - 40.0), hex(ui::TEXT2));
        ui::rule(&v, card.x + 20.0, card.y + 42.0, card.w - 40.0, Color::new(1.0, 1.0, 1.0, 0.1));
        let rec = progress.record(p.portrait);
        let line = card.y + 63.0;
        let mut x = card.x + 20.0;
        if let Some(apm) = rec.best_apm {
            x += ui::pill(&v, x, line, &format!("BEST APM {apm:.1}"), 10.0, hex(ui::CYAN), ui::Pill::Soft) + 6.0;
        }
        if let Some(pps) = rec.best_pps {
            x += ui::pill(&v, x, line, &format!("BEST PPS {pps:.2}"), 10.0, hex(ui::CYAN), ui::Pill::Soft) + 6.0;
        }
        if x == card.x + 20.0 {
            ui::caps(&v, "NO BESTS YET", x, ui::baseline(line, 11.0), 11.0, hex(ui::TEXT4), 1.5);
        }
        if let Some(item) = Item::all().find(|&i| i.unlock() == skins::Unlock::Beat(p.portrait)) {
            let got = progress.unlocked(item);
            let right = card.right() - 20.0;
            let chip_w = if got {
                ui::pill_right(&v, right, line, "UNLOCKED", 10.0, hex(ui::GREEN), ui::Pill::Solid)
            } else {
                ui::pill_right(&v, right, line, "BEAT THEM TO UNLOCK", 10.0, hex(ui::ORANGE), ui::Pill::Soft)
            };
            let label = format!("{} {}", item.name(), item.kind());
            let tx = right - chip_w - 8.0;
            v.text_right(&label, tx, ui::baseline(line, 16.0), 16.0, WHITE);
            let sw = 20.0;
            draw_item_swatch(&v, item, progress.skin, tx - ui::text_w(&v, &label, 16.0) - sw - 8.0, line - sw / 2.0, sw);
        }
    }
    ui::hints(&v, cx, 722.0, &[("Up/Down", "or mouse to choose"), ("Enter", "or click to fight"), ("Esc", "back")], 13.0);
}

fn select_item_rect(i: usize) -> Rect {
    Rect::new(CLASSIC_W / 2.0 - 240.0, 144.0 + i as f32 * 56.0, 480.0, 50.0)
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
const MENU_ITEM_H: f32 = 50.0;
const MENU_ITEM_GAP: f32 = 12.0;
const MENU_TOP: f32 = 270.0;
const TOGGLE_W: f32 = 200.0;
const TOGGLE_H: f32 = 40.0;
const TOGGLE_GAP: f32 = 20.0;

/// Something clickable on the main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuHit {
    Item(usize),
    /// Opens the settings screen.
    Settings,
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
    let y = MENU_TOP + items as f32 * (MENU_ITEM_H + MENU_ITEM_GAP) + 44.0;
    let left = CLASSIC_W / 2.0 - TOGGLE_W - TOGGLE_GAP / 2.0;
    let slot = match which {
        MenuHit::Settings => 0.0,
        _ => 1.0,
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
        .chain([MenuHit::Settings, MenuHit::Sound])
        .find(|&hit| {
            let r = match hit {
                MenuHit::Item(i) => menu_item_rect(i),
                toggle => menu_toggle_rect(items, toggle),
            };
            r.contains(p)
        })
}

pub fn draw_menu(items: &[MenuItem], selected: usize, hover: Option<MenuHit>, sound_on: bool, scene_on: bool, progress: &Progress) {
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

    // The title lockup: the two words over a warm glow, a tagline between rules.
    ui::radial(&v, cx, 124.0, 300.0, 70.0, Color::new(1.0, 0.55, 0.35, 0.10), Color::new(1.0, 0.55, 0.35, 0.0));
    let (bw, gap) = (ui::text_w(&v, "BLOCK", ui::DISPLAY), 24.0);
    let total = bw + gap + ui::text_w(&v, "BRAWL", ui::DISPLAY);
    let tx = cx - total / 2.0;
    ui::text_shadow(&v, "BLOCK", tx, 150.0, ui::DISPLAY, hex(ui::GOLD));
    ui::text_shadow(&v, "BRAWL", tx + bw + gap, 150.0, ui::DISPLAY, hex(ui::PINK));
    let tag = "A FALLING-BLOCK BATTLER";
    let tw = ui::caps_w(&v, tag, 13.0, 3.0);
    ui::caps_centered(&v, tag, cx, 182.0, 13.0, hex(ui::TEXT2), 3.0);
    for (x, w) in [(cx - tw / 2.0 - 96.0, 84.0), (cx + tw / 2.0 + 12.0, 84.0)] {
        ui::rule(&v, x, 177.0, w, Color::new(1.0, 1.0, 1.0, 0.18));
    }
    draw_level_card(&v, progress, cx, 198.0);

    for (i, item) in items.iter().enumerate() {
        let r = menu_item_rect(i);
        let st = ui::State::new(i == selected, hover == Some(MenuHit::Item(i)));
        ui::button(&v, r, item.title, 30.0, st, hex(ui::GOLD));
        // Its number on the left, a chevron on the right when focused.
        ui::caps(&v, &format!("0{}", i + 1), r.x + 22.0, ui::baseline(r.center().y, 12.0), 12.0, if st.focused { hex(ui::GOLD) } else { hex(ui::TEXT4) }, 1.0);
        if st.focused {
            ui::chevron(&v, r.right() - 26.0, r.center().y, 14.0, 1.0, hex(ui::GOLD));
        }
    }
    if let Some(item) = items.get(selected) {
        let y = menu_item_rect(items.len()).y + 16.0;
        v.text_centered(item.blurb, cx, y, ui::fit(&v, item.blurb, 18.0, 600.0), hex(ui::TEXT2));
    }

    for which in [MenuHit::Settings, MenuHit::Sound] {
        let r = menu_toggle_rect(items.len(), which);
        let st = ui::State::new(false, hover == Some(which));
        ui::button_bg(&v, r, 8.0, st, hex(ui::GOLD));
        let col = ui::label_color(st);
        let ty = ui::baseline(r.center().y, 17.0);
        let ix = r.x + 26.0;
        match which {
            MenuHit::Settings => {
                draw_gear(&v, ix, r.center().y, col, hex(0x161a28));
                ui::caps(&v, "SETTINGS", ix + 18.0, ty, 17.0, col, 2.0);
            }
            _ => {
                draw_speaker(&v, ix, r.center().y, sound_on, col);
                ui::caps(&v, "SOUND", ix + 18.0, ty, 17.0, col, 2.0);
                let (label, color) = if sound_on { ("ON", hex(ui::GREEN)) } else { ("OFF", hex(ui::TEXT2)) };
                ui::pill_right(&v, r.right() - 12.0, r.center().y, label, 11.0, color, ui::Pill::Soft);
            }
        }
    }

    ui::hints(&v, cx, 712.0, &[("Up/Down", "or mouse to choose"), ("Enter", "or click to play"), ("M/E/N/J", "quick toggles")], 13.0);
    v.text_centered("made by xSolstice1  ·  github.com/xSolstice1", cx, 746.0, 14.0, hex(ui::TEXT4));
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
/// Top of the focused item's card.
const SKIN_INFO_Y: f32 = 560.0;

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
    // Hold and next, beside the field, as in play.
    let side = x + bw + 16.0;
    let (pw, ph) = (72.0, 54.0);
    let muted = hex(ui::TEXT2);
    let look = ui::Panel::themed(t.panel, t.panel_line).radius(7.0);
    ui::caps(v, "HOLD", side, y + 12.0, 11.0, muted, 1.5);
    ui::panel(v, Rect::new(side, y + 18.0, pw, ph), &look);
    draw_piece_sized(v, PieceKind::L, side + pw / 2.0, y + 18.0 + ph / 2.0, 12.0, piece_color(PieceKind::L));
    let next_y = y + 96.0;
    ui::caps(v, "NEXT", side, next_y, 11.0, muted, 1.5);
    ui::panel(v, Rect::new(side, next_y + 6.0, pw, 150.0), &look);
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
    let backdrop = hex(preview.theme.colors().backdrop);
    clear_background(backdrop);
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    // A soft vignette so the panels lift off any backdrop.
    ui::radial(&v, CLASSIC_W / 2.0, 330.0, 560.0, 460.0, Color::new(1.0, 1.0, 1.0, 0.035), Color::new(0.0, 0.0, 0.0, 0.25));
    let cx = CLASSIC_W / 2.0;
    ui::text_shadow_centered(&v, "SKINS", cx, 58.0, ui::TITLE, WHITE);
    draw_level_card(&v, progress, cx, 74.0);

    for (c, title) in SKIN_COLUMNS.iter().enumerate() {
        let head = skin_item_rect(c, 0);
        ui::section(&v, head.x, head.y - 12.0, SKIN_COL_W, title, if c == col { hex(ui::GOLD) } else { hex(ui::TEXT2) });
        for (r, item) in skin_column(c).into_iter().enumerate() {
            let rect = skin_item_rect(c, r);
            let active = c == col && r == row;
            let hovered = hover == Some(SkinsHit::Item { col: c, row: r });
            let unlocked = progress.unlocked(item);
            let on = item.equipped_in(equipped);
            let st = ui::State { focused: active, hovered, disabled: !unlocked && !active && !hovered, pressed: false };
            ui::button_bg(&v, rect, 7.0, st, hex(ui::GOLD));
            if on && !active {
                ui::stroke(&v, rect, 7.0, 1.0, with_alpha(hex(ui::GREEN), 0.6));
            }
            // A swatch of the item drawn in its own look.
            let sw = rect.h - 14.0;
            let (sx, sy) = (rect.x + 12.0, rect.y + 7.0);
            draw_item_swatch(&v, item, equipped, sx, sy, sw);
            if !unlocked {
                ui::fill(&v, Rect::new(sx - 1.0, sy - 1.0, sw + 2.0, sw + 2.0), 3.0, Color::new(0.04, 0.05, 0.08, 0.72), Color::new(0.04, 0.05, 0.08, 0.8));
                draw_lock(&v, sx + sw / 2.0, sy + sw / 2.0);
            }
            let text = match (unlocked, active) {
                (true, true) => WHITE,
                (true, false) => hex(ui::TEXT2),
                (false, true) => hex(ui::TEXT2),
                (false, false) => hex(ui::TEXT4),
            };
            v.text(item.name(), sx + sw + 8.0, ui::baseline(rect.center().y, 16.0), ui::fit(&v, item.name(), 16.0, rect.right() - sx - sw - 26.0), text);
            if on {
                // Equipped: a green dot with a halo.
                let (dx, dy) = (rect.right() - 11.0, rect.center().y);
                ui::radial(&v, dx, dy, 8.0, 8.0, with_alpha(hex(ui::GREEN), 0.4), with_alpha(hex(ui::GREEN), 0.0));
                v.circle(dx, dy, 3.0, hex(ui::GREEN));
            }
        }
    }

    // The preview, framed, with the collection count.
    let frame = skin_preview_frame();
    let total = Item::all().count();
    let owned = Item::all().filter(|&i| progress.unlocked(i)).count();
    ui::section(&v, frame.x, SKIN_TOP - 12.0, frame.w - 110.0, "PREVIEW", hex(ui::TEXT2));
    ui::pill_right(&v, frame.right(), SKIN_TOP - 17.0, &format!("{owned} / {total} UNLOCKED"), 10.0, hex(ui::TEXT2), ui::Pill::Soft);
    ui::panel(&v, frame, &ui::Panel::card().radius(12.0));
    skins::with(preview, || draw_skin_preview(&v, SKIN_PREVIEW_X, SKIN_TOP + 14.0));

    // About the focused item.
    if let Some(item) = focused {
        let card = Rect::new(SKIN_COL_X, SKIN_INFO_Y, CLASSIC_W - 2.0 * SKIN_COL_X, 84.0);
        let on = item.equipped_in(equipped);
        let unlocked = progress.unlocked(item);
        let accent = if on { hex(ui::GREEN) } else if unlocked { hex(ui::GOLD) } else { hex(ui::ORANGE) };
        ui::panel(&v, card, &ui::Panel::card().tint(accent, 0.05));
        ui::accent_bar(&v, card, accent);
        let sw = 52.0;
        draw_item_swatch(&v, item, equipped, card.x + 22.0, card.y + 16.0, sw);
        ui::stroke(&v, Rect::new(card.x + 20.0, card.y + 14.0, sw + 4.0, sw + 4.0), 5.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.18));
        let tx = card.x + 22.0 + sw + 18.0;
        ui::text_shadow(&v, item.name(), tx, card.y + 38.0, 30.0, WHITE);
        let name_w = ui::text_w(&v, item.name(), 30.0);
        ui::pill(&v, tx + name_w + 12.0, card.y + 28.0, &item.kind().to_uppercase(), 10.0, hex(ui::TEXT2), ui::Pill::Soft);
        let line = card.y + 62.0;
        if on {
            ui::pill(&v, tx, line, "EQUIPPED", 11.0, hex(ui::GREEN), ui::Pill::Solid);
        } else if unlocked {
            let w = ui::keycap(&v, tx, line, "Enter", 14.0);
            v.text("or click to equip", tx + w + 8.0, ui::baseline(line, 18.0), 18.0, hex(ui::GOLD));
        } else {
            let w = ui::pill(&v, tx, line, "LOCKED", 11.0, hex(ui::ORANGE), ui::Pill::Soft);
            v.text_mixed(&format!("{} to unlock", progress::unlock_hint(item)), tx + w + 10.0, ui::baseline(line, 18.0), 18.0, hex(ui::INK));
        }
    }

    let back = skin_back_rect();
    back_button(&v, back, "BACK", hover == Some(SkinsHit::Back));
    ui::hints(&v, cx, 724.0, &[("Arrows", "or mouse to browse"), ("Enter", "or click to equip"), ("Esc", "back")], 13.0);
}

/// A small padlock.
fn draw_lock(v: &View, cx: f32, cy: f32) {
    let c = hex(ui::TEXT2);
    v.circle_lines(cx, cy - 2.0, 4.0, 1.5, c);
    ui::fill(v, Rect::new(cx - 5.5, cy - 1.0, 11.0, 8.0), 2.0, mix_colors(c, WHITE, 0.3), c);
    v.rect(cx - 0.75, cy + 1.5, 1.5, 3.0, hex(0x1a1e2c));
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
            assert!(a.bottom() < SKIN_INFO_Y || *a == skin_back_rect(), "{a:?}");
            for b in &rects[i + 1..] {
                assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
            }
        }
        let frame = skin_preview_frame();
        assert!(frame.right() <= CLASSIC_W && frame.x > skin_item_rect(SKIN_COLUMNS.len() - 1, 0).right(), "preview frame {frame:?}");
        // The field (and its theme's outer frame) sits inside the preview frame.
        assert!(SKIN_TOP + 14.0 + PREVIEW_ROWS as f32 * PREVIEW_CELL + 8.0 <= frame.bottom());
        assert!(frame.bottom() < SKIN_INFO_Y);
    }

    #[test]
    fn preview_rows_are_full_width() {
        for row in PREVIEW_STACK {
            assert_eq!(row.len(), WIDTH);
        }
    }

    #[test]
    fn menu_targets_never_overlap_and_fit_the_canvas() {
        for items in 1..=5 {
            let mut rects: Vec<Rect> = (0..items).map(menu_item_rect).collect();
            rects.push(menu_toggle_rect(items, MenuHit::Settings));
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
    fn party_fields_fit_the_panel_for_any_room_size() {
        for n in 1..crate::online::PARTY_MAX {
            let (cols, rows, cell) = party_grid(n);
            assert!(cols * rows >= n, "{n}");
            assert!(cell >= 8.0, "{n}: cell {cell} too small to read");
            assert!(cols as f32 * (WIDTH as f32 * cell + 12.0) <= PARTY_W, "{n} too wide");
            assert!(rows as f32 * (VISIBLE_ROWS as f32 * cell + 30.0) <= PARTY_H, "{n} too tall");
        }
    }

    #[test]
    fn text_sizes_snap_to_a_few_glyph_sizes() {
        let sizes: std::collections::BTreeSet<u16> = (10..400).map(|px| glyph_size(px as f32 * 0.5).0).collect();
        assert!(sizes.len() <= GLYPH_SIZES.len());
        let (s, k) = glyph_size(30.0);
        assert!(s >= 30 && (k * s as f32 - 30.0).abs() < 1e-3 && k <= 1.0);
    }

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


// ───────────── Settings ─────────────

/// Something clickable on the settings screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsHit {
    Tab(usize),
    Row(usize),
    /// A slider row, and where along the bar (0..1).
    Slider(usize, f32),
    /// A key row's slot (0 or 1).
    KeySlot(usize, usize),
    Back,
}

const SET_X: f32 = 70.0;
const SET_W: f32 = CLASSIC_W - 140.0;
const SET_TABS_Y: f32 = 132.0;
const SET_TOP: f32 = 196.0;
const SET_ROW_H: f32 = 46.0;
const SET_CTRL_X: f32 = SET_X + 330.0;
const SET_CTRL_W: f32 = SET_W - 330.0 - 18.0;

fn settings_tab_rect(i: usize) -> Rect {
    let w = SET_W / settings_ui::TABS.len() as f32;
    Rect::new(SET_X + i as f32 * w, SET_TABS_Y, w - 6.0, 40.0)
}

fn settings_row_rect(i: usize) -> Rect {
    Rect::new(SET_X, SET_TOP + i as f32 * SET_ROW_H, SET_W, SET_ROW_H - 6.0)
}

fn settings_slider_rect(i: usize) -> Rect {
    let r = settings_row_rect(i);
    Rect::new(SET_CTRL_X, r.y + r.h / 2.0 - 5.0, SET_CTRL_W - 110.0, 10.0)
}

fn settings_key_rect(i: usize, slot: usize) -> Rect {
    let r = settings_row_rect(i);
    let w = (SET_CTRL_W - 10.0) / 2.0;
    Rect::new(SET_CTRL_X + slot as f32 * (w + 10.0), r.y + 6.0, w, r.h - 12.0)
}

fn settings_back_rect() -> Rect {
    Rect::new(SET_X, 688.0, 150.0, 40.0)
}

/// What's under `screen_pos` on the settings screen showing `tab`.
pub fn settings_hit(screen_pos: Vec2, tab: usize) -> Option<SettingsHit> {
    let p = View::fit_window(Vec2::ZERO, CLASSIC_W).to_virtual(screen_pos);
    if settings_back_rect().contains(p) {
        return Some(SettingsHit::Back);
    }
    if let Some(t) = (0..settings_ui::TABS.len()).find(|&t| settings_tab_rect(t).contains(p)) {
        return Some(SettingsHit::Tab(t));
    }
    let rows = settings_ui::rows(tab);
    let i = (0..rows.len()).find(|&i| settings_row_rect(i).contains(p))?;
    Some(match rows[i].kind() {
        Kind::Slider { .. } => {
            let bar = settings_slider_rect(i);
            // Generous hit area: anywhere over the bar's span on that row.
            if p.x >= bar.x - 8.0 && p.x <= bar.right() + 8.0 {
                SettingsHit::Slider(i, (p.x - bar.x) / bar.w)
            } else {
                SettingsHit::Row(i)
            }
        }
        Kind::Key(_) => match (0..2).find(|&s| settings_key_rect(i, s).contains(p)) {
            Some(s) => SettingsHit::KeySlot(i, s),
            None => SettingsHit::Row(i),
        },
        _ => SettingsHit::Row(i),
    })
}

/// The settings screen: tabs across the top, a row per setting (sliders, switches,
/// key slots), a line about the focused row, and a Back button.
#[allow(clippy::too_many_arguments)]
pub fn draw_settings(s: &Settings, extra: Extra, tab: usize, row: usize, slot: usize, rebinding: Option<(Bind, usize)>, hover: Option<SettingsHit>, from_pause: bool) {
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    scene::draw_menu_backdrop(&v, CLASSIC_W, Theme::classic(), s.scene);
    let cx = CLASSIC_W / 2.0;
    let gold = hex(ui::GOLD);
    let muted = hex(ui::TEXT2);
    let faint = hex(ui::TEXT3);
    ui::text_shadow_centered(&v, "SETTINGS", cx, 84.0, ui::TITLE, WHITE);
    if from_pause {
        ui::pill_centered(&v, cx, 110.0, "THE RUN IS PAUSED", 10.0, hex(ui::CYAN), ui::Pill::Soft);
    }

    // Tabs: a segmented control.
    let first = settings_tab_rect(0);
    let last = settings_tab_rect(settings_ui::TABS.len() - 1);
    let track = Rect::new(first.x - 4.0, first.y - 4.0, last.right() - first.x + 8.0, first.h + 8.0);
    ui::panel(&v, track, &ui::Panel::well().radius(11.0));
    for (i, name) in settings_ui::TABS.iter().enumerate() {
        let r = settings_tab_rect(i);
        let on = i == tab;
        let hovered = hover == Some(SettingsHit::Tab(i));
        if on {
            ui::shadow(&v, r, 8.0, 8.0, 3.0, 0.45);
            ui::fill(&v, r, 8.0, hex(0x2a3150), hex(0x1b2032));
            ui::stroke(&v, r, 8.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.12));
            ui::shine(&v, r, 8.0, 0.16);
            let u = Rect::new(r.center().x - r.w * 0.2, r.bottom() - 4.0, r.w * 0.4, 2.0);
            ui::glow(&v, u, 1.0, 6.0, with_alpha(gold, 0.4));
            ui::fill(&v, u, 1.0, gold, gold);
        } else if hovered {
            ui::fill(&v, r, 8.0, Color::new(1.0, 1.0, 1.0, 0.06), Color::new(1.0, 1.0, 1.0, 0.03));
        }
        ui::caps_centered(&v, name, r.center().x, ui::baseline(r.center().y, 15.0), 15.0, if on { WHITE } else if hovered { hex(ui::INK) } else { muted }, 2.0);
    }

    let rows = settings_ui::rows(tab);
    for (i, &item) in rows.iter().enumerate() {
        let r = settings_row_rect(i);
        let focused = i == row;
        let hovered = matches!(hover, Some(SettingsHit::Row(h) | SettingsHit::Slider(h, _) | SettingsHit::KeySlot(h, _)) if h == i);
        if focused {
            ui::shadow(&v, r, 8.0, 10.0, 3.0, 0.4);
            ui::fill(&v, r, 8.0, a_mix(hex(0x262d45), gold, 0.04), hex(0x181c2b));
            ui::stroke(&v, r, 8.0, 1.0, with_alpha(gold, 0.45));
            ui::shine(&v, r, 8.0, 0.12);
            ui::accent_bar(&v, r, gold);
        } else {
            ui::fill(&v, r, 8.0, Color::new(0.09, 0.11, 0.16, 0.9), Color::new(0.06, 0.07, 0.11, 0.9));
            ui::stroke(&v, r, 8.0, 1.0, Color::new(1.0, 1.0, 1.0, if hovered { 0.14 } else { 0.05 }));
        }
        let cy = r.center().y;
        let ty = ui::baseline(cy, ui::BODY);
        v.text(item.label(), r.x + 20.0, ty, ui::BODY, if focused { WHITE } else { hex(ui::INK) });
        match item.kind() {
            Kind::Slider { .. } => {
                let bar = settings_slider_rect(i);
                let f = item.fraction(s);
                let look = if focused { ui::Bar::new(hex(0xff9a3c), gold) } else { ui::Bar::new(hex(0x6a5a2a), hex(0x9a8540)).glow(0.0) };
                ui::progress(&v, bar, f, &look.stretch());
                let knob = Rect::new(bar.x + bar.w * f - 8.0, bar.center().y - 8.0, 16.0, 16.0);
                ui::shadow(&v, knob, 8.0, 6.0, 2.0, 0.5);
                if focused {
                    ui::glow(&v, knob, 8.0, 6.0, with_alpha(gold, 0.35));
                }
                ui::fill(&v, knob, 8.0, WHITE, hex(0xc9cfdc));
                ui::stroke(&v, knob, 8.0, 1.0, if focused { gold } else { Color::new(0.0, 0.0, 0.0, 0.25) });
                v.text_right(&item.shown(s, extra), r.right() - 18.0, ty, 18.0, if focused { WHITE } else { muted });
            }
            Kind::Toggle => {
                let on = item.on(s, extra);
                let (pw, ph) = (54.0, 26.0);
                let sw = Rect::new(r.right() - 18.0 - pw, cy - ph / 2.0, pw, ph);
                if on {
                    ui::glow(&v, sw, ph / 2.0, 6.0, with_alpha(hex(ui::GREEN), 0.25));
                    ui::fill(&v, sw, ph / 2.0, hex(0x3fc46a), hex(0x26894a));
                } else {
                    ui::fill(&v, sw, ph / 2.0, hex(0x0b0d14), hex(0x1d2232));
                }
                ui::stroke(&v, sw, ph / 2.0, 1.0, Color::new(1.0, 1.0, 1.0, if on { 0.25 } else { 0.08 }));
                let kx = if on { sw.right() - ph / 2.0 } else { sw.x + ph / 2.0 };
                let k = Rect::new(kx - 10.0, cy - 10.0, 20.0, 20.0);
                ui::shadow(&v, k, 10.0, 5.0, 2.0, 0.5);
                ui::fill(&v, k, 10.0, WHITE, if on { hex(0xdfe5ee) } else { hex(0x9aa3b8) });
                ui::caps_right(&v, if on { "ON" } else { "OFF" }, sw.x - 12.0, ui::baseline(cy, 13.0), 13.0, if on { hex(ui::GREEN) } else { faint }, 2.0);
            }
            Kind::Choice => {
                let label = item.shown(s, extra);
                let box_r = Rect::new(r.right() - 18.0 - 190.0, cy - 14.0, 190.0, 28.0);
                ui::panel(&v, box_r, &ui::Panel::well().radius(7.0));
                let col = if focused { gold } else { muted };
                ui::chevron(&v, box_r.x + 14.0, cy, 10.0, -1.0, col);
                ui::chevron(&v, box_r.right() - 14.0, cy, 10.0, 1.0, col);
                v.text_centered(&label, box_r.center().x, ty, ui::fit(&v, &label, 18.0, box_r.w - 50.0), if focused { WHITE } else { hex(ui::INK) });
            }
            Kind::Key(b) => {
                let keys = s.keys.get(b);
                for (k, key) in keys.iter().enumerate() {
                    let kr = settings_key_rect(i, k);
                    let waiting = rebinding == Some((b, k));
                    let picked = focused && slot == k;
                    let hovered = hover == Some(SettingsHit::KeySlot(i, k));
                    // A keycap: a lip under a raised face.
                    let face = Rect::new(kr.x, kr.y, kr.w, kr.h - 3.0);
                    ui::fill(&v, Rect::new(kr.x, kr.y + 3.0, kr.w, kr.h - 3.0), 6.0, hex(0x0a0c12), hex(0x07080c));
                    if waiting {
                        ui::glow(&v, face, 6.0, 8.0, with_alpha(gold, 0.35));
                        ui::fill(&v, face, 6.0, hex(0x4a3f16), hex(0x2e2810));
                    } else if picked || hovered {
                        ui::fill(&v, face, 6.0, hex(0x343b55), hex(0x232839));
                    } else {
                        ui::fill(&v, face, 6.0, hex(0x262b3e), hex(0x191d2b));
                    }
                    let edge = if waiting { gold } else if picked { with_alpha(gold, 0.6) } else { Color::new(1.0, 1.0, 1.0, if hovered { 0.2 } else { 0.08 }) };
                    ui::stroke(&v, face, 6.0, 1.0, edge);
                    ui::shine(&v, face, 6.0, 0.1);
                    let text = if waiting { "press a key".to_owned() } else { key.map_or("none".to_owned(), crate::settings::key_name) };
                    let color = if waiting { gold } else if key.is_some() { WHITE } else { faint };
                    v.text_centered(&text, face.center().x, ui::baseline(face.center().y, 18.0), 18.0, color);
                }
            }
            Kind::Button => {
                let w = ui::keycap_w(&v, "Enter", 14.0);
                ui::keycap(&v, r.right() - 18.0 - w, cy, "Enter", 14.0);
                if focused {
                    ui::glow(&v, Rect::new(r.right() - 18.0 - w, cy - 11.5, w, 21.0), 4.0, 6.0, with_alpha(gold, 0.3));
                }
            }
        }
    }
    if let Some(&item) = rows.get(row) {
        let y = SET_TOP + rows.len() as f32 * SET_ROW_H + 18.0;
        v.text_centered(item.hint(), cx, y, ui::fit(&v, item.hint(), 17.0, SET_W), muted);
    }

    let b = settings_back_rect();
    back_button(&v, b, if from_pause { "BACK TO RUN" } else { "BACK" }, hover == Some(SettingsHit::Back));
    ui::hints(&v, cx, 745.0, &[("Up/Down", "row"), ("Left/Right", "adjust"), ("Enter", "toggle / rebind"), ("Tab/[/]", "tabs"), ("Esc", "back")], 12.0);
}

#[cfg(test)]
mod settings_layout_tests {
    use super::*;

    #[test]
    fn settings_rows_fit_above_the_back_button_and_never_overlap() {
        for tab in 0..settings_ui::TABS.len() {
            let n = settings_ui::rows(tab).len();
            let hint_y = SET_TOP + n as f32 * SET_ROW_H + 18.0;
            assert!(hint_y < settings_back_rect().y - 6.0, "tab {tab}: hint at {hint_y}");
            for i in 0..n {
                assert!(!settings_row_rect(i).overlaps(&settings_row_rect(i + 1)) || i + 1 == n);
                assert!(settings_key_rect(i, 0).right() < settings_key_rect(i, 1).x);
                assert!(settings_key_rect(i, 1).right() <= settings_row_rect(i).right());
            }
        }
    }
}

/// The card every results screen sits in, over the dimmed field.
fn results_card(v: &View, accent: Color) -> Rect {
    ui::fill(v, Rect::new(BOARD_X, BOARD_Y, BOARD_W, BOARD_H), 0.0, Color::new(0.02, 0.03, 0.05, 0.72), Color::new(0.02, 0.03, 0.05, 0.86));
    let r = Rect::new(BOARD_X + 10.0, BOARD_Y + 14.0, BOARD_W - 20.0, BOARD_H - 28.0);
    ui::panel(v, r, &ui::Panel::card().radius(14.0).tint(accent, 0.05));
    // A wash of the accent behind the title.
    ui::radial(v, r.center().x, r.y + 70.0, 170.0, 80.0, with_alpha(accent, 0.12), with_alpha(accent, 0.0));
    r
}

/// Keys at the bottom of a results card.
fn results_hints(v: &View, card: Rect, again: &str) {
    ui::hints(v, card.center().x, card.bottom() - 18.0, &[("Enter", again), ("Esc", "menu")], 15.0);
}

/// Label on the left, value right-aligned, in faint alternating stripes.
fn stat_table(v: &View, left: f32, right: f32, top: f32, rows: &[(&str, String)], row_h: f32) {
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = top + i as f32 * row_h;
        if i % 2 == 0 {
            ui::fill(v, Rect::new(left - 6.0, y, right - left + 12.0, row_h), 5.0, Color::new(1.0, 1.0, 1.0, 0.03), Color::new(1.0, 1.0, 1.0, 0.025));
        }
        let cy = y + row_h / 2.0;
        let size = (row_h * 0.75).min(17.0);
        v.text(label, left, ui::baseline(cy, size), size, hex(ui::TEXT2));
        v.text_right(value, right, ui::baseline(cy, size), size, WHITE);
    }
}

/// An item drawn in its own look, in a `sw` square at (sx, sy) (skins list, unlocks).
fn draw_item_swatch(v: &View, item: Item, equipped: skins::Skin, sx: f32, sy: f32, sw: f32) {
    skins::with(item.apply(equipped), || match item {
        Item::Style(st) => v.block_styled(st, sx, sy, sw, piece_color(PieceKind::T)),
        Item::Theme(th) => {
            let tc = th.colors();
            ui::fill(v, Rect::new(sx, sy, sw, sw), 4.0, mix_colors(hex(tc.frame), WHITE, 0.15), hex(tc.frame));
            ui::fill(v, Rect::new(sx + 2.5, sy + 2.5, sw - 5.0, sw - 5.0), 2.5, mix_colors(hex(tc.field), WHITE, 0.05), hex(tc.field));
            v.rect(sx + sw / 2.0, sy + 3.0, 1.0, sw - 6.0, hex(tc.grid));
            v.rect(sx + 3.0, sy + sw / 2.0, sw - 6.0, 1.0, hex(tc.grid));
        }
        Item::Palette(_) => {
            for (k, kind) in [PieceKind::I, PieceKind::T, PieceKind::Z, PieceKind::O].into_iter().enumerate() {
                let h = sw / 2.0;
                let (x, y) = (sx + (k % 2) as f32 * h, sy + (k / 2) as f32 * h);
                let c = piece_color(kind);
                ui::fill(v, Rect::new(x + 0.5, y + 0.5, h - 1.0, h - 1.0), 2.5, mix_colors(c, WHITE, 0.2), mix_colors(c, BLACK, 0.15));
            }
        }
    });
}

/// `text_mixed` over a soft drop shadow.
fn text_mixed_shadow(v: &View, t: &str, x: f32, y: f32, size: f32, c: Color) {
    let o = (size / 26.0).max(1.0);
    v.text_mixed(t, x + o * 0.5, y + o, size, Color::new(0.0, 0.0, 0.03, 0.55 * c.a));
    v.text_mixed(t, x, y, size, c);
}

/// A small cog.
fn draw_gear(v: &View, cx: f32, cy: f32, c: Color, hole: Color) {
    for i in 0..8 {
        let a = i as f32 / 8.0 * std::f32::consts::TAU;
        v.circle(cx + a.cos() * 6.5, cy + a.sin() * 6.5, 2.2, c);
    }
    v.circle(cx, cy, 6.0, c);
    v.circle(cx, cy, 2.6, hole);
}

/// A small speaker, with waves when on.
fn draw_speaker(v: &View, cx: f32, cy: f32, on: bool, c: Color) {
    v.rect(cx - 7.0, cy - 3.0, 4.0, 6.0, c);
    v.triangle(vec2(cx - 4.0, cy - 3.0), vec2(cx + 1.0, cy - 7.5), vec2(cx + 1.0, cy + 7.5), c);
    v.triangle(vec2(cx - 4.0, cy - 3.0), vec2(cx + 1.0, cy + 7.5), vec2(cx - 4.0, cy + 3.0), c);
    if on {
        // Two sound waves, as arcs of dots.
        for (r, a) in [(5.0, 0.85), (8.5, 0.55)] {
            for k in 0..9 {
                let t = (k as f32 / 8.0 - 0.5) * 1.8;
                v.circle(cx + 1.5 + t.cos() * r, cy + t.sin() * r, 0.85, with_alpha(c, a));
            }
        }
    } else {
        let x = cx + 6.0;
        v.triangle(vec2(x - 3.0, cy - 4.0), vec2(x - 2.0, cy - 4.0), vec2(x + 3.0, cy + 4.0), c);
        v.triangle(vec2(x - 3.0, cy - 4.0), vec2(x + 3.0, cy + 4.0), vec2(x + 2.0, cy + 4.0), c);
        v.triangle(vec2(x + 3.0, cy - 4.0), vec2(x + 2.0, cy - 4.0), vec2(x - 3.0, cy + 4.0), c);
        v.triangle(vec2(x + 3.0, cy - 4.0), vec2(x - 3.0, cy + 4.0), vec2(x - 2.0, cy + 4.0), c);
    }
}

/// Level badge, "LEVEL 4" and the XP to the next level over a bar, in a card `cx`-centred
/// from `y` (50 tall).
fn draw_level_card(v: &View, progress: &Progress, cx: f32, y: f32) {
    let (level, into, need) = progress::level_of(progress.xp);
    let card = Rect::new(cx - 160.0, y, 320.0, 50.0);
    ui::panel(v, card, &ui::Panel::card().radius(12.0));
    let gold = hex(ui::GOLD);
    ui::level_badge(v, card.x + 28.0, card.center().y, 16.0, level, gold);
    let (l, r) = (card.x + 56.0, card.right() - 16.0);
    ui::caps(v, &format!("LEVEL {level}"), l, card.y + 20.0, 13.0, hex(ui::INK), 2.0);
    let (frac, xp) = if level >= progress::MAX_LEVEL { (1.0, "MAX".to_owned()) } else { (into as f32 / need.max(1) as f32, format!("{into} / {need} XP")) };
    ui::caps_right(v, &xp, r, card.y + 20.0, 12.0, hex(ui::TEXT2), 1.0);
    ui::progress(v, Rect::new(l, card.y + 28.0, r - l, 10.0), frac, &ui::Bar::xp().ticks(10).glow(0.8));
}

/// The frame around the skins preview.
fn skin_preview_frame() -> Rect {
    Rect::new(SKIN_PREVIEW_X - 14.0, SKIN_TOP, WIDTH as f32 * PREVIEW_CELL + 16.0 + 72.0 + 28.0, PREVIEW_ROWS as f32 * PREVIEW_CELL + 28.0)
}

/// A secondary button with a back chevron.
fn back_button(v: &View, r: Rect, label: &str, hovered: bool) {
    let st = ui::State::new(false, hovered);
    ui::button_bg(v, r, 8.0, st, hex(ui::GOLD));
    let col = ui::label_color(st);
    let w = ui::caps_w(v, label, 15.0, 2.0) + 16.0;
    let x = r.center().x - w / 2.0;
    ui::chevron(v, x + 4.0, r.center().y, 11.0, -1.0, col);
    ui::caps(v, label, x + 16.0, ui::baseline(r.center().y, 15.0), 15.0, col, 2.0);
}

/// `c` nudged toward `toward`, keeping its alpha.
fn a_mix(c: Color, toward: Color, t: f32) -> Color {
    with_alpha(mix_colors(c, toward, t), c.a)
}


// ───────────── Online lobby ─────────────

fn online_button_rect(i: usize) -> Rect {
    Rect::new(CLASSIC_W / 2.0 - 160.0, 262.0 + i as f32 * 84.0, 320.0, 56.0)
}

/// The lobby's Create / Join buttons under `screen_pos`, if any.
pub fn online_hit(screen_pos: Vec2) -> Option<usize> {
    let p = View::fit_window(Vec2::ZERO, CLASSIC_W).to_virtual(screen_pos);
    (0..3).find(|&i| online_button_rect(i).contains(p))
}

/// The online lobby: create or join a room, type a code, or wait in a room with its code.
pub fn draw_online(lobby: &crate::app::Lobby, me: &str, progress: &Progress, host: bool) {
    use crate::app::Lobby;
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    scene::draw_menu_backdrop(&v, CLASSIC_W, Theme::classic(), progress.settings.scene);
    let cx = CLASSIC_W / 2.0;
    let title = match lobby {
        Lobby::Room { kind: crate::online::RoomKind::Party, .. } => "PARTY ROOM",
        Lobby::Room { kind: crate::online::RoomKind::Duel, .. } => "ONLINE 1V1",
        _ => "ONLINE",
    };
    ui::text_shadow_centered(&v, title, cx, 150.0, ui::TITLE, WHITE);
    ui::caps_centered(&v, &format!("PLAYING AS {me}"), cx, 186.0, ui::CAPS, hex(ui::TEXT3), 2.0);
    let mouse = View::fit_window(Vec2::ZERO, CLASSIC_W).to_virtual(Vec2::from(mouse_position()));
    match lobby {
        Lobby::Choose { focus } => {
            let options = [("CREATE 1V1", "a room for you and a friend"), ("CREATE PARTY", "free-for-all, up to 10 players"), ("JOIN ROOM", "type a friend's code")];
            for (i, (label, hint)) in options.iter().enumerate() {
                let r = online_button_rect(i);
                let st = ui::State::new(*focus == i, r.contains(mouse));
                ui::button(&v, r, label, 24.0, st, hex(ui::GOLD));
                v.text_centered(hint, cx, r.bottom() + 16.0, 14.0, hex(ui::TEXT3));
            }
            v.text_centered("Everyone gets the same pieces. In a party, each attack hits a random rival.", cx, 548.0, 16.0, hex(ui::TEXT2));
        }
        Lobby::Typing { code } => {
            let card = Rect::new(cx - 200.0, 260.0, 400.0, 220.0);
            ui::panel(&v, card, &ui::Panel::card());
            ui::caps_centered(&v, "ENTER THE ROOM CODE", cx, card.y + 40.0, ui::CAPS, hex(ui::TEXT2), 2.0);
            for i in 0..4 {
                let b = Rect::new(cx - 2.0 * 66.0 + i as f32 * 66.0 + 5.0, card.y + 70.0, 56.0, 72.0);
                ui::panel(&v, b, &ui::Panel::well());
                let ch = code.chars().nth(i).map(|c| c.to_string());
                if code.len() == i {
                    ui::stroke(&v, b, 7.0, 2.0, hex(ui::GOLD));
                }
                if let Some(ch) = ch {
                    v.text_centered(&ch, b.x + b.w / 2.0, b.y + 50.0, 44.0, WHITE);
                }
            }
            let hint = if code.len() == 4 { "Enter to join   ·   Esc back" } else { "type the 4 letters   ·   Esc back" };
            v.text_centered(hint, cx, card.bottom() - 24.0, 16.0, hex(ui::TEXT3));
        }
        Lobby::Room { code, kind, names, here, problem, .. } => {
            use crate::online::RoomKind;
            let names: Vec<&(String, String)> = names.iter().filter(|(id, _)| here.contains(id)).collect();
            let party = *kind == RoomKind::Party;
            let card = if party { Rect::new(cx - 260.0, 220.0, 520.0, 420.0) } else { Rect::new(cx - 220.0, 240.0, 440.0, 260.0) };
            ui::panel(&v, card, &ui::Panel::card());
            ui::caps_centered(&v, "ROOM CODE", cx, card.y + 38.0, ui::CAPS, hex(ui::TEXT2), 2.0);
            let spaced: String = code.chars().flat_map(|c| [c, ' ']).collect();
            ui::text_shadow_centered(&v, spaced.trim_end(), cx, card.y + 104.0, 64.0, hex(ui::GOLD));
            v.text_centered(if party { "share this code: up to 10 can join" } else { "share this code with your friend" }, cx, card.y + 136.0, 16.0, hex(ui::TEXT3));
            let joined = crate::net::status() == crate::net::Status::Joined;
            let (line, color, steady) = match (problem, *kind) {
                (Some(p), _) => (p.to_string(), hex(ui::PINK), true),
                _ if !joined => ("Connecting...".to_owned(), hex(ui::TEXT2), false),
                (_, RoomKind::Party) if names.is_empty() => ("Waiting for players...".to_owned(), hex(ui::TEXT2), false),
                (_, RoomKind::Party) if host => ("Press Enter to start".to_owned(), hex(ui::GREEN), false),
                (_, RoomKind::Party) => ("Waiting for the host to start...".to_owned(), hex(ui::TEXT2), false),
                (_, RoomKind::Duel) if !names.is_empty() => (format!("{} is here. Starting...", names[0].1), hex(ui::GREEN), true),
                (_, RoomKind::Duel) => ("Waiting for your opponent...".to_owned(), hex(ui::TEXT2), false),
                (_, RoomKind::Unknown) => ("Joining...".to_owned(), hex(ui::TEXT2), false),
            };
            if party {
                // Everyone here, two columns of five; the host wears the crown.
                let me_host = host;
                let mut list: Vec<(String, bool)> = vec![(me.to_owned(), true)];
                list.extend(names.iter().map(|(_, n)| (n.clone(), false)));
                ui::caps(&v, &format!("PLAYERS  {}/{}", list.len().min(crate::online::PARTY_MAX), crate::online::PARTY_MAX), card.x + 30.0, card.y + 170.0, 12.0, hex(ui::TEXT2), 2.0);
                for i in 0..crate::online::PARTY_MAX {
                    let (col, row) = (i / 5, i % 5);
                    let r = Rect::new(card.x + 24.0 + col as f32 * 240.0, card.y + 182.0 + row as f32 * 34.0, 232.0, 28.0);
                    match list.get(i) {
                        Some((name, mine)) => {
                            ui::panel(&v, r, &ui::Panel::well());
                            let c = if *mine { hex(ui::CYAN) } else { WHITE };
                            v.text_mixed(name, r.x + 12.0, ui::baseline(r.center().y, 17.0), 17.0, c);
                            if *mine && me_host {
                                ui::pill(&v, r.right() - 54.0, r.center().y, "HOST", 10.0, hex(ui::GOLD), ui::Pill::Soft);
                            } else if *mine {
                                ui::caps(&v, "YOU", r.right() - 40.0, ui::baseline(r.center().y, 11.0), 11.0, hex(ui::TEXT3), 1.5);
                            }
                        }
                        None => ui::stroke(&v, r, 6.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.06)),
                    }
                }
            }
            // A quiet pulse while waiting.
            let pulse = 0.6 + 0.4 * (get_time() as f32 * 3.0).sin().abs();
            let ly = if party { card.bottom() - 52.0 } else { card.y + 194.0 };
            v.text_mixed(&line, cx - v.measure_mixed(&line, 20.0) / 2.0, ly, 20.0, Color { a: if steady { 1.0 } else { pulse }, ..color });
            v.text_centered("Esc leave", cx, card.bottom() - 20.0, 15.0, hex(ui::TEXT3));
        }
    }
}
