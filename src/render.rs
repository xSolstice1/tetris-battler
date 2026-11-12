//! Drawing. Everything is laid out in a fixed virtual canvas and scaled to fit
//! the window (letterboxed), so resizing just works.

use macroquad::prelude::*;

use crate::board::{HIDDEN_ROWS, TOTAL_ROWS, VISIBLE_ROWS, WIDTH};
use crate::fx::Fx;
use crate::game::Game;
use crate::piece::PieceKind;

const VIRTUAL_W: f32 = 760.0;
const VIRTUAL_H: f32 = 760.0;
pub const CELL: f32 = 32.0;
const MINI: f32 = 20.0;
pub const BOARD_X: f32 = 220.0;
pub const BOARD_Y: f32 = 80.0;
pub const BOARD_W: f32 = WIDTH as f32 * CELL;
const BOARD_H: f32 = VISIBLE_ROWS as f32 * CELL;
const LEFT_X: f32 = 40.0;
const RIGHT_X: f32 = BOARD_X + BOARD_W + 30.0;
const PANEL_W: f32 = 150.0;

pub fn hex(rgb: u32) -> Color {
    Color::from_rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn shade(c: Color, f: f32) -> Color {
    Color::new(c.r * f, c.g * f, c.b * f, c.a)
}

pub fn piece_color(kind: PieceKind) -> Color {
    match kind {
        PieceKind::I => hex(0x2ee6d6),
        PieceKind::O => hex(0xffd23f),
        PieceKind::T => hex(0xc061ff),
        PieceKind::S => hex(0x5cdb5c),
        PieceKind::Z => hex(0xff4f6d),
        PieceKind::J => hex(0x4f8bff),
        PieceKind::L => hex(0xff9a3c),
    }
}

/// Maps virtual coordinates to screen pixels, plus a shake offset (virtual px).
pub struct View {
    scale: f32,
    ox: f32,
    oy: f32,
}

impl View {
    fn fit_window(shake: Vec2) -> Self {
        let scale = (screen_width() / VIRTUAL_W).min(screen_height() / VIRTUAL_H);
        View {
            scale,
            ox: (screen_width() - VIRTUAL_W * scale) / 2.0 + shake.x * scale,
            oy: (screen_height() - VIRTUAL_H * scale) / 2.0 + shake.y * scale,
        }
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        let s = self.scale;
        draw_rectangle(self.ox + x * s, self.oy + y * s, w * s, h * s, c);
    }

    fn outline(&self, x: f32, y: f32, w: f32, h: f32, thickness: f32, c: Color) {
        let s = self.scale;
        draw_rectangle_lines(self.ox + x * s, self.oy + y * s, w * s, h * s, thickness * s, c);
    }

    fn text(&self, t: &str, x: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        draw_text(t, self.ox + x * s, self.oy + y * s, size * s, c);
    }

    pub fn text_centered(&self, t: &str, cx: f32, y: f32, size: f32, c: Color) {
        let s = self.scale;
        let dims = measure_text(t, None, (size * s).max(1.0) as u16, 1.0);
        draw_text(t, self.ox + cx * s - dims.width / 2.0, self.oy + y * s, size * s, c);
    }

    pub fn block(&self, x: f32, y: f32, size: f32, c: Color) {
        self.rect(x, y, size, size, shade(c, 0.55));
        self.rect(x + 2.0, y + 2.0, size - 4.0, size - 4.0, c);
        self.rect(x + 2.0, y + 2.0, size - 4.0, (size - 4.0) * 0.22, shade(c, 1.25));
    }
}

/// Virtual position of board cell (x, y). Row `HIDDEN_ROWS` is the top visible row.
pub fn cell_pos(x: i32, y: i32) -> (f32, f32) {
    (
        BOARD_X + x as f32 * CELL,
        BOARD_Y + (y - HIDDEN_ROWS as i32) as f32 * CELL,
    )
}

/// The spawn row just above the field is drawn too, so freshly spawned pieces are visible.
fn drawable_row(y: i32) -> bool {
    y >= HIDDEN_ROWS as i32 - 1
}

fn draw_mini_piece(v: &View, kind: PieceKind, cx: f32, cy: f32, color: Color) {
    let cells = kind.cells(0);
    let (min_x, max_x) = cells.iter().fold((i32::MAX, i32::MIN), |(lo, hi), c| (lo.min(c.0), hi.max(c.0)));
    let (min_y, max_y) = cells.iter().fold((i32::MAX, i32::MIN), |(lo, hi), c| (lo.min(c.1), hi.max(c.1)));
    let w = (max_x - min_x + 1) as f32 * MINI;
    let h = (max_y - min_y + 1) as f32 * MINI;
    for (x, y) in cells {
        v.block(
            cx - w / 2.0 + (x - min_x) as f32 * MINI,
            cy - h / 2.0 + (y - min_y) as f32 * MINI,
            MINI,
            color,
        );
    }
}

fn panel(v: &View, label: &str, x: f32, y: f32, w: f32, h: f32) {
    v.text(label, x, y - 8.0, 22.0, hex(0x9aa3b8));
    v.rect(x, y, w, h, hex(0x141824));
    v.outline(x, y, w, h, 2.0, hex(0x262c3d));
}

pub fn draw(game: &Game, paused: bool, fx: &Fx) {
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(fx.shake_offset());
    let field_bg = hex(0x10131c);

    // Field frame (flashes with the last clear's colour) and background.
    v.rect(
        BOARD_X - 4.0,
        BOARD_Y - 4.0,
        BOARD_W + 8.0,
        BOARD_H + 8.0,
        fx.border_color(hex(0x262c3d)),
    );
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, field_bg);
    let grid = hex(0x181c28);
    for x in 1..WIDTH {
        v.rect(BOARD_X + x as f32 * CELL, BOARD_Y, 1.0, BOARD_H, grid);
    }
    for y in 1..VISIBLE_ROWS {
        v.rect(BOARD_X, BOARD_Y + y as f32 * CELL, BOARD_W, 1.0, grid);
    }

    // Settled stack, offset while it collapses after a clear. During a hard drop's
    // flight, show the field as it was before the lock.
    let board = fx.frozen_board().unwrap_or(&game.board);
    for y in HIDDEN_ROWS - 1..TOTAL_ROWS {
        let offset = fx.row_offset(y);
        for x in 0..WIDTH {
            if let Some(kind) = board.get(x, y) {
                let (px, py) = cell_pos(x as i32, y as i32);
                let c = if game.game_over { hex(0x4a5064) } else { piece_color(kind) };
                v.block(px, py + offset, CELL, c);
            }
        }
    }

    fx.draw_field(&v);

    if !game.game_over {
        let a = game.active;
        let color = piece_color(a.kind);
        let ghost_y = game.ghost_y();
        for (cx, cy) in a.kind.cells(a.rot) {
            if drawable_row(ghost_y + cy) {
                let (px, py) = cell_pos(a.x + cx, ghost_y + cy);
                v.rect(px + 2.0, py + 2.0, CELL - 4.0, CELL - 4.0, with_alpha(color, 0.12));
                v.outline(px + 2.0, py + 2.0, CELL - 4.0, CELL - 4.0, 2.0, with_alpha(color, 0.5));
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

    // Title + hold + stats on the left.
    v.text("BLOCK", LEFT_X, 40.0, 34.0, hex(0xffd23f));
    v.text("BRAWL", LEFT_X + 88.0, 40.0, 34.0, hex(0xff4f6d));

    panel(&v, "HOLD", LEFT_X, BOARD_Y, PANEL_W, 100.0);
    if let Some(kind) = game.hold {
        let c = if game.hold_used { hex(0x4a5064) } else { piece_color(kind) };
        draw_mini_piece(&v, kind, LEFT_X + PANEL_W / 2.0, BOARD_Y + 50.0, c);
    }

    let stats = [
        ("SCORE", game.score.to_string()),
        ("LINES", game.lines.to_string()),
        ("LEVEL", game.level.to_string()),
    ];
    for (i, (label, value)) in stats.iter().enumerate() {
        let y = BOARD_Y + 160.0 + i as f32 * 70.0;
        v.text(label, LEFT_X, y, 22.0, hex(0x9aa3b8));
        v.text(value, LEFT_X, y + 32.0, 34.0, WHITE);
    }

    let controls = [
        "Left/Right  move",
        "Down  soft drop",
        "Space  hard drop",
        "Up / X  rotate cw",
        "Z / Ctrl  rotate ccw",
        "C / Shift  hold",
        "P / Esc  pause",
    ];
    for (i, line) in controls.iter().enumerate() {
        v.text(line, LEFT_X, BOARD_Y + 420.0 + i as f32 * 22.0, 18.0, hex(0x5d6680));
    }

    // Next queue on the right.
    let slot = 80.0;
    panel(&v, "NEXT", RIGHT_X, BOARD_Y, PANEL_W, slot * game.next.len() as f32 + 10.0);
    for (i, &kind) in game.next.iter().enumerate() {
        draw_mini_piece(
            &v,
            kind,
            RIGHT_X + PANEL_W / 2.0,
            BOARD_Y + 45.0 + i as f32 * slot,
            piece_color(kind),
        );
    }

    fx.draw_top(&v);

    // Overlays.
    let overlay = |title: &str, subtitle: &str| {
        v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.7));
        v.text_centered(title, BOARD_X + BOARD_W / 2.0, BOARD_Y + BOARD_H / 2.0 - 10.0, 52.0, WHITE);
        v.text_centered(subtitle, BOARD_X + BOARD_W / 2.0, BOARD_Y + BOARD_H / 2.0 + 30.0, 22.0, hex(0x9aa3b8));
    };
    if game.game_over {
        overlay("GAME OVER", "Enter / R to restart");
    } else if paused {
        overlay("PAUSED", "P / Esc to resume");
    }
}
