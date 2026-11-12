//! Drawing. Everything is laid out in a fixed virtual canvas and scaled to fit
//! the window (letterboxed), so resizing just works.

use macroquad::prelude::*;

use crate::board::{Cell, HIDDEN_ROWS, TOTAL_ROWS, VISIBLE_ROWS, WIDTH};
use crate::battle::{self, Ability, Battle, Defeat, Phase};
use crate::monster_art::{self, Pixel};
use crate::fx::{Fx, Motion};
use crate::game::Game;
use crate::piece::PieceKind;

/// Virtual canvas width for Classic and the menu; Battler adds the monster panel.
const CLASSIC_W: f32 = 760.0;
const BATTLE_W: f32 = 1100.0;
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

pub fn cell_color(cell: Cell) -> Color {
    match cell {
        Cell::Piece(kind) => piece_color(kind),
        Cell::Garbage => hex(0x5a6275),
    }
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

    fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        let s = self.scale;
        draw_ellipse(self.ox + cx * s, self.oy + cy * s, rx * s, ry * s, 0.0, c);
    }

    fn circle(&self, cx: f32, cy: f32, r: f32, c: Color) {
        let s = self.scale;
        draw_circle(self.ox + cx * s, self.oy + cy * s, r * s, c);
    }

    fn circle_lines(&self, cx: f32, cy: f32, r: f32, thickness: f32, c: Color) {
        let s = self.scale;
        draw_circle_lines(self.ox + cx * s, self.oy + cy * s, r * s, thickness * s, c);
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

pub fn draw(game: &Game, paused: bool, fx: &Fx, sound_on: bool, battle: Option<&Battle>) {
    clear_background(hex(0x0b0d14));
    let width = if battle.is_some() { BATTLE_W } else { CLASSIC_W };
    let v = View::fit_window(fx.shake_offset(), width);
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
            if let Some(cell) = board.get(x, y) {
                let (px, py) = cell_pos(x as i32, y as i32);
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
        let pulse = if danger { 0.75 + 0.25 * (get_time() as f32 * 12.0).sin() } else { 1.0 };
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
    let motion_y = BOARD_Y + 420.0 + controls.len() as f32 * 22.0 + 8.0;
    v.text("M  motion:", LEFT_X, motion_y, 18.0, hex(0x5d6680));
    v.text(fx.motion().label(), LEFT_X + 84.0, motion_y, 18.0, hex(0x9aa3b8));
    v.text("N  sound:", LEFT_X, motion_y + 22.0, 18.0, hex(0x5d6680));
    v.text(if sound_on { "on" } else { "off" }, LEFT_X + 84.0, motion_y + 22.0, 18.0, hex(0x9aa3b8));

    // Next queue on the right.
    let slot = 80.0;
    let queue_h = slot * game.next.len() as f32 + 10.0;
    panel(&v, "NEXT", RIGHT_X, BOARD_Y, PANEL_W, queue_h);
    if battle.is_some_and(Battle::next_hidden) {
        // Fog ability: the queue is hidden behind drifting murk.
        let t = get_time() as f32;
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

    if let Some(b) = battle {
        draw_battle_panel(&v, b);
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

    fx.draw_top(&v);

    // Overlays.
    let field_cx = BOARD_X + BOARD_W / 2.0;
    let overlay = |title: &str, subtitle: &str| {
        v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.7));
        v.text_centered(title, field_cx, BOARD_Y + BOARD_H / 2.0 - 10.0, 52.0, WHITE);
        v.text_centered(subtitle, field_cx, BOARD_Y + BOARD_H / 2.0 + 30.0, 22.0, hex(0x9aa3b8));
    };
    match battle {
        Some(b) if b.over() || game.game_over => draw_battle_results(&v, b),
        _ if game.game_over => overlay("GAME OVER", "Enter retry   ·   Esc menu"),
        _ if paused => overlay("PAUSED", "Esc resume   ·   Q menu"),
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
fn draw_monster(v: &View, b: &Battle, cx: f32, feet_y: f32) {
    use monster_art::{H, W};
    let t = get_time() as f32;
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
    let recent_hit = b.hits.last().map_or(f32::MAX, |&(_, age)| age);
    let flash = (1.0 - recent_hit / 0.12).max(0.0);
    let knock = (1.0 - recent_hit / 0.25).max(0.0);
    // Winding up for a garbage attack: tint red and tremble.
    let warn = if fighting && b.attack_timer < ATTACK_WARN { 1.0 - b.attack_timer / ATTACK_WARN } else { 0.0 };
    let warn_pulse = 0.6 + 0.4 * (t * 18.0).sin();
    // Just attacked (or just arrived): a quick lunge.
    let since_attack = m.attack_every - b.attack_timer;
    let lunge = if fighting { (1.0 - since_attack / 0.25).max(0.0) } else { 0.0 };

    let breathe = (t * 3.0).sin();
    let sx = 1.0 - 0.03 * breathe + 0.12 * lunge;
    let sy = 1.0 + 0.035 * breathe + 0.08 * lunge;
    let x0 = cx + warn * (t * 55.0).sin() * 3.0 + knock * knock * 12.0;
    let blink = (t % 3.7) < 0.12 || recent_hit < 0.2;

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

    // Damage numbers float up from the monster.
    for &(damage, age) in &b.hits {
        let k = age / battle::HIT_POPUP_TIME;
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

fn draw_battle_panel(v: &View, b: &Battle) {
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
    draw_monster(v, b, cx, feet_y);
    if let Some(ability) = m.ability {
        let active = b.ability_in_effect();
        let pulse = 0.6 + 0.4 * (get_time() as f32 * 8.0).sin().abs();
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
    let pulse = if urgent { 0.6 + 0.4 * (get_time() as f32 * 10.0).sin().abs() } else { 1.0 };
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

fn draw_battle_results(v: &View, b: &Battle) {
    let cx = BOARD_X + BOARD_W / 2.0;
    v.rect(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, Color::new(0.0, 0.0, 0.0, 0.78));
    let title = match b.phase {
        Phase::Over(Defeat::Timeout) => "TIME UP",
        _ => "TOPPED OUT",
    };
    v.text_centered(title, cx, BOARD_Y + 200.0, 52.0, hex(0xff4f6d));
    let lines = [
        ("monsters defeated", b.defeated.to_string()),
        ("total damage", b.total_damage.to_string()),
        ("fell to", format!("{} (lv {})", b.monster.title(), b.monster.tier)),
    ];
    for (i, (label, value)) in lines.iter().enumerate() {
        let y = BOARD_Y + 270.0 + i as f32 * 62.0;
        v.text_centered(label, cx, y, 20.0, hex(0x9aa3b8));
        v.text_centered(value, cx, y + 30.0, 30.0, WHITE);
    }
    v.text_centered("Enter retry   ·   Esc menu", cx, BOARD_Y + 500.0, 22.0, hex(0x9aa3b8));
}

pub struct MenuItem {
    pub title: &'static str,
    pub blurb: &'static str,
}

// Menu layout, shared by drawing and mouse hit-testing (virtual coordinates).
const MENU_ITEM_W: f32 = 420.0;
const MENU_ITEM_H: f32 = 64.0;
const MENU_ITEM_GAP: f32 = 18.0;
const MENU_TOP: f32 = 270.0;
const TOGGLE_W: f32 = 200.0;
const TOGGLE_H: f32 = 40.0;
const TOGGLE_GAP: f32 = 20.0;

/// Something clickable on the main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuHit {
    Item(usize),
    Motion,
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

/// The blurb sits under the items; the toggles sit under the blurb.
fn menu_toggle_rect(items: usize, which: MenuHit) -> Rect {
    let y = MENU_TOP + items as f32 * (MENU_ITEM_H + MENU_ITEM_GAP) + 60.0;
    let left = CLASSIC_W / 2.0 - TOGGLE_W - TOGGLE_GAP / 2.0;
    let x = if which == MenuHit::Motion { left } else { left + TOGGLE_W + TOGGLE_GAP };
    Rect::new(x, y, TOGGLE_W, TOGGLE_H)
}

impl View {
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
        .chain([MenuHit::Motion, MenuHit::Sound])
        .find(|&hit| {
            let r = match hit {
                MenuHit::Item(i) => menu_item_rect(i),
                toggle => menu_toggle_rect(items, toggle),
            };
            r.contains(p)
        })
}

pub fn draw_menu(items: &[MenuItem], selected: usize, hover: Option<MenuHit>, sound_on: bool, motion: Motion) {
    clear_background(hex(0x0b0d14));
    let v = View::fit_window(Vec2::ZERO, CLASSIC_W);
    let cx = CLASSIC_W / 2.0;

    // A few static blocks as backdrop, one of each piece colour.
    for (i, kind) in PieceKind::ALL.iter().enumerate() {
        let x = 60.0 + i as f32 * 95.0;
        let y = 640.0 + (i % 2) as f32 * 30.0;
        draw_mini_piece(&v, *kind, x, y, with_alpha(piece_color(*kind), 0.25));
    }

    v.text_centered("BLOCK", cx - 95.0, 170.0, 76.0, hex(0xffd23f));
    v.text_centered("BRAWL", cx + 105.0, 170.0, 76.0, hex(0xff4f6d));
    v.text_centered("a falling-block battler", cx, 210.0, 22.0, hex(0x5d6680));

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
        "Up/Down or mouse to choose  ·  Enter or click to play  ·  M / N toggle",
        cx,
        712.0,
        18.0,
        hex(0x5d6680),
    );
    v.text_centered("made by xSolstice1  ·  github.com/xSolstice1", cx, 742.0, 16.0, hex(0x3d4458));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_targets_never_overlap_and_fit_the_canvas() {
        for items in 1..=4 {
            let mut rects: Vec<Rect> = (0..items).map(menu_item_rect).collect();
            rects.push(menu_toggle_rect(items, MenuHit::Motion));
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
