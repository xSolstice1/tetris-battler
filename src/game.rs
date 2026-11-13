//! Rules for a single player's field: movement, rotation, gravity, lock delay,
//! hold, preview queue, line clears and scoring. No rendering or input here, so
//! it's unit-testable and can later be driven by a bot or the network.
//!
//! Everything noteworthy that happens is pushed as an [`Event`]; the effects layer
//! (and later audio and the battle layer) consume those. Logic never waits on them.

use std::collections::VecDeque;

use crate::bag::{Bag, Rng};
use crate::board::{Board, Cell, HIDDEN_ROWS, Row, WIDTH};
use crate::piece::PieceKind;
use crate::stats::Stats;

pub const NEXT_PREVIEW: usize = 5;
const LOCK_DELAY: f32 = 0.5;
const MAX_LOCK_RESETS: u32 = 15;
const SPAWN_X: i32 = 3;
/// Box top sits one row above the visible field, so the piece's lower row starts visible.
const SPAWN_Y: i32 = HIDDEN_ROWS as i32 - 1;

/// Garbage lines sent per clear, indexed by lines cleared.
const LINE_ATTACK: [u32; 5] = [0, 0, 1, 2, 4];
/// T-spin attack, indexed by lines cleared (zero-line T-spins send nothing).
const T_SPIN_ATTACK: [u32; 4] = [0, 2, 4, 6];
/// Bonus by combo count (0 = first clear of a chain); the last entry repeats.
const COMBO_ATTACK: [u32; 12] = [0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5];
const BACK_TO_BACK_ATTACK: u32 = 1;
const PERFECT_CLEAR_ATTACK: u32 = 10;
/// At most this many garbage rows rise per lock; the rest wait for the next one.
const MAX_GARBAGE_PER_LOCK: u32 = 8;

/// Attack value of a clear, before it cancels any incoming garbage.
pub fn attack_for(lines: u32, t_spin: bool, back_to_back: bool, combo: u32, perfect_clear: bool) -> u32 {
    if lines == 0 {
        return 0;
    }
    let base = if t_spin {
        T_SPIN_ATTACK[lines.min(3) as usize]
    } else {
        LINE_ATTACK[lines.min(4) as usize]
    };
    let mut attack = base + COMBO_ATTACK[(combo as usize).min(COMBO_ATTACK.len() - 1)];
    if back_to_back {
        attack += BACK_TO_BACK_ATTACK;
    }
    if perfect_clear {
        attack += PERFECT_CLEAR_ATTACK;
    }
    attack
}

/// A chunk of incoming garbage: rows share one hole column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Garbage {
    lines: u32,
    hole: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    MoveLeft,
    MoveRight,
    SoftDrop,
    HardDrop,
    RotateCw,
    RotateCcw,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Active {
    pub kind: PieceKind,
    pub rot: u8,
    pub x: i32,
    pub y: i32,
}

impl Active {
    /// Absolute board cells occupied by this piece.
    pub fn cells(&self) -> [(i32, i32); 4] {
        self.kind
            .cells(self.rot)
            .map(|(cx, cy)| (self.x + cx, self.y + cy))
    }
}

/// A line clear (or a zero-line T-spin). `rows` are pre-clear board indices,
/// top to bottom, and `snapshot` holds what those rows contained.
#[derive(Clone, Debug, PartialEq)]
pub struct LineClear {
    pub rows: Vec<usize>,
    pub snapshot: Vec<Row>,
    pub t_spin: bool,
    pub back_to_back: bool,
    pub combo: u32,
    pub perfect_clear: bool,
    /// Attack value of this clear (garbage lines), before cancelling.
    pub attack: u32,
    /// How much of `attack` went into cancelling incoming garbage.
    pub cancelled: u32,
}

impl LineClear {
    pub fn lines(&self) -> u32 {
        self.rows.len() as u32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Moved,
    Rotated,
    SoftDropped,
    /// Emitted before the matching `Locked`. `piece` is the position before the drop
    /// and `board` the field before the piece locked, so the effects layer can show
    /// the piece travelling down before the logic's result (lock, clears) appears.
    HardDropped {
        piece: Active,
        distance: i32,
        board: Board,
    },
    Locked { piece: Active },
    LinesCleared(LineClear),
    Held,
    Spawned,
    LevelUp { level: u32 },
    /// Garbage was queued against this player; it rises on their next non-clearing lock.
    GarbageIncoming { lines: u32 },
    /// Queued garbage just pushed the stack up.
    GarbageRose { lines: u32 },
    GameOver,
}

pub struct Game {
    pub board: Board,
    pub active: Active,
    pub hold: Option<PieceKind>,
    pub hold_used: bool,
    pub next: VecDeque<PieceKind>,
    pub score: u64,
    pub lines: u32,
    pub level: u32,
    pub game_over: bool,
    /// Multiplies gravity speed (battle abilities use this). 1.0 = normal.
    pub gravity_scale: f32,
    pub stats: Stats,
    bag: Bag,
    pending_garbage: VecDeque<Garbage>,
    garbage_rng: Rng,
    /// Level rises with lines cleared unless a mode has fixed it via `set_level`.
    auto_level: bool,
    combo: i32,
    back_to_back: bool,
    gravity_acc: f32,
    lock_timer: f32,
    lock_resets: u32,
    lowest_y: i32,
    last_move_was_rotation: bool,
    events: Vec<Event>,
}

/// Seconds per row at a given level (guideline curve), capped at level 20.
fn gravity_interval(level: u32) -> f32 {
    let l = (level.clamp(1, 20) - 1) as f32;
    (0.8 - l * 0.007).powf(l)
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut bag = Bag::new(seed);
        let first = bag.next();
        let next = (0..NEXT_PREVIEW).map(|_| bag.next()).collect();
        let mut game = Game {
            board: Board::new(),
            active: Active {
                kind: first,
                rot: 0,
                x: SPAWN_X,
                y: SPAWN_Y,
            },
            hold: None,
            hold_used: false,
            next,
            score: 0,
            lines: 0,
            level: 1,
            game_over: false,
            gravity_scale: 1.0,
            stats: Stats::default(),
            bag,
            pending_garbage: VecDeque::new(),
            garbage_rng: Rng::new(seed.rotate_left(17) ^ 0x6A4B_A6E5),
            auto_level: true,
            combo: -1,
            back_to_back: false,
            gravity_acc: 0.0,
            lock_timer: 0.0,
            lock_resets: 0,
            lowest_y: SPAWN_Y,
            last_move_was_rotation: false,
            events: Vec::new(),
        };
        game.spawn(first);
        game
    }

    pub fn apply(&mut self, action: Action) {
        if self.game_over {
            return;
        }
        match action {
            Action::MoveLeft | Action::MoveRight => {
                let dx = if action == Action::MoveLeft { -1 } else { 1 };
                if self.try_move(dx, 0) {
                    self.events.push(Event::Moved);
                }
            }
            Action::SoftDrop => {
                if self.try_move(0, 1) {
                    self.score += 1;
                    self.events.push(Event::SoftDropped);
                }
            }
            Action::HardDrop => self.hard_drop(),
            Action::RotateCw => self.rotate(true),
            Action::RotateCcw => self.rotate(false),
            Action::Hold => self.hold_piece(),
        }
    }

    /// Advances gravity and lock delay by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        if self.game_over {
            return;
        }
        self.stats.time += dt;
        if self.grounded() {
            self.gravity_acc = 0.0;
            self.lock_timer += dt;
            if self.lock_timer >= LOCK_DELAY {
                self.lock();
            }
        } else {
            self.gravity_acc += dt;
            let step = gravity_interval(self.level) / self.gravity_scale.max(0.01);
            while self.gravity_acc >= step {
                self.gravity_acc -= step;
                if !self.try_move(0, 1) {
                    self.gravity_acc = 0.0;
                    break;
                }
            }
        }
    }

    /// Row the active piece would land on if hard-dropped.
    pub fn ghost_y(&self) -> i32 {
        let a = self.active;
        let mut y = a.y;
        while self.board.fits(a.kind, a.rot, a.x, y + 1) {
            y += 1;
        }
        y
    }

    /// 0.0 while airborne, rising to 1.0 as the lock delay runs out.
    pub fn lock_progress(&self) -> f32 {
        if self.game_over || !self.grounded() {
            0.0
        } else {
            (self.lock_timer / LOCK_DELAY).min(1.0)
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Queues garbage against this player. It rises on their next lock that
    /// doesn't clear lines, unless their clears cancel it first.
    pub fn receive_garbage(&mut self, lines: u32) {
        if lines == 0 || self.game_over {
            return;
        }
        let hole = self.garbage_rng.below(WIDTH);
        self.pending_garbage.push_back(Garbage { lines, hole });
        self.events.push(Event::GarbageIncoming { lines });
    }

    /// Total garbage lines waiting to rise.
    pub fn pending_garbage(&self) -> u32 {
        self.pending_garbage.iter().map(|g| g.lines).sum()
    }

    /// Drops all incoming garbage (e.g. between battle opponents).
    pub fn clear_pending_garbage(&mut self) {
        self.pending_garbage.clear();
    }

    /// Fixes the level (gravity) and stops it rising with lines cleared.
    pub fn set_level(&mut self, level: u32) {
        self.auto_level = false;
        self.level = level.max(1);
    }

    /// Replaces the field and active piece. Debug presets only.
    #[cfg(debug_assertions)]
    pub fn load_preset(&mut self, board: Board, kind: PieceKind) {
        self.board = board;
        self.hold_used = false;
        self.game_over = false;
        self.spawn(kind);
    }

    /// The active piece is resting on the stack or floor.
    pub fn grounded(&self) -> bool {
        let a = self.active;
        !self.board.fits(a.kind, a.rot, a.x, a.y + 1)
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let a = self.active;
        if !self.board.fits(a.kind, a.rot, a.x + dx, a.y + dy) {
            return false;
        }
        self.active.x += dx;
        self.active.y += dy;
        self.last_move_was_rotation = false;
        self.after_move();
        true
    }

    fn rotate(&mut self, clockwise: bool) {
        let a = self.active;
        let to = if clockwise { (a.rot + 1) % 4 } else { (a.rot + 3) % 4 };
        for (dx, dy) in a.kind.kicks(a.rot, clockwise) {
            if self.board.fits(a.kind, to, a.x + dx, a.y + dy) {
                self.active = Active {
                    rot: to,
                    x: a.x + dx,
                    y: a.y + dy,
                    ..a
                };
                self.last_move_was_rotation = true;
                self.after_move();
                self.events.push(Event::Rotated);
                return;
            }
        }
    }

    /// Lock-delay bookkeeping: reaching a new lowest row refreshes everything;
    /// moving while on the ground buys more time, up to `MAX_LOCK_RESETS` times.
    fn after_move(&mut self) {
        if self.active.y > self.lowest_y {
            self.lowest_y = self.active.y;
            self.lock_resets = 0;
            self.lock_timer = 0.0;
        } else if self.grounded() && self.lock_resets < MAX_LOCK_RESETS {
            self.lock_resets += 1;
            self.lock_timer = 0.0;
        }
    }

    fn hard_drop(&mut self) {
        let start = self.active;
        let target = self.ghost_y();
        let distance = target - start.y;
        if distance > 0 {
            self.active.y = target;
            self.last_move_was_rotation = false;
        }
        self.score += 2 * distance as u64;
        self.events.push(Event::HardDropped {
            piece: start,
            distance,
            board: self.board.clone(),
        });
        self.lock();
    }

    fn hold_piece(&mut self) {
        if self.hold_used {
            return;
        }
        let current = self.active.kind;
        let kind = match self.hold.replace(current) {
            Some(held) => held,
            None => self.pop_next(),
        };
        self.events.push(Event::Held);
        self.spawn(kind);
        self.hold_used = true;
    }

    fn pop_next(&mut self) -> PieceKind {
        let kind = self.next.pop_front().expect("preview queue is never empty");
        self.next.push_back(self.bag.next());
        kind
    }

    fn spawn(&mut self, kind: PieceKind) {
        self.active = Active {
            kind,
            rot: 0,
            x: SPAWN_X,
            y: SPAWN_Y,
        };
        self.gravity_acc = 0.0;
        self.lock_timer = 0.0;
        self.lock_resets = 0;
        self.lowest_y = SPAWN_Y;
        self.last_move_was_rotation = false;
        // Block out: no room to spawn.
        if self.board.fits(kind, 0, SPAWN_X, SPAWN_Y) {
            self.events.push(Event::Spawned);
        } else {
            self.end_game();
        }
    }

    fn end_game(&mut self) {
        self.game_over = true;
        self.events.push(Event::GameOver);
    }

    /// 3-corner rule: a T whose last move was a rotation, with 3 of the 4
    /// corners around its centre occupied.
    fn is_t_spin(&self) -> bool {
        let a = self.active;
        if a.kind != PieceKind::T || !self.last_move_was_rotation {
            return false;
        }
        [(0, 0), (2, 0), (0, 2), (2, 2)]
            .iter()
            .filter(|&&(cx, cy)| !self.board.is_free(a.x + cx, a.y + cy))
            .count()
            >= 3
    }

    fn lock(&mut self) {
        let piece = self.active;
        let t_spin = self.is_t_spin();
        self.board.place(piece.kind, piece.rot, piece.x, piece.y);
        self.events.push(Event::Locked { piece });

        // Lock out: the whole piece came to rest above the visible field.
        if piece.cells().iter().all(|&(_, y)| y < HIDDEN_ROWS as i32) {
            self.end_game();
            return;
        }

        self.stats.pieces += 1;
        let rows = self.board.full_rows();
        let cleared = rows.len();
        let snapshot: Vec<Row> = rows.iter().map(|&y| self.board.row(y)).collect();
        self.stats.garbage_cleared += snapshot
            .iter()
            .filter(|row| row.contains(&Some(Cell::Garbage)))
            .count() as u32;
        self.board.remove_rows(&rows);
        self.score_clear(rows, snapshot, t_spin);

        // Garbage only rises when the lock didn't clear anything.
        if cleared == 0 && !self.raise_garbage() {
            self.end_game();
            return;
        }

        self.hold_used = false;
        let kind = self.pop_next();
        self.spawn(kind);
    }

    /// Raises up to `MAX_GARBAGE_PER_LOCK` queued rows. False if that topped the player out.
    fn raise_garbage(&mut self) -> bool {
        let mut budget = MAX_GARBAGE_PER_LOCK;
        let mut raised = 0;
        let mut fits = true;
        while budget > 0 {
            let Some(chunk) = self.pending_garbage.front_mut() else {
                break;
            };
            let n = chunk.lines.min(budget);
            fits &= self.board.push_garbage(n as usize, chunk.hole);
            chunk.lines -= n;
            budget -= n;
            raised += n;
            if chunk.lines == 0 {
                self.pending_garbage.pop_front();
            }
        }
        if raised > 0 {
            self.events.push(Event::GarbageRose { lines: raised });
        }
        fits
    }

    /// Spends `attack` cancelling queued garbage, oldest first. Returns the amount used.
    fn cancel_garbage(&mut self, attack: u32) -> u32 {
        let mut left = attack;
        while left > 0 {
            let Some(chunk) = self.pending_garbage.front_mut() else {
                break;
            };
            let n = chunk.lines.min(left);
            chunk.lines -= n;
            left -= n;
            if chunk.lines == 0 {
                self.pending_garbage.pop_front();
            }
        }
        attack - left
    }

    fn score_clear(&mut self, rows: Vec<usize>, snapshot: Vec<Row>, t_spin: bool) {
        let cleared = rows.len() as u32;
        let level = self.level as u64;
        let base: u64 = match (t_spin, cleared) {
            (false, 0) => 0,
            (false, 1) => 100,
            (false, 2) => 300,
            (false, 3) => 500,
            (false, _) => 800,
            (true, 0) => 400,
            (true, 1) => 800,
            (true, 2) => 1200,
            (true, _) => 1600,
        };

        if cleared == 0 {
            self.combo = -1;
            self.score += base * level;
            if t_spin {
                self.events.push(Event::LinesCleared(LineClear {
                    rows,
                    snapshot,
                    t_spin,
                    back_to_back: false,
                    combo: 0,
                    perfect_clear: false,
                    attack: 0,
                    cancelled: 0,
                }));
            }
            return;
        }

        let difficult = cleared == 4 || t_spin;
        let back_to_back = difficult && self.back_to_back;
        self.back_to_back = difficult;
        self.combo += 1;

        let mut points = base * level;
        if back_to_back {
            points = points * 3 / 2;
        }
        points += 50 * self.combo as u64 * level;
        self.score += points;

        let combo = self.combo as u32;
        let perfect_clear = self.board.is_empty();
        let attack = attack_for(cleared, t_spin, back_to_back, combo, perfect_clear);
        let cancelled = self.cancel_garbage(attack);
        self.stats
            .record_clear(cleared, t_spin, back_to_back, combo, perfect_clear, attack);
        self.events.push(Event::LinesCleared(LineClear {
            rows,
            snapshot,
            t_spin,
            back_to_back,
            combo,
            perfect_clear,
            attack,
            cancelled,
        }));

        self.lines += cleared;
        let new_level = self.lines / 10 + 1;
        if self.auto_level && new_level > self.level {
            self.level = new_level;
            self.events.push(Event::LevelUp { level: new_level });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Cell, TOTAL_ROWS};

    fn game_with(kind: PieceKind) -> Game {
        let mut g = Game::new(1);
        g.spawn(kind);
        g.take_events();
        g
    }

    fn clears(events: &[Event]) -> Vec<&LineClear> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::LinesCleared(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    /// Vertical I in the rightmost column, hard-dropped.
    fn drop_i_into_right_well(g: &mut Game) {
        g.apply(Action::RotateCw);
        for _ in 0..10 {
            g.apply(Action::MoveRight);
        }
        g.apply(Action::HardDrop);
    }

    #[test]
    fn hard_drop_locks_and_spawns_next_from_queue() {
        let mut g = Game::new(3);
        let expected_next = g.next[0];
        g.apply(Action::HardDrop);
        assert_eq!(g.active.kind, expected_next);
        assert_eq!(g.next.len(), NEXT_PREVIEW);
        assert!(!g.board.is_empty());
    }

    #[test]
    fn hard_drop_reports_start_and_distance_before_locking() {
        let mut g = game_with(PieceKind::O);
        let start = g.active;
        let landing = g.ghost_y();
        let board_before = g.board.clone();
        g.apply(Action::HardDrop);
        let events = g.take_events();
        assert_eq!(
            events[0],
            Event::HardDropped {
                piece: start,
                distance: landing - start.y,
                board: board_before,
            }
        );
        assert!(matches!(events[1], Event::Locked { .. }));
    }

    #[test]
    fn hold_only_once_per_piece() {
        let mut g = Game::new(5);
        let first = g.active.kind;
        g.apply(Action::Hold);
        assert_eq!(g.hold, Some(first));
        let after_hold = g.active.kind;
        g.apply(Action::Hold);
        assert_eq!(g.active.kind, after_hold, "second hold must be ignored");
        g.apply(Action::HardDrop);
        g.apply(Action::Hold);
        assert_eq!(g.active.kind, first, "hold re-enabled after locking");
    }

    #[test]
    fn i_piece_kicks_off_the_left_wall() {
        let mut g = game_with(PieceKind::I);
        g.apply(Action::RotateCw); // vertical
        for _ in 0..10 {
            g.apply(Action::MoveLeft);
        }
        let x_before = g.active.x;
        g.apply(Action::RotateCw); // would poke through the wall without a kick
        assert_eq!(g.active.rot, 2);
        assert_ne!(g.active.x, x_before);
        assert!(g.board.fits(g.active.kind, g.active.rot, g.active.x, g.active.y));
    }

    #[test]
    fn quad_clear_reports_rows_snapshot_and_perfect_clear() {
        let mut g = game_with(PieceKind::I);
        for y in TOTAL_ROWS - 4..TOTAL_ROWS {
            for x in 0..WIDTH - 1 {
                g.board.set(x, y, PieceKind::J);
            }
        }
        drop_i_into_right_well(&mut g);

        assert_eq!(g.lines, 4);
        let events = g.take_events();
        let c = clears(&events);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].rows, (TOTAL_ROWS - 4..TOTAL_ROWS).collect::<Vec<_>>());
        assert_eq!(c[0].snapshot[0][0], Some(Cell::Piece(PieceKind::J)));
        assert_eq!(c[0].snapshot[0][WIDTH - 1], Some(Cell::Piece(PieceKind::I)));
        assert!(c[0].perfect_clear);
        assert_eq!(c[0].attack, 4 + PERFECT_CLEAR_ATTACK);
    }

    #[test]
    fn non_adjacent_clear_reports_both_rows() {
        let mut g = game_with(PieceKind::I);
        let bottom = TOTAL_ROWS - 1;
        // Rows bottom and bottom-2 need only column 9; rows in between need more.
        for x in 0..WIDTH - 1 {
            g.board.set(x, bottom, PieceKind::J);
            g.board.set(x, bottom - 2, PieceKind::J);
        }
        g.board.set(0, bottom - 1, PieceKind::S);
        g.board.set(0, bottom - 3, PieceKind::S);
        drop_i_into_right_well(&mut g);

        let events = g.take_events();
        let c = clears(&events);
        assert_eq!(c[0].rows, vec![bottom - 2, bottom]);
        assert!(!c[0].perfect_clear);
    }

    #[test]
    fn level_up_fires_once_when_crossing_ten_lines() {
        let mut g = game_with(PieceKind::I);
        g.lines = 8;
        for y in TOTAL_ROWS - 4..TOTAL_ROWS {
            for x in 0..WIDTH - 1 {
                g.board.set(x, y, PieceKind::J);
            }
        }
        drop_i_into_right_well(&mut g);

        let level_ups: Vec<_> = g
            .take_events()
            .into_iter()
            .filter(|e| matches!(e, Event::LevelUp { .. }))
            .collect();
        assert_eq!(level_ups, vec![Event::LevelUp { level: 2 }]);
    }

    #[test]
    fn lock_progress_rises_only_while_grounded() {
        let mut g = game_with(PieceKind::O);
        assert_eq!(g.lock_progress(), 0.0);
        while !g.grounded() {
            g.apply(Action::SoftDrop);
        }
        g.tick(LOCK_DELAY / 2.0);
        assert!((g.lock_progress() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn attack_table() {
        assert_eq!(attack_for(1, false, false, 0, false), 0);
        assert_eq!(attack_for(2, false, false, 0, false), 1);
        assert_eq!(attack_for(4, false, false, 0, false), 4);
        assert_eq!(attack_for(2, true, false, 0, false), 4, "T-spin double");
        assert_eq!(attack_for(4, false, true, 0, false), 5, "back-to-back quad");
        assert_eq!(attack_for(1, false, false, 5, false), 2, "combo bonus");
        assert_eq!(attack_for(1, false, false, 99, false), 5, "combo bonus caps");
        assert_eq!(attack_for(0, true, false, 0, false), 0, "zero-line T-spin");
    }

    #[test]
    fn garbage_rises_on_a_lock_that_clears_nothing() {
        let mut g = game_with(PieceKind::O);
        g.receive_garbage(3);
        assert_eq!(g.pending_garbage(), 3);
        g.apply(Action::HardDrop);
        assert_eq!(g.pending_garbage(), 0);
        let bottom = TOTAL_ROWS - 1;
        let garbage_cells = (0..WIDTH).filter(|&x| g.board.get(x, bottom) == Some(Cell::Garbage)).count();
        assert_eq!(garbage_cells, WIDTH - 1, "one hole per row");
        assert!(g.take_events().contains(&Event::GarbageRose { lines: 3 }));
    }

    #[test]
    fn clears_cancel_garbage_before_it_rises() {
        let mut g = game_with(PieceKind::I);
        for y in TOTAL_ROWS - 4..TOTAL_ROWS {
            for x in 0..WIDTH - 1 {
                g.board.set(x, y, PieceKind::J);
            }
        }
        g.receive_garbage(2);
        g.receive_garbage(20);
        drop_i_into_right_well(&mut g); // quad + perfect clear = 14 attack
        let c = clears(&g.take_events())[0].clone();
        assert_eq!((c.attack, c.cancelled), (14, 14));
        assert_eq!(g.pending_garbage(), 8);
        assert!(g.board.is_empty(), "a clearing lock raises nothing");
    }

    #[test]
    fn garbage_rises_at_most_eight_per_lock() {
        let mut g = game_with(PieceKind::O);
        g.receive_garbage(12);
        g.apply(Action::HardDrop);
        assert_eq!(g.pending_garbage(), 4);
    }

    #[test]
    fn garbage_can_top_out() {
        let mut g = game_with(PieceKind::O);
        g.receive_garbage(TOTAL_ROWS as u32);
        for _ in 0..10 {
            if g.game_over {
                break;
            }
            g.apply(Action::HardDrop);
        }
        assert!(g.game_over);
    }

    #[test]
    fn fixed_level_ignores_lines() {
        let mut g = game_with(PieceKind::I);
        g.set_level(7);
        g.lines = 9;
        for y in TOTAL_ROWS - 4..TOTAL_ROWS {
            for x in 0..WIDTH - 1 {
                g.board.set(x, y, PieceKind::J);
            }
        }
        drop_i_into_right_well(&mut g);
        assert_eq!(g.level, 7);
    }

    #[test]
    fn stats_count_pieces_time_attack_and_garbage_dug() {
        let mut g = game_with(PieceKind::I);
        g.tick(0.5);
        assert!((g.stats.time - 0.5).abs() < 1e-6);
        // Bottom 4 rows are garbage with the hole in column 9.
        assert!(g.board.push_garbage(4, WIDTH - 1));
        drop_i_into_right_well(&mut g);
        assert_eq!(g.stats.pieces, 1);
        assert_eq!(g.stats.lines, 4);
        assert_eq!(g.stats.garbage_cleared, 4);
        assert_eq!(g.stats.quads, 1);
        assert_eq!(g.stats.attack, 4 + 10, "quad + perfect clear");
    }

    #[test]
    fn stats_clock_stops_at_game_over() {
        let mut g = game_with(PieceKind::O);
        g.game_over = true;
        g.tick(1.0);
        assert_eq!(g.stats.time, 0.0);
    }

    #[test]
    fn gravity_moves_piece_down() {
        let mut g = Game::new(9);
        let y0 = g.active.y;
        g.tick(1.01);
        assert!(g.active.y > y0);
    }
}
