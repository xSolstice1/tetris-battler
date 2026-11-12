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

use crate::board::{Row, TOTAL_ROWS, WIDTH};
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

#[derive(Default)]
pub struct Fx {
    particles: Particles,
    shake: Shake,
    callouts: Callouts,
    clears: Vec<ClearAnim>,
    collapse: Option<Collapse>,
    trails: Vec<Trail>,
    flashes: Vec<LockFlash>,
    border: Option<(Color, f32)>,
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

impl Fx {
    pub fn handle(&mut self, event: &Event) {
        match event {
            Event::HardDropped { piece, distance } => self.hard_drop(*piece, *distance),
            Event::Locked { piece } => self.flashes.push(LockFlash {
                cells: piece.cells(),
                t: 0.0,
            }),
            Event::LinesCleared(clear) => self.line_clear(clear),
            Event::LevelUp { level } => {
                let gold = render::hex(0xffd23f);
                self.callouts.push(format!("LEVEL {level}"), gold, 30.0);
                self.border = Some((gold, BORDER_FLASH_TIME));
            }
            Event::GameOver => self.shake.add(0.6),
            Event::Moved | Event::Rotated | Event::SoftDropped | Event::Held | Event::Spawned => {}
        }
    }

    fn hard_drop(&mut self, piece: Active, distance: i32) {
        if distance <= 0 {
            return;
        }
        let color = render::piece_color(piece.kind);
        let cells = piece.cells();

        let mut columns: Vec<(i32, i32, i32)> = Vec::new();
        for &(x, y) in &cells {
            match columns.iter_mut().find(|c| c.0 == x) {
                Some(c) => {
                    c.1 = c.1.min(y);
                    c.2 = c.2.min(y + distance);
                }
                None => columns.push((x, y, y + distance)),
            }
        }
        self.trails.push(Trail { columns, color, t: 0.0 });

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
            self.collapse = Some(Collapse {
                shift: collapse_shifts(&c.rows),
                t: 0.0,
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;

        for anim in &mut self.clears {
            anim.t += dt;
            for row in &mut anim.rows {
                for x in 0..WIDTH {
                    let delay = (x as f32 - (WIDTH as f32 - 1.0) / 2.0).abs().floor() * SWEEP_PER_COLUMN;
                    if !row.burst[x] && anim.t >= delay {
                        row.burst[x] = true;
                        if let Some(kind) = row.cells[x] {
                            let color = render::piece_color(kind);
                            burst_cell(&mut self.particles, x as i32, row.y as i32, color, anim.big);
                        }
                    }
                }
            }
        }
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
        for trail in &self.trails {
            let fade = 1.0 - trail.t / TRAIL_TIME;
            for &(x, top, landing) in &trail.columns {
                let (px, y0) = render::cell_pos(x, top);
                let (_, y1) = render::cell_pos(x, landing);
                if y1 <= y0 {
                    continue;
                }
                // Brighter and wider toward the landing point.
                const SLICES: usize = 10;
                let h = (y1 - y0) / SLICES as f32;
                for i in 0..SLICES {
                    let f = (i + 1) as f32 / SLICES as f32;
                    let w = (CELL - 10.0) * (0.4 + 0.6 * f);
                    v.rect(
                        px + (CELL - w) / 2.0,
                        y0 + h * i as f32,
                        w,
                        h + 0.5,
                        Color { a: f * 0.45 * fade, ..mix(trail.color, WHITE, 0.4) },
                    );
                }
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
    fn collapse_shift_is_zero_below_the_clear() {
        let shift = collapse_shifts(&[30]);
        assert_eq!(shift[TOTAL_ROWS - 1], 0);
        assert_eq!(shift[30], 1);
    }
}
