//! Rules for a single player's field: movement, rotation, gravity, lock delay,
//! hold, preview queue, line clears and scoring. No rendering or input here, so
//! it's unit-testable and can later be driven by a bot or the network.
//!
//! Everything noteworthy that happens is pushed as an [`Event`]; the effects layer
//! (and later audio and the battle layer) consume those. Logic never waits on them.

use std::collections::VecDeque;

use crate::bag::Bag;
use crate::board::{Board, HIDDEN_ROWS, Row};
use crate::piece::PieceKind;

pub const NEXT_PREVIEW: usize = 5;
const LOCK_DELAY: f32 = 0.5;
const MAX_LOCK_RESETS: u32 = 15;
const SPAWN_X: i32 = 3;
/// Box top sits one row above the visible field, so the piece's lower row starts visible.
const SPAWN_Y: i32 = HIDDEN_ROWS as i32 - 1;

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
    bag: Bag,
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
            bag,
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
        if self.grounded() {
            self.gravity_acc = 0.0;
            self.lock_timer += dt;
            if self.lock_timer >= LOCK_DELAY {
                self.lock();
            }
        } else {
            self.gravity_acc += dt;
            let step = gravity_interval(self.level);
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

    /// Replaces the field and active piece. Debug presets only.
    #[cfg(debug_assertions)]
    pub fn load_preset(&mut self, board: Board, kind: PieceKind) {
        self.board = board;
        self.hold_used = false;
        self.game_over = false;
        self.spawn(kind);
    }

    fn grounded(&self) -> bool {
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

        let rows = self.board.full_rows();
        let snapshot = rows.iter().map(|&y| self.board.row(y)).collect();
        self.board.remove_rows(&rows);
        self.score_clear(rows, snapshot, t_spin);

        self.hold_used = false;
        let kind = self.pop_next();
        self.spawn(kind);
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

        self.events.push(Event::LinesCleared(LineClear {
            rows,
            snapshot,
            t_spin,
            back_to_back,
            combo: self.combo as u32,
            perfect_clear: self.board.is_empty(),
        }));

        self.lines += cleared;
        let new_level = self.lines / 10 + 1;
        if new_level > self.level {
            self.level = new_level;
            self.events.push(Event::LevelUp { level: new_level });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{TOTAL_ROWS, WIDTH};

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
        assert_eq!(c[0].snapshot[0][0], Some(PieceKind::J));
        assert_eq!(c[0].snapshot[0][WIDTH - 1], Some(PieceKind::I));
        assert!(c[0].perfect_clear);
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
    fn gravity_moves_piece_down() {
        let mut g = Game::new(9);
        let y0 = g.active.y;
        g.tick(1.01);
        assert!(g.active.y > y0);
    }
}
