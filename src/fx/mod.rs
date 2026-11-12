//! Visual effects driven by game events. Purely presentational: nothing here
//! feeds back into game logic, and effects use macroquad's RNG, never the
//! game's seeded bag.

mod callouts;
mod particles;
mod shake;
mod tween;

use std::f32::consts::TAU;

use macroquad::prelude::*;
use macroquad::rand::gen_range;

use crate::board::{Board, Row, TOTAL_ROWS, WIDTH};
use crate::game::{Active, Event, LineClear};
use crate::render::{self, BOARD_W, BOARD_X, BOARD_Y, CELL, View};
use callouts::Callouts;
use particles::{Particle, Particles};
use shake::Shake;
use tween::{out_back_soft, out_cubic};

/// Cleared cells burst from the centre outward, this long per column of distance.
const SWEEP_PER_COLUMN: f32 = 0.022;
/// How fast a clearing row turns white.
const FLASH_IN: f32 = 0.05;
const BEAM_TIME: f32 = 0.3;
const CLEAR_ANIM_TIME: f32 = 0.5;
/// Rows above a clear hold still while the cells burst, then drop.
const COLLAPSE_DELAY: f32 = 0.1;
const COLLAPSE_TIME: f32 = 0.2;
const TRAIL_TIME: f32 = 0.22;
const LOCK_FLASH_TIME: f32 = 0.16;
const BORDER_FLASH_TIME: f32 = 0.45;
/// Hard-drop flight time: base + per row travelled, capped. Short enough to never
/// feel like input lag (logic has already locked the piece), long enough to see.
const DROP_BASE_TIME: f32 = 0.05;
const DROP_TIME_PER_ROW: f32 = 0.0035;
const DROP_MAX_TIME: f32 = 0.12;
/// The flight never takes more than this share of the player's recent time between
/// hard drops, so fast players get a near-instant snap (the board on screen is
/// stale while a piece is in flight).
const DROP_SHARE_OF_INTERVAL: f32 = 0.25;
/// Flights shorter than about a frame aren't worth showing; land immediately.
const DROP_MIN_TIME: f32 = 0.02;
/// Weight of the newest interval in the running average (reacts within ~5 drops).
const DROP_INTERVAL_SMOOTHING: f32 = 0.4;
/// Pauses longer than this count as this, so one break doesn't hide the flight for long.
const DROP_INTERVAL_CAP: f32 = 1.0;
/// Motion-blur copies trail this far behind in drop progress.
const DROP_BLUR_STEP: f32 = 0.12;
/// How quickly the drawn active piece catches up with its logical row (1/s).
/// ~30 ms time constant: glides between rows without feeling floaty.
const ACTIVE_FOLLOW_RATE: f32 = 35.0;
/// The drawn active piece never trails its logical row by more than this (rows).
const ACTIVE_MAX_LAG: f32 = 0.4;

/// How much the effects may move things away from the true board state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    /// Hard-drop flight, stack collapse and active-piece glide.
    #[default]
    Full,
    /// Every frame shows the real board: no flight, collapse or glide.
    /// Particles, shake and callouts stay, as they don't misrepresent the field.
    Competitive,
}

impl Motion {
    pub fn toggled(self) -> Self {
        match self {
            Motion::Full => Motion::Competitive,
            Motion::Competitive => Motion::Full,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Motion::Full => "full",
            Motion::Competitive => "competitive",
        }
    }
}

/// A hard-dropped piece still travelling down on screen. Events the game emitted
/// after the drop (lock, clears, level up, ...) wait in `queued` until it lands,
/// and `board` is the field as it was before the lock.
struct Drop {
    piece: Active,
    distance: i32,
    board: Board,
    color: Color,
    t: f32,
    duration: f32,
    queued: Vec<Event>,
}

impl Drop {
    /// Ease-in: accelerates into the impact.
    fn progress_at(&self, t: f32) -> f32 {
        let p = (t / self.duration).clamp(0.0, 1.0);
        p * p
    }

    /// Rows travelled so far (fractional).
    fn rows_fallen_at(&self, t: f32) -> f32 {
        self.progress_at(t) * self.distance as f32
    }
}

/// Per-column trail extents: (column, top row before the drop, top row after landing).
fn trail_columns(piece: &Active, distance: i32) -> Vec<(i32, i32, i32)> {
    let mut columns: Vec<(i32, i32, i32)> = Vec::new();
    for (x, y) in piece.cells() {
        match columns.iter_mut().find(|c| c.0 == x) {
            Some(c) => {
                c.1 = c.1.min(y);
                c.2 = c.2.min(y + distance);
            }
            None => columns.push((x, y, y + distance)),
        }
    }
    columns
}

/// Vertical light streak from `y0` down to `y1` (virtual px), brighter and wider at the bottom.
fn draw_streak(v: &View, x: i32, y0: f32, y1: f32, color: Color, fade: f32) {
    if y1 <= y0 {
        return;
    }
    const SLICES: usize = 10;
    let (px, _) = render::cell_pos(x, 0);
    let h = (y1 - y0) / SLICES as f32;
    let tint = mix(color, WHITE, 0.4);
    for i in 0..SLICES {
        let f = (i + 1) as f32 / SLICES as f32;
        let w = (CELL - 10.0) * (0.4 + 0.6 * f);
        v.rect(
            px + (CELL - w) / 2.0,
            y0 + h * i as f32,
            w,
            h + 0.5,
            Color { a: f * 0.45 * fade, ..tint },
        );
    }
}

struct ClearingRow {
    y: usize,
    cells: Row,
    burst: [bool; WIDTH],
}

struct ClearAnim {
    rows: Vec<ClearingRow>,
    big: bool,
    t: f32,
}

struct Collapse {
    /// Rows each post-clear row fell, indexed by post-clear row.
    shift: [u8; TOTAL_ROWS],
    t: f32,
}

struct Trail {
    /// (column, top row before the drop, top row after landing)
    columns: Vec<(i32, i32, i32)>,
    color: Color,
    t: f32,
}

struct LockFlash {
    cells: [(i32, i32); 4],
    t: f32,
}

pub struct Fx {
    particles: Particles,
    shake: Shake,
    callouts: Callouts,
    clears: Vec<ClearAnim>,
    collapse: Option<Collapse>,
    trails: Vec<Trail>,
    flashes: Vec<LockFlash>,
    border: Option<(Color, f32)>,
    drop: Option<Drop>,
    /// Drawn row of the active piece, easing toward the logical row.
    active_y: Option<f32>,
    /// Set when a new piece appears, so it doesn't glide in from the old one.
    snap_active: bool,
    /// Effects clock when the last hard drop happened, and the running average gap.
    last_drop_at: Option<f32>,
    drop_interval: f32,
    motion: Motion,
    presented: Vec<Event>,
    time: f32,
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// For each post-clear row index, how many rows it fell.
fn collapse_shifts(cleared: &[usize]) -> [u8; TOTAL_ROWS] {
    // Rows spawned empty at the top fell the full amount; they draw nothing anyway.
    let mut shift = [cleared.len() as u8; TOTAL_ROWS];
    for old in (0..TOTAL_ROWS).filter(|y| !cleared.contains(y)) {
        let below = cleared.iter().filter(|&&r| r > old).count();
        shift[old + below] = below as u8;
    }
    shift
}

fn burst_cell(particles: &mut Particles, x: i32, y: i32, color: Color, big: bool) {
    let (px, py) = render::cell_pos(x, y);
    let centre = vec2(px + CELL / 2.0, py + CELL / 2.0);
    let (count, max_speed, max_size) = if big { (9, 520.0, 9.0) } else { (5, 380.0, 7.0) };
    // Push debris away from the middle so the burst reads as splitting open.
    let outward = (x as f32 - (WIDTH as f32 - 1.0) / 2.0) * 18.0;
    for _ in 0..count {
        let angle = gen_range(0.0, TAU);
        let speed = gen_range(120.0, max_speed);
        let vel = vec2(angle.cos() * speed + outward, angle.sin() * speed - 180.0);
        let jitter = vec2(gen_range(-8.0, 8.0), gen_range(-8.0, 8.0));
        particles.spawn(
            Particle::new(
                centre + jitter,
                vel,
                gen_range(0.35, 0.8),
                gen_range(3.0, max_size),
                mix(color, WHITE, gen_range(0.0, 0.5)),
            )
            .gravity(1100.0)
            .drag(1.5),
        );
    }
    for _ in 0..2 {
        let angle = gen_range(0.0, TAU);
        let speed = gen_range(400.0, 700.0);
        particles.spawn(
            Particle::new(centre, vec2(angle.cos(), angle.sin()) * speed, 0.25, 3.0, WHITE).drag(4.0),
        );
    }
}

impl Default for Fx {
    fn default() -> Self {
        Fx::new(Motion::default())
    }
}

impl Fx {
    pub fn new(motion: Motion) -> Self {
        Fx {
            particles: Particles::default(),
            shake: Shake::default(),
            callouts: Callouts::default(),
            clears: Vec::new(),
            collapse: None,
            trails: Vec::new(),
            flashes: Vec::new(),
            border: None,
            drop: None,
            active_y: None,
            snap_active: false,
            last_drop_at: None,
            // Assume a relaxed pace until we've seen some drops.
            drop_interval: DROP_INTERVAL_CAP,
            motion,
            presented: Vec::new(),
            time: 0.0,
        }
    }

    pub fn motion(&self) -> Motion {
        self.motion
    }

    /// Switching to competitive immediately settles anything that's off the true board.
    pub fn set_motion(&mut self, motion: Motion) {
        self.motion = motion;
        if motion == Motion::Competitive {
            self.land();
            self.collapse = None;
            self.burst_due_cells(true);
            self.snap_active = true;
        }
    }

    /// Flight time for a drop of `distance` rows, given how fast the player is dropping.
    fn drop_duration(&self, distance: i32) -> f32 {
        if self.motion == Motion::Competitive {
            return 0.0;
        }
        let by_distance = (DROP_BASE_TIME + distance as f32 * DROP_TIME_PER_ROW).min(DROP_MAX_TIME);
        by_distance.min(DROP_SHARE_OF_INTERVAL * self.drop_interval)
    }

    fn record_drop_time(&mut self) {
        if let Some(last) = self.last_drop_at {
            let gap = (self.time - last).min(DROP_INTERVAL_CAP);
            self.drop_interval += (gap - self.drop_interval) * DROP_INTERVAL_SMOOTHING;
        }
        self.last_drop_at = Some(self.time);
    }

    pub fn handle(&mut self, event: &Event) {
        // The next piece is already live while the last one is still falling on
        // screen: its events (and their sounds) must never wait in the drop queue.
        let live_piece = matches!(
            event,
            Event::Moved | Event::Rotated | Event::SoftDropped | Event::Held | Event::Spawned
        );
        if matches!(event, Event::Spawned | Event::Held) {
            self.snap_active = true;
        }

        if let Event::HardDropped {
            piece,
            distance,
            board,
        } = event
        {
            self.land(); // a new drop finishes any still in flight
            self.record_drop_time();
            if *distance > 0 {
                self.drop = Some(Drop {
                    piece: *piece,
                    distance: *distance,
                    board: board.clone(),
                    color: render::piece_color(piece.kind),
                    t: 0.0,
                    duration: self.drop_duration(*distance),
                    queued: Vec::new(),
                });
                // Too quick to see: skip the flight but keep the impact (dust, shake, trail).
                if self.drop.as_ref().is_some_and(|d| d.duration < DROP_MIN_TIME) {
                    self.land();
                }
            } else {
                // Already on the floor: nothing to animate, but it still "lands".
                self.presented.push(event.clone());
            }
            return;
        }

        match &mut self.drop {
            Some(drop) if !live_piece => drop.queued.push(event.clone()),
            _ => self.apply(event),
        }
    }

    /// Events in the order and at the moment they were shown on screen; a hard
    /// drop appears when its piece lands. Drain once per frame (audio uses this).
    pub fn take_presented(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.presented)
    }

    /// Ends an in-flight hard drop: impact effects, then everything that was queued.
    fn land(&mut self) {
        let Some(drop) = self.drop.take() else {
            return;
        };
        self.impact(&drop);
        self.presented.push(Event::HardDropped {
            piece: drop.piece,
            distance: drop.distance,
            board: drop.board.clone(),
        });
        for event in &drop.queued {
            self.apply(event);
        }
    }

    fn apply(&mut self, event: &Event) {
        self.presented.push(event.clone());
        match event {
            Event::Locked { piece } => {
                // A new lock must never be drawn on rows that are still sliding:
                // finish any collapse (and its pending cell bursts) right now.
                if self.collapse.take().is_some() {
                    self.burst_due_cells(true);
                }
                self.flashes.push(LockFlash {
                    cells: piece.cells(),
                    t: 0.0,
                });
            }
            Event::LinesCleared(clear) => self.line_clear(clear),
            Event::LevelUp { level } => {
                let gold = render::hex(0xffd23f);
                self.callouts.push(format!("LEVEL {level}"), gold, 30.0);
                self.border = Some((gold, BORDER_FLASH_TIME));
            }
            Event::GameOver => self.shake.add(0.6),
            Event::HardDropped { .. }
            | Event::Moved
            | Event::Rotated
            | Event::SoftDropped
            | Event::Held
            | Event::Spawned => {}
        }
    }

    fn impact(&mut self, drop: &Drop) {
        let (piece, distance, color) = (drop.piece, drop.distance, drop.color);
        let cells = piece.cells();
        self.trails.push(Trail {
            columns: trail_columns(&piece, distance),
            color,
            t: 0.0,
        });

        // Dust puffs from under each cell that hit something.
        let dust = mix(color, WHITE, 0.6);
        for &(x, y) in cells.iter().filter(|&&(x, y)| !cells.contains(&(x, y + 1))) {
            let (px, py) = render::cell_pos(x, y + distance + 1);
            for _ in 0..4 {
                self.particles.spawn(
                    Particle::new(
                        vec2(px + gen_range(0.0, CELL), py),
                        vec2(gen_range(-220.0, 220.0), gen_range(-160.0, -40.0)),
                        gen_range(0.25, 0.5),
                        gen_range(2.0, 5.0),
                        Color { a: 0.8, ..dust },
                    )
                    .gravity(500.0)
                    .drag(3.0),
                );
            }
        }

        let d = distance as f32;
        self.shake.add((0.08 + d * 0.006).min(0.25));
        self.shake.kick((2.0 + d * 0.35).min(9.0));
    }

    fn line_clear(&mut self, c: &LineClear) {
        let lines = c.lines();
        let purple = render::hex(0xc061ff);
        let (name, color, size) = match (c.t_spin, lines) {
            (true, 0) => ("T-SPIN", purple, 36.0),
            (true, 1) => ("T-SPIN SINGLE", purple, 38.0),
            (true, 2) => ("T-SPIN DOUBLE", purple, 42.0),
            (true, _) => ("T-SPIN TRIPLE", purple, 46.0),
            (false, 1) => ("SINGLE", render::hex(0xc8d0e0), 30.0),
            (false, 2) => ("DOUBLE", render::hex(0x5ad1ff), 34.0),
            (false, 3) => ("TRIPLE", render::hex(0x7dff8a), 38.0),
            (false, _) => ("QUAD", render::hex(0xffd23f), 48.0),
        };

        // Callouts stack newest-on-top, so push the headline last.
        if c.perfect_clear {
            self.callouts.push_rainbow("PERFECT CLEAR", 34.0);
        }
        if c.combo > 0 {
            let size = 24.0 + 3.0 * c.combo.min(8) as f32;
            self.callouts.push(format!("COMBO {}", c.combo), render::hex(0xff9a3c), size);
        }
        let headline = if c.back_to_back { format!("B2B {name}") } else { name.to_owned() };
        self.callouts.push(headline, color, size);

        let mut trauma = match lines {
            0 | 1 => 0.08,
            2 => 0.14,
            3 => 0.22,
            _ => 0.45,
        };
        if c.t_spin {
            trauma += 0.15;
        }
        if c.back_to_back {
            trauma += 0.05;
        }
        if c.perfect_clear {
            trauma += 0.3;
        }
        self.shake.add(trauma);
        self.border = Some((color, BORDER_FLASH_TIME));

        if lines > 0 {
            self.clears.push(ClearAnim {
                rows: c
                    .rows
                    .iter()
                    .zip(&c.snapshot)
                    .map(|(&y, &cells)| ClearingRow {
                        y,
                        cells,
                        burst: [false; WIDTH],
                    })
                    .collect(),
                big: lines >= 4 || c.t_spin,
                t: 0.0,
            });
            match self.motion {
                Motion::Full => {
                    self.collapse = Some(Collapse {
                        shift: collapse_shifts(&c.rows),
                        t: 0.0,
                    })
                }
                // Rows are already where they belong; don't draw old cells over them.
                Motion::Competitive => self.burst_due_cells(true),
            }
        }
    }

    /// Bursts clearing cells whose sweep delay has passed, or all of them if `force`.
    fn burst_due_cells(&mut self, force: bool) {
        for anim in &mut self.clears {
            for row in &mut anim.rows {
                for x in 0..WIDTH {
                    let delay = (x as f32 - (WIDTH as f32 - 1.0) / 2.0).abs().floor() * SWEEP_PER_COLUMN;
                    if !row.burst[x] && (force || anim.t >= delay) {
                        row.burst[x] = true;
                        if let Some(kind) = row.cells[x] {
                            let color = render::piece_color(kind);
                            burst_cell(&mut self.particles, x as i32, row.y as i32, color, anim.big);
                        }
                    }
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;

        if let Some(drop) = &mut self.drop {
            drop.t += dt;
            if drop.t >= drop.duration {
                self.land();
            }
        }

        for anim in &mut self.clears {
            anim.t += dt;
        }
        self.burst_due_cells(false);
        self.clears.retain(|a| a.t < CLEAR_ANIM_TIME);

        if let Some(c) = &mut self.collapse {
            c.t += dt;
            if c.t >= COLLAPSE_DELAY + COLLAPSE_TIME {
                self.collapse = None;
            }
        }

        self.trails.retain_mut(|t| {
            t.t += dt;
            t.t < TRAIL_TIME
        });
        self.flashes.retain_mut(|f| {
            f.t += dt;
            f.t < LOCK_FLASH_TIME
        });
        if let Some((_, remaining)) = &mut self.border {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.border = None;
            }
        }

        self.particles.update(dt);
        self.shake.update(dt);
        self.callouts.update(dt);
    }

    /// Eases the drawn row of the active piece toward `logical_y`. Call once per frame
    /// after handling that frame's events. Snaps exactly once the piece is `grounded`,
    /// so tucks and spins always show the true position.
    pub fn follow_active(&mut self, logical_y: i32, grounded: bool, dt: f32) {
        let target = logical_y as f32;
        let snap = self.snap_active || grounded || self.motion == Motion::Competitive;
        self.snap_active = false;
        match &mut self.active_y {
            Some(y) if !snap => {
                *y += (target - *y) * (1.0 - (-ACTIVE_FOLLOW_RATE * dt).exp());
                *y = y.clamp(target - ACTIVE_MAX_LAG, target + ACTIVE_MAX_LAG);
                if (target - *y).abs() < 0.01 {
                    *y = target;
                }
            }
            _ => self.active_y = Some(target),
        }
    }

    /// Vertical pixel offset of the drawn active piece from its logical row.
    pub fn active_offset(&self, logical_y: i32) -> f32 {
        self.active_y.map_or(0.0, |y| (y - logical_y as f32) * CELL)
    }

    /// While a hard drop is still falling on screen, the field as it was before the
    /// lock; the renderer should draw this instead of the live board.
    pub fn frozen_board(&self) -> Option<&Board> {
        self.drop.as_ref().map(|d| &d.board)
    }

    pub fn shake_offset(&self) -> Vec2 {
        self.shake.offset()
    }

    pub fn border_color(&self, base: Color) -> Color {
        match self.border {
            Some((flash, remaining)) => mix(base, flash, remaining / BORDER_FLASH_TIME),
            None => base,
        }
    }

    /// Vertical pixel offset for a board row while the stack is collapsing.
    pub fn row_offset(&self, y: usize) -> f32 {
        match &self.collapse {
            Some(c) => {
                let p = (c.t - COLLAPSE_DELAY) / COLLAPSE_TIME;
                -(c.shift[y] as f32) * CELL * (1.0 - out_back_soft(p))
            }
            None => 0.0,
        }
    }

    /// Effects that sit on the field, under the active piece.
    pub fn draw_field(&self, v: &View) {
        // Landed trails fade out where the piece came from.
        for trail in &self.trails {
            let fade = 1.0 - trail.t / TRAIL_TIME;
            for &(x, top, landing) in &trail.columns {
                let (_, y0) = render::cell_pos(x, top);
                let (_, y1) = render::cell_pos(x, landing);
                draw_streak(v, x, y0, y1, trail.color, fade);
            }
        }

        // In-flight hard drop: streak grows behind the falling piece, with blur copies.
        if let Some(drop) = &self.drop {
            let fallen = drop.rows_fallen_at(drop.t) * CELL;
            for &(x, top, _) in &trail_columns(&drop.piece, drop.distance) {
                let (_, y0) = render::cell_pos(x, top);
                draw_streak(v, x, y0, y0 + fallen, drop.color, 1.0);
            }
            for (k, alpha) in [(3.0, 0.12), (2.0, 0.2), (1.0, 0.32)] {
                let lag = drop.rows_fallen_at(drop.t - k * DROP_BLUR_STEP * drop.duration) * CELL;
                for (x, y) in drop.piece.cells() {
                    let (px, py) = render::cell_pos(x, y);
                    v.rect(px + 2.0, py + lag + 2.0, CELL - 4.0, CELL - 4.0, Color { a: alpha, ..drop.color });
                }
            }
            for (x, y) in drop.piece.cells() {
                let (px, py) = render::cell_pos(x, y);
                v.block(px, py + fallen, CELL, drop.color);
            }
        }

        for anim in &self.clears {
            let beam_t = anim.t / BEAM_TIME;
            let beam_alpha = (1.0 - beam_t).max(0.0) * if anim.big { 0.55 } else { 0.35 };
            for row in &anim.rows {
                let (_, py) = render::cell_pos(0, row.y as i32);
                if beam_alpha > 0.0 {
                    let h = CELL * (1.0 + 2.5 * out_cubic(beam_t));
                    let (x, w) = if anim.big {
                        (BOARD_X - 60.0, BOARD_W + 120.0)
                    } else {
                        (BOARD_X, BOARD_W)
                    };
                    v.rect(x, py + (CELL - h) / 2.0, w, h, Color { a: beam_alpha, ..WHITE });
                }
                for x in 0..WIDTH {
                    if let (false, Some(kind)) = (row.burst[x], row.cells[x]) {
                        let c = mix(render::piece_color(kind), WHITE, anim.t / FLASH_IN);
                        v.block(BOARD_X + x as f32 * CELL, py, CELL, c);
                    }
                }
            }
        }

        for flash in &self.flashes {
            let k = 1.0 - flash.t / LOCK_FLASH_TIME;
            let color = Color { a: 0.65 * k * k, ..WHITE };
            for &(x, y) in &flash.cells {
                let (px, py) = render::cell_pos(x, y);
                v.rect(px + 1.0, py + 1.0, CELL - 2.0, CELL - 2.0, color);
            }
        }
    }

    /// Effects drawn above everything else on the field.
    pub fn draw_top(&self, v: &View) {
        self.particles.draw(v);
        self.callouts.draw(v, BOARD_X + BOARD_W / 2.0, BOARD_Y + 190.0, self.time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_shift_counts_cleared_rows_below() {
        let bottom = TOTAL_ROWS - 1;
        // Clear bottom and bottom-2: the row between drops 1, rows above drop 2.
        let shift = collapse_shifts(&[bottom - 2, bottom]);
        assert_eq!(shift[bottom], 1, "old bottom-1 lands on the bottom row");
        assert_eq!(shift[bottom - 1], 2, "old bottom-3 lands two rows lower");
        assert_eq!(shift[10], 2);
    }

    #[test]
    fn hard_drop_holds_follow_up_events_until_it_lands() {
        use crate::piece::PieceKind;
        let mut fx = Fx::default();
        let piece = Active {
            kind: PieceKind::O,
            rot: 0,
            x: 3,
            y: 19,
        };
        let board = Board::new();
        fx.handle(&Event::HardDropped {
            piece,
            distance: 18,
            board: board.clone(),
        });
        fx.handle(&Event::Locked {
            piece: Active { y: 37, ..piece },
        });
        assert_eq!(fx.frozen_board(), Some(&board));
        assert!(fx.flashes.is_empty(), "lock flash waits for the landing");

        fx.update(DROP_MAX_TIME + 0.01);
        assert_eq!(fx.frozen_board(), None);
        assert_eq!(fx.flashes.len(), 1);
        assert_eq!(fx.trails.len(), 1);
    }

    #[test]
    fn second_hard_drop_lands_the_first_immediately() {
        use crate::piece::PieceKind;
        let mut fx = Fx::default();
        let piece = Active {
            kind: PieceKind::T,
            rot: 0,
            x: 3,
            y: 19,
        };
        let drop = |board| Event::HardDropped {
            piece,
            distance: 10,
            board,
        };
        fx.handle(&drop(Board::new()));
        fx.handle(&Event::Locked { piece });
        fx.handle(&drop(Board::new()));
        assert_eq!(fx.flashes.len(), 1, "first drop's lock applied on interrupt");
        assert!(fx.frozen_board().is_some(), "second drop now in flight");
    }

    const FRAME: f32 = 1.0 / 60.0;

    fn o_piece(y: i32) -> Active {
        Active {
            kind: crate::piece::PieceKind::O,
            rot: 0,
            x: 3,
            y,
        }
    }

    fn hard_drop(distance: i32) -> Event {
        Event::HardDropped {
            piece: o_piece(19),
            distance,
            board: Board::new(),
        }
    }

    fn single_clear() -> Event {
        Event::LinesCleared(LineClear {
            rows: vec![TOTAL_ROWS - 1],
            snapshot: vec![[Some(crate::piece::PieceKind::J); WIDTH]],
            t_spin: false,
            back_to_back: false,
            combo: 0,
            perfect_clear: false,
        })
    }

    /// Hard drops `count` times, `gap` seconds apart.
    fn drop_repeatedly(fx: &mut Fx, count: usize, gap: f32) {
        for _ in 0..count {
            fx.handle(&hard_drop(15));
            fx.update(gap);
        }
    }

    #[test]
    fn active_piece_glides_but_snaps_on_spawn() {
        let mut fx = Fx::default();
        fx.follow_active(20, false, FRAME);
        fx.follow_active(21, false, FRAME);
        let offset = fx.active_offset(21);
        assert!(offset < 0.0 && offset > -CELL, "partway between rows: {offset}");

        fx.handle(&Event::Spawned);
        fx.follow_active(19, false, FRAME);
        assert_eq!(fx.active_offset(19), 0.0);
    }

    #[test]
    fn glide_lag_is_capped() {
        let mut fx = Fx::default();
        fx.follow_active(20, false, FRAME);
        fx.follow_active(30, false, FRAME); // e.g. 20G gravity: many rows in one frame
        assert!(fx.active_offset(30) >= -ACTIVE_MAX_LAG * CELL - 1e-3);
    }

    #[test]
    fn glide_snaps_when_grounded() {
        let mut fx = Fx::default();
        fx.follow_active(20, false, FRAME);
        fx.follow_active(21, true, FRAME);
        assert_eq!(fx.active_offset(21), 0.0);
    }

    #[test]
    fn relaxed_pace_keeps_the_full_flight() {
        let mut fx = Fx::default();
        drop_repeatedly(&mut fx, 10, 1.0);
        let full = DROP_BASE_TIME + 15.0 * DROP_TIME_PER_ROW;
        assert!((fx.drop_duration(15) - full).abs() < 1e-4);
    }

    #[test]
    fn fast_play_shrinks_the_flight() {
        let mut fx = Fx::default();
        drop_repeatedly(&mut fx, 15, 0.125); // 8 PPS
        assert!((fx.drop_interval - 0.125).abs() < 0.01, "avg {}", fx.drop_interval);
        assert!(fx.drop_duration(15) <= DROP_SHARE_OF_INTERVAL * 0.135);
    }

    #[test]
    fn very_fast_play_skips_the_flight_but_keeps_the_impact() {
        let mut fx = Fx::default();
        drop_repeatedly(&mut fx, 20, 0.06);
        fx.handle(&hard_drop(15));
        assert_eq!(fx.frozen_board(), None, "landed immediately");
        assert!(!fx.trails.is_empty(), "impact effects still fire");
    }

    #[test]
    fn next_lock_cuts_the_collapse_short() {
        let mut fx = Fx::default();
        fx.handle(&single_clear());
        assert!(fx.row_offset(10) < 0.0, "rows above the clear start raised");
        fx.handle(&Event::Locked { piece: o_piece(37) });
        assert_eq!(fx.row_offset(10), 0.0);
        assert!(fx.clears.iter().flat_map(|a| &a.rows).all(|r| r.burst.iter().all(|&b| b)));
    }

    fn kinds(events: &[Event]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match e {
                Event::HardDropped { .. } => "drop",
                Event::Locked { .. } => "lock",
                Event::LinesCleared(_) => "clear",
                Event::Moved => "move",
                Event::Spawned => "spawn",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn hard_drop_is_presented_at_landing_with_its_follow_ups() {
        let mut fx = Fx::default();
        fx.handle(&hard_drop(18));
        fx.handle(&Event::Locked { piece: o_piece(37) });
        fx.handle(&single_clear());
        fx.handle(&Event::Spawned);
        fx.handle(&Event::Moved); // the next piece moving mid-flight
        assert_eq!(kinds(&fx.take_presented()), ["spawn", "move"], "live piece isn't delayed");

        fx.update(DROP_MAX_TIME + 0.01);
        assert_eq!(kinds(&fx.take_presented()), ["drop", "lock", "clear"]);
    }

    #[test]
    fn zero_distance_hard_drop_is_presented_immediately() {
        let mut fx = Fx::default();
        fx.handle(&hard_drop(0));
        assert_eq!(kinds(&fx.take_presented()), ["drop"]);
    }

    #[test]
    fn competitive_motion_never_leaves_the_true_board() {
        let mut fx = Fx::new(Motion::Competitive);
        fx.handle(&hard_drop(18));
        assert_eq!(fx.frozen_board(), None, "no flight");
        fx.handle(&single_clear());
        assert_eq!(fx.row_offset(10), 0.0, "no collapse");
        fx.follow_active(20, false, FRAME);
        fx.follow_active(21, false, FRAME);
        assert_eq!(fx.active_offset(21), 0.0, "no glide");
    }

    #[test]
    fn switching_to_competitive_settles_effects_in_progress() {
        let mut fx = Fx::default();
        fx.handle(&single_clear());
        fx.handle(&hard_drop(18));
        assert!(fx.frozen_board().is_some());
        fx.set_motion(Motion::Competitive);
        assert_eq!(fx.frozen_board(), None);
        assert_eq!(fx.row_offset(10), 0.0);
    }

    #[test]
    fn collapse_shift_is_zero_below_the_clear() {
        let shift = collapse_shifts(&[30]);
        assert_eq!(shift[TOTAL_ROWS - 1], 0);
        assert_eq!(shift[30], 1);
    }
}
