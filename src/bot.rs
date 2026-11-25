//! Shadow opponents: bots that play a real `Game` through the same actions a
//! player has (rotate, move, hold, hard drop), at a set pace and skill.
//!
//! Each piece the bot looks at every reachable placement of the current piece
//! and of the hold piece, scores the field each would leave (holes, height,
//! bumpiness, lines and their attack value), and plays the best. Weaker shadows
//! sometimes settle for a worse spot or misdrop. Pure logic, no rendering.

use crate::bag::Rng;
use crate::board::{Board, Cell, HIDDEN_ROWS, TOTAL_ROWS, WIDTH};
use crate::game::{Action, Active, Game, attack_for};
use crate::piece::PieceKind;

/// How a shadow plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub name: &'static str,
    /// One-line description for the opponent select screen.
    pub blurb: &'static str,
    /// Difficulty label.
    pub rank: &'static str,
    /// Pace: pieces per minute, each piece's time drawn from this range.
    pub ppm: (f32, f32),
    /// Seconds between the keys it presses for a piece.
    pub key_gap: f32,
    /// Chance per piece of settling for a near-best placement instead of the best.
    pub sloppy: f32,
    /// Chance per piece of a real misdrop (a clearly worse placement).
    pub misdrop: f32,
    /// Builds a well for quads instead of clearing lines as soon as it can.
    pub stacks_for_quads: bool,
    /// Uses hold.
    pub holds: bool,
    /// Also weighs where the next piece could go on each field (slower, much stronger).
    pub lookahead: bool,
    /// Finds tucks and spins (soft drop, then slide or rotate under overhangs), sets up
    /// T-spin slots, and chases T-spins, combos and perfect clears.
    pub spins: bool,
    /// When the stack is low, searches the coming pieces for a perfect clear and plays it.
    pub pc_search: bool,
    /// UI accent colour (0xRRGGBB).
    pub color: u32,
    /// Portrait art key (assets/portraits/<key>.png).
    pub portrait: &'static str,
}

/// Easiest first.
pub const SHADOWS: [Profile; 7] = [
    Profile {
        name: "JIANJIE THE SLOTH",
        blurb: "In no hurry. None at all. For your very first match.",
        rank: "SLOTH",
        // About 10 PPM.
        ppm: (9.0, 11.0),
        key_gap: 0.25,
        sloppy: 0.35,
        misdrop: 0.05,
        stacks_for_quads: false,
        holds: false,
        lookahead: false,
        spins: false,
        pc_search: false,
        color: 0xb08d6a,
        portrait: "jianjie",
    },
    Profile {
        name: "LUOYI ANGEL",
        blurb: "Gentle and patient. The perfect first opponent.",
        rank: "NOVICE",
        // About 0.75x Luoyi.
        ppm: (45.0, 52.0),
        key_gap: 0.1,
        sloppy: 0.08,
        misdrop: 0.0,
        stacks_for_quads: true,
        holds: true,
        lookahead: false,
        spins: false,
        pc_search: false,
        color: 0xf4e3a1,
        portrait: "luoyi_angel",
    },
    Profile {
        name: "LUOYI",
        blurb: "Learning to stack for quads. Slow, but tidy.",
        rank: "BEGINNER",
        ppm: (60.0, 70.0),
        key_gap: 0.08,
        sloppy: 0.08,
        misdrop: 0.002,
        stacks_for_quads: true,
        holds: true,
        lookahead: false,
        spins: false,
        pc_search: false,
        color: 0x7fd8ff,
        portrait: "luoyi",
    },
    Profile {
        name: "LUOYI EXTREME",
        blurb: "Luoyi, sharper. A little faster, a lot fewer slips.",
        rank: "INTERMEDIATE",
        ppm: (70.0, 78.0),
        key_gap: 0.07,
        sloppy: 0.08,
        misdrop: 0.0,
        stacks_for_quads: true,
        holds: true,
        lookahead: false,
        spins: false,
        pc_search: false,
        color: 0xff6a3c,
        portrait: "luoyi_extreme",
    },
    Profile {
        name: "DEON",
        blurb: "S-tier. Clean stack, quads back to back, punishes every mistake.",
        rank: "S TIER",
        // 1.6-1.7 PPS.
        ppm: (97.0, 104.0),
        key_gap: 0.05,
        sloppy: 0.0,
        misdrop: 0.0,
        stacks_for_quads: true,
        holds: true,
        lookahead: true,
        spins: false,
        pc_search: false,
        color: 0x5cdb5c,
        portrait: "deon",
    },
    Profile {
        name: "DEON EXTREME",
        blurb: "Deon unleashed. T-spins, quads, combos and perfect clears.",
        rank: "EXTREME",
        // Averages 110-120 PPM.
        ppm: (111.0, 122.0),
        key_gap: 0.035,
        sloppy: 0.0,
        misdrop: 0.0,
        stacks_for_quads: true,
        holds: true,
        lookahead: true,
        spins: true,
        pc_search: false,
        color: 0xc061ff,
        portrait: "deon_extreme",
    },
    Profile {
        name: "DEON NIGHTMARE",
        blurb: "Don't. Perfect clears on sight, T-spins, no wasted move.",
        rank: "NIGHTMARE",
        // About 1.5x Deon Extreme.
        ppm: (166.0, 180.0),
        key_gap: 0.022,
        sloppy: 0.0,
        misdrop: 0.0,
        stacks_for_quads: true,
        holds: true,
        lookahead: true,
        spins: true,
        pc_search: true,
        color: 0xff3b1f,
        portrait: "deon_nightmare",
    },
];

/// Where pieces appear (matches `Game`'s spawn).
const SPAWN_X: i32 = 3;
const SPAWN_Y: i32 = HIDDEN_ROWS as i32 - 1;
/// A sloppy pick stays within this much of the best placement's score.
const SLOPPY_MARGIN: f32 = 3.0;
/// The column kept open for quads.
const WELL: usize = WIDTH - 1;

/// A placement the bot could make, with the keys that get there.
#[derive(Clone, Debug)]
struct Candidate {
    actions: Vec<Action>,
    score: f32,
}

pub struct Bot {
    pub profile: Profile,
    rng: Rng,
    /// Keys still to press for the current piece, and the time until the next one.
    plan: Vec<Action>,
    key_timer: f32,
    /// Pieces locked so far (to notice a new piece).
    seen_pieces: u32,
    planned: bool,
    /// The plan was made at spawn (for timing); it is made again from where the piece
    /// actually is (gravity moves it meanwhile) just before the first key.
    fresh: bool,
    /// Where the piece was when the plan was made.
    planned_at: Option<Active>,
}

impl Bot {
    pub fn new(profile: Profile, seed: u64) -> Self {
        let bot = Bot {
            profile,
            rng: Rng::new(seed ^ 0x5AD0_57A7),
            plan: Vec::new(),
            key_timer: 0.0,
            seen_pieces: 0,
            planned: false,
            fresh: false,
            planned_at: None,
        };
        bot
    }

    fn rand01(&mut self) -> f32 {
        (self.rng.next_u64() % 1_000_000) as f32 / 1_000_000.0
    }

    /// Seconds this piece should take, from the profile's pace.
    fn piece_time(&mut self) -> f32 {
        let (lo, hi) = self.profile.ppm;
        let ppm = lo + (hi - lo) * self.rand01();
        60.0 / ppm
    }

    /// Advances the bot by `dt`, pressing keys on `game` when due. Call before `game.tick`.
    pub fn update(&mut self, dt: f32, game: &mut Game) {
        if game.game_over {
            return;
        }
        if game.stats.pieces != self.seen_pieces || !self.planned {
            // A new piece: pick its placement now (the field only changes on a lock), then
            // wait so its hard drop lands on the piece's time: the pace is the pace.
            self.seen_pieces = game.stats.pieces;
            let total = self.piece_time();
            self.plan = self.choose(game);
            // Soft drop is held, so its rows come in one go and cost no key time.
            let keys = self.plan.iter().filter(|&&a| a != Action::SoftDrop).count().max(1) as f32;
            self.plan.reverse();
            self.planned = true;
            self.fresh = false;
            self.planned_at = Some(game.active);
            self.key_timer = (total - (keys - 1.0) * self.profile.key_gap).max(0.0);
        }
        self.key_timer -= dt;
        if self.key_timer <= 0.0 && !self.fresh {
            self.fresh = true;
            // Only if gravity moved the piece: otherwise the plan still holds.
            if self.planned_at != Some(game.active) {
                self.plan = self.choose(game);
                self.plan.reverse();
            }
        }
        while self.key_timer <= 0.0 {
            let Some(action) = self.plan.pop() else {
                break;
            };
            game.apply(action);
            if action == Action::SoftDrop {
                continue;
            }
            self.key_timer += self.profile.key_gap;
            if action == Action::HardDrop {
                break;
            }
        }
    }

    /// Picks the keys for the current piece.
    fn choose(&mut self, game: &Game) -> Vec<Action> {
        let mut options = candidates(game, self.profile);
        if options.is_empty() {
            return vec![Action::HardDrop];
        }
        options.sort_by(|a, b| b.score.total_cmp(&a.score));
        let roll = self.rand01();
        let pick = if roll < self.profile.misdrop {
            let n = options.len().min(12);
            2.min(n - 1) + self.rng.below((n - 2.min(n - 1)).max(1))
        } else if roll < self.profile.misdrop + self.profile.sloppy {
            // Near enough: any of the top few that is almost as good as the best.
            let best = options[0].score;
            let close = options.iter().take(4).filter(|o| o.score >= best - SLOPPY_MARGIN).count();
            self.rng.below(close.max(1))
        } else {
            0
        };
        options.swap_remove(pick.min(options.len() - 1)).actions
    }
}

/// Rotates like `Game` does (SRS kicks). None if no kick fits.
fn rotate(board: &Board, a: Active, clockwise: bool) -> Option<Active> {
    let to = if clockwise { (a.rot + 1) % 4 } else { (a.rot + 3) % 4 };
    a.kind.kicks(a.rot, clockwise).into_iter().find_map(|(dx, dy)| {
        board
            .fits(a.kind, to, a.x + dx, a.y + dy)
            .then_some(Active { rot: to, x: a.x + dx, y: a.y + dy, ..a })
    })
}

fn drop_y(board: &Board, a: Active) -> i32 {
    let mut y = a.y;
    while board.fits(a.kind, a.rot, a.x, y + 1) {
        y += 1;
    }
    y
}

/// Where a piece can end up, the keys that get it there, and whether locking it there
/// counts as a T-spin.
#[derive(Clone, Debug)]
struct Placement {
    piece: Active,
    keys: Vec<Action>,
    t_spin: bool,
}

/// Every placement reachable from `start` by rotating, sliding and hard-dropping:
/// where it lands and the keys that get there.
fn placements(board: &Board, start: Active, prefix: &[Action]) -> Vec<Placement> {
    let mut out = Vec::new();
    // Rotation states: none, cw, cw+cw, ccw.
    let turns: [&[bool]; 4] = [&[], &[true], &[true, true], &[false]];
    for turn in turns {
        let mut a = start;
        let mut keys = prefix.to_vec();
        let mut ok = true;
        for &cw in turn {
            match rotate(board, a, cw) {
                Some(r) => a = r,
                None => ok = false,
            }
            keys.push(if cw { Action::RotateCw } else { Action::RotateCcw });
        }
        if !ok {
            continue;
        }
        // Drop here, then slide to every column it can reach at this height.
        let mut push = |s: Active, mut keys: Vec<Action>| {
            keys.push(Action::HardDrop);
            out.push(Placement { piece: Active { y: drop_y(board, s), ..s }, keys, t_spin: false });
        };
        push(a, keys.clone());
        for dir in [-1, 1] {
            let mut s = a;
            let mut slide = keys.clone();
            while board.fits(s.kind, s.rot, s.x + dir, s.y) {
                s.x += dir;
                slide.push(if dir < 0 { Action::MoveLeft } else { Action::MoveRight });
                push(s, slide.clone());
            }
        }
    }
    out
}

/// `Game`'s T-spin rule: a T whose last move was a rotation, with 3 of the 4 corners
/// around its centre occupied.
fn is_t_spin(board: &Board, a: Active, last_was_rotation: bool) -> bool {
    a.kind == PieceKind::T
        && last_was_rotation
        && [(0, 0), (2, 0), (0, 2), (2, 2)].iter().filter(|&&(cx, cy)| !board.is_free(a.x + cx, a.y + cy)).count() >= 3
}

/// Every resting spot reachable from `start` by any mix of slides, rotations (with kicks)
/// and soft drops, then a hard drop: tucks under overhangs and spins into slots included.
/// Breadth-first, so each spot comes with its shortest key sequence.
fn spin_placements(board: &Board, start: Active, prefix: &[Action]) -> Vec<Placement> {
    const XS: i32 = WIDTH as i32 + 6;
    let index = |a: Active, rot_last: bool| -> usize {
        ((((a.y as usize) * XS as usize + (a.x + 3) as usize) * 4 + a.rot as usize) * 2) + rot_last as usize
    };
    let mut seen = vec![false; (TOTAL_ROWS + 2) * XS as usize * 4 * 2];
    // (state, last move was a rotation, parent node, action that led here)
    let mut nodes: Vec<(Active, bool, usize, Action)> = vec![(start, false, usize::MAX, Action::HardDrop)];
    seen[index(start, false)] = true;
    let mut found: Vec<([(i32, i32); 4], bool)> = Vec::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        let (a, rot_last, _, _) = nodes[i];
        if !board.fits(a.kind, a.rot, a.x, a.y + 1) {
            // Resting: lock it here.
            let t_spin = is_t_spin(board, a, rot_last);
            let mut cells = a.cells();
            cells.sort();
            if !found.contains(&(cells, t_spin)) {
                found.push((cells, t_spin));
                let mut keys = vec![Action::HardDrop];
                let mut n = i;
                while nodes[n].2 != usize::MAX {
                    keys.push(nodes[n].3);
                    n = nodes[n].2;
                }
                keys.extend(prefix.iter().rev());
                keys.reverse();
                out.push(Placement { piece: a, keys, t_spin });
            }
        }
        let mut next = |b: Active, rot: bool, act: Action, nodes: &mut Vec<(Active, bool, usize, Action)>| {
            let k = index(b, rot);
            if b.y >= 0 && (b.y as usize) < TOTAL_ROWS + 2 && b.x >= -3 && b.x < XS - 3 && !seen[k] {
                seen[k] = true;
                nodes.push((b, rot, i, act));
            }
        };
        for (dx, act) in [(-1, Action::MoveLeft), (1, Action::MoveRight)] {
            if board.fits(a.kind, a.rot, a.x + dx, a.y) {
                next(Active { x: a.x + dx, ..a }, false, act, &mut nodes);
            }
        }
        if board.fits(a.kind, a.rot, a.x, a.y + 1) {
            next(Active { y: a.y + 1, ..a }, false, Action::SoftDrop, &mut nodes);
        }
        for (cw, act) in [(true, Action::RotateCw), (false, Action::RotateCcw)] {
            if let Some(r) = rotate(board, a, cw) {
                next(r, true, act, &mut nodes);
            }
        }
        i += 1;
    }
    out
}

/// The best open T-spin slot on a field: a downward T resting under an overhang with
/// three corners covered, that would complete two (or three) rows. Returns how many rows
/// it would clear and how many of the field's "holes" are really just the slot (they are
/// waiting for the T, not mistakes). A spinner builds these.
fn t_slot(b: &Board) -> Option<(usize, u32)> {
    let mut best: Option<(usize, u32)> = None;
    for y in HIDDEN_ROWS as i32 - 2..TOTAL_ROWS as i32 - 1 {
        for x in -1..WIDTH as i32 - 1 {
            let a = Active { kind: PieceKind::T, rot: 2, x, y };
            if !b.fits(a.kind, a.rot, x, y) || b.fits(a.kind, a.rot, x, y + 1) {
                continue;
            }
            if !is_t_spin(b, a, true) {
                continue;
            }
            // Covered from above (not just droppable), and two rows it would complete.
            if b.fits(a.kind, a.rot, x, y - 1) && b.fits(a.kind, a.rot, x, y - 2) && b.fits(a.kind, a.rot, x, y - 3) {
                continue;
            }
            let mut t = b.clone();
            t.place(a.kind, a.rot, x, y);
            let full = a.cells().iter().map(|c| c.1).filter(|&r| (0..WIDTH).all(|cx| t.get(cx, r as usize).is_some())).collect::<std::collections::BTreeSet<_>>().len();
            if full >= 2 {
                // Slot cells with a block somewhere above them count as holes in `field_score`.
                let roofed = a
                    .cells()
                    .iter()
                    .filter(|&&(cx, cy)| (0..cy).any(|yy| b.get(cx as usize, yy as usize).is_some()))
                    .count() as u32;
                if best.is_none_or(|(f, _)| full > f) {
                    best = Some((full, roofed));
                }
            }
        }
    }
    best
}

// ───────────── Perfect-clear search ─────────────
//
// On a low stack the bottom rows fit in a u64 (bit `row * 10 + x`, row 0 at the floor).
// A depth-first search over the coming pieces (with hold) looks for placements that fill
// every cell and leave the board empty; the first of them is played.

/// Highest stack a perfect clear is searched from (rows).
const PC_MAX_HEIGHT: u32 = 4;
/// Search budget per decision (nodes), so a hopeless search can't stall the game.
const PC_NODE_BUDGET: u32 = 60_000;

/// A piece's cells for a rotation as (dx, rows above its lowest cell).
fn pc_shape(kind: PieceKind, rot: u8) -> [(i32, i32); 4] {
    let cells = kind.cells(rot);
    let bottom = cells.iter().map(|c| c.1).max().unwrap();
    cells.map(|(cx, cy)| (cx, bottom - cy))
}

/// The low part of a field as bits, if everything above `PC_MAX_HEIGHT` rows is empty.
fn pc_bits(board: &Board) -> Option<(u64, u32)> {
    let mut bits = 0u64;
    let mut height = 0;
    for y in 0..TOTAL_ROWS {
        let row = (TOTAL_ROWS - 1 - y) as u32;
        for x in 0..WIDTH {
            if board.get(x, y).is_some() {
                if row >= PC_MAX_HEIGHT {
                    return None;
                }
                bits |= 1 << (row * WIDTH as u32 + x as u32);
                height = height.max(row + 1);
            }
        }
    }
    Some((bits, height))
}

/// Hard-drops `shape` at column `x` onto `bits`; returns the cells' bits and lowest row,
/// if it lands entirely below `limit` rows.
fn pc_drop(bits: u64, shape: &[(i32, i32); 4], x: i32, limit: u32) -> Option<(u64, u32)> {
    if shape.iter().any(|&(dx, _)| x + dx < 0 || x + dx >= WIDTH as i32) {
        return None;
    }
    let mask = |r: i32| -> Option<u64> {
        let mut m = 0u64;
        for &(dx, dy) in shape {
            let row = r + dy;
            if row < 0 {
                return None;
            }
            if row < 6 {
                m |= 1 << (row as u32 * WIDTH as u32 + (x + dx) as u32);
            }
        }
        Some(m)
    };
    let mut r = 6;
    while let Some(m) = mask(r - 1) {
        if m & bits != 0 {
            break;
        }
        r -= 1;
    }
    let m = mask(r)?;
    if shape.iter().any(|&(_, dy)| (r + dy) as u32 >= limit) {
        return None;
    }
    Some((m, r as u32))
}

/// Clears full rows (shifting the ones above down).
fn pc_clear(bits: u64, height: u32) -> (u64, u32) {
    let full = (1u64 << WIDTH) - 1;
    let mut out = 0u64;
    let mut kept = 0;
    for row in 0..height {
        let r = (bits >> (row * WIDTH as u32)) & full;
        if r != full {
            out |= r << (kept * WIDTH as u32);
            kept += 1;
        }
    }
    (out, kept)
}

/// Whether some empty cell sits under a filled one (only spins could fill it).
fn pc_covered(bits: u64, height: u32) -> bool {
    (0..WIDTH as u32).any(|x| {
        let mut seen_empty = false;
        for row in 0..height {
            let filled = bits >> (row * WIDTH as u32 + x) & 1 == 1;
            if !filled {
                seen_empty = true;
            } else if seen_empty {
                return true;
            }
        }
        false
    })
}

struct PcSearch<'a> {
    queue: &'a [PieceKind],
    nodes: u32,
    dead: std::collections::HashSet<(u64, u32, usize, Option<PieceKind>)>,
}

impl PcSearch<'_> {
    /// Can `bits` (filling `height` rows exactly) be cleared from queue position `i` with `hold`?
    fn solve(&mut self, bits: u64, height: u32, i: usize, hold: Option<PieceKind>) -> bool {
        if bits == 0 && height == 0 {
            return true;
        }
        let empty = height * WIDTH as u32 - bits.count_ones();
        let left = self.queue.len().saturating_sub(i) as u32 + hold.is_some() as u32;
        if empty % 4 != 0 || empty / 4 > left || self.nodes > PC_NODE_BUDGET {
            return false;
        }
        let key = (bits, height, i, hold);
        if self.dead.contains(&key) {
            return false;
        }
        self.nodes += 1;
        for (kind, next_i, next_hold) in self.options(i, hold) {
            for rot in 0..4u8 {
                if kind == PieceKind::O && rot > 0 {
                    break;
                }
                let shape = pc_shape(kind, rot);
                for x in -3..WIDTH as i32 {
                    let Some((m, _)) = pc_drop(bits, &shape, x, height) else { continue };
                    let (after, h) = pc_clear(bits | m, height);
                    if pc_covered(after, h) {
                        continue;
                    }
                    if self.solve(after, h, next_i, next_hold) {
                        return true;
                    }
                }
            }
        }
        self.dead.insert(key);
        false
    }

    /// The pieces playable at queue position `i`: the current one, or (via hold) the held
    /// one or the one after. With where the queue and hold stand afterwards.
    fn options(&self, i: usize, hold: Option<PieceKind>) -> Vec<(PieceKind, usize, Option<PieceKind>)> {
        let mut out = Vec::new();
        let Some(&cur) = self.queue.get(i) else {
            return hold.map(|h| vec![(h, i, None)]).unwrap_or_default();
        };
        out.push((cur, i + 1, hold));
        match hold {
            Some(h) if h != cur => out.push((h, i + 1, Some(cur))),
            None => {
                if let Some(&n) = self.queue.get(i + 1) {
                    out.push((n, i + 2, Some(cur)));
                }
            }
            _ => {}
        }
        out
    }
}

/// If the coming pieces can clear the whole field, the keys for this piece's part of it.
fn perfect_clear_move(game: &Game) -> Option<Vec<Action>> {
    let (bits, height) = pc_bits(&game.board)?;
    if bits == 0 {
        return None;
    }
    let mut queue = vec![game.active.kind];
    queue.extend(game.next.iter().copied());
    // Smallest height the clear can finish at with these pieces.
    let target = (height.max(1)..=PC_MAX_HEIGHT).find(|&h| (h * WIDTH as u32 - bits.count_ones()) % 4 == 0)?;
    let mut search = PcSearch { queue: &queue, nodes: 0, dead: Default::default() };
    let can_hold = !game.hold_used;
    for (kind, next_i, next_hold) in search.options(0, game.hold) {
        let held = kind != game.active.kind || next_i == 2;
        if held && !can_hold {
            continue;
        }
        for rot in 0..4u8 {
            let shape = pc_shape(kind, rot);
            for x in -3..WIDTH as i32 {
                let Some((m, _)) = pc_drop(bits, &shape, x, target) else { continue };
                let (after, h) = pc_clear(bits | m, target);
                if pc_covered(after, h) || !search.solve(after, h, next_i, next_hold) {
                    continue;
                }
                // Found: the keys that put this piece on exactly those cells.
                let start = if held { spawn(kind) } else { game.active };
                let prefix: Vec<Action> = if held { vec![Action::Hold] } else { vec![] };
                let want = m;
                return placements(&game.board, start, &prefix).into_iter().find_map(|p| {
                    let mut got = 0u64;
                    for (cx, cy) in p.piece.cells() {
                        let row = TOTAL_ROWS as i32 - 1 - cy;
                        if !(0..6).contains(&row) {
                            return None;
                        }
                        got |= 1 << (row as u32 * WIDTH as u32 + cx as u32);
                    }
                    (got == want).then_some(p.keys)
                });
            }
        }
    }
    None
}

fn spawn(kind: PieceKind) -> Active {
    Active { kind, rot: 0, x: SPAWN_X, y: SPAWN_Y }
}

/// Every placement of the current piece and (if allowed) the hold piece, scored.
/// Shadows that look ahead also try the piece after it on each resulting field.
fn candidates(game: &Game, profile: Profile) -> Vec<Candidate> {
    if profile.pc_search {
        if let Some(keys) = perfect_clear_move(game) {
            return vec![Candidate { score: 1e9, actions: keys }];
        }
    }
    let pending = game.pending_garbage();
    let mut starts = vec![(game.active, Vec::new(), game.next[0])];
    if profile.holds && !game.hold_used {
        let (kind, after) = match game.hold {
            Some(h) => (h, game.next[0]),
            None => (game.next[0], game.next[1]),
        };
        if kind != game.active.kind && game.board.fits(kind, 0, SPAWN_X, SPAWN_Y) {
            starts.push((spawn(kind), vec![Action::Hold], after));
        }
    }
    let now = Chain { b2b: game.back_to_back(), combo: game.combo() };
    let mut out = Vec::new();
    for (start, prefix, after) in starts {
        let options = if profile.spins { spin_placements(&game.board, start, &prefix) } else { placements(&game.board, start, &prefix) };
        for p in options {
            let Some(first) = settle(&game.board, p.piece, p.t_spin) else {
                out.push(Candidate { score: -1e6, actions: p.keys });
                continue;
            };
            let (clear, chain) = clear_score(&first, now, pending, profile);
            let field = first.field;
            let score = if profile.lookahead && field.fits(after, 0, SPAWN_X, SPAWN_Y) {
                // The best this field allows next, from which only its clears and final field count.
                let best_next = placements(&field, spawn(after), &[])
                    .into_iter()
                    .filter_map(|p2| {
                        let second = settle(&field, p2.piece, false)?;
                        let (c2, _) = clear_score(&second, chain, pending, profile);
                        Some(c2 + field_score(&second.field, p2.piece, pending, profile))
                    })
                    .fold(f32::MIN, f32::max);
                clear + best_next.max(-1e5)
            } else {
                clear + field_score(&field, p.piece, pending, profile)
            };
            out.push(Candidate { score, actions: p.keys });
        }
    }
    out
}

/// Column heights (rows filled from the floor up to the top block).
fn heights(board: &Board) -> [i32; WIDTH] {
    let mut h = [0; WIDTH];
    for (x, hx) in h.iter_mut().enumerate() {
        *hx = (0..TOTAL_ROWS).find(|&y| board.get(x, y).is_some()).map_or(0, |y| (TOTAL_ROWS - y) as i32);
    }
    h
}

/// What a lock does: the field after its full rows clear, how many cleared and how many
/// of those were garbage.
struct Settled {
    field: Board,
    lines: u32,
    garbage: u32,
    t_spin: bool,
}

/// Back-to-back and combo state carried from one lock to the next.
#[derive(Clone, Copy, Debug)]
struct Chain {
    b2b: bool,
    combo: i32,
}

/// None if `piece` would lock out (entirely above the visible field).
fn settle(board: &Board, piece: Active, t_spin: bool) -> Option<Settled> {
    if piece.cells().iter().all(|&(_, y)| y < HIDDEN_ROWS as i32) {
        return None;
    }
    let mut b = board.clone();
    b.place(piece.kind, piece.rot, piece.x, piece.y);
    let rows = b.full_rows();
    let garbage = rows.iter().filter(|&&y| b.row(y).contains(&Some(Cell::Garbage))).count() as u32;
    b.remove_rows(&rows);
    Some(Settled { field: b, lines: rows.len() as u32, garbage, t_spin })
}

/// How high the stack (plus garbage on its way) stands above where it gets risky.
fn danger(field: &Board, pending: u32, profile: Profile) -> f32 {
    let h = heights(field);
    let max_h = *h.iter().max().unwrap_or(&0);
    // Buried holes count too: a holey stack should dig, not keep building on top.
    let holes = (0..WIDTH)
        .map(|x| ((TOTAL_ROWS - h[x] as usize)..TOTAL_ROWS).filter(|&y| field.get(x, y).is_none()).count())
        .sum::<usize>() as f32;
    let safe = if profile.stacks_for_quads { 11.0 } else { 8.0 };
    ((max_h + pending as i32) as f32 + 1.5 * holes - safe).max(0.0)
}

/// What a lock's clear is worth to this shadow, and the chain after it.
///
/// A quad builder plays like a pro: quads and T-spins (back to back) are the point, and a
/// small clear is a waste of the stack that breaks the chain, so it is only worth it to dig
/// out garbage, to clear the board, to keep a combo going, or when the stack is in danger.
fn clear_score(s: &Settled, now: Chain, pending: u32, profile: Profile) -> (f32, Chain) {
    let lines = s.lines;
    if lines == 0 {
        return (0.0, Chain { b2b: now.b2b, combo: -1 });
    }
    let quad = lines >= 4;
    let spin = s.t_spin && profile.spins;
    let difficult = quad || spin;
    let combo = (now.combo + 1) as u32;
    let perfect = s.field.is_empty();
    let attack = attack_for(lines, spin, difficult && now.b2b, if profile.spins { combo } else { 0 }, perfect) as f32;
    let after = Chain { b2b: difficult, combo: combo as i32 };
    let perfect_bonus = if perfect { 40.0 } else { 0.0 };
    let b2b_bonus = if difficult && now.b2b { 8.0 } else { 0.0 };
    if profile.stacks_for_quads && danger(&s.field, pending, profile) < 1.0 {
        let score = if spin {
            [0.0, 9.0, 28.0, 36.0][lines.min(3) as usize]
        } else if quad || perfect {
            22.0
        } else {
            // Digging garbage is fine; burning your own stack is not.
            let own = (lines - s.garbage.min(lines)) as f32;
            // Cheaper than a hole, though: a hole costs a future quad. A spinner riding a
            // combo minds it less.
            let chain_on = profile.spins && combo >= 2;
            let waste = if chain_on { 1.0 } else { 4.0 };
            1.0 * s.garbage as f32 - waste * own - if now.b2b { 5.0 } else { 0.0 }
        };
        (score + b2b_bonus + perfect_bonus + 1.5 * attack, after)
    } else {
        // Surviving (or not a planner): any clear helps, bigger ones more.
        (3.4 * lines as f32 + 1.5 * attack + perfect_bonus, after)
    }
}

/// How good a field is to keep playing on. Higher is better. A spinner judges it both as
/// a quad stack (with its well) and as a T-spin stack, and keeps whichever plan it favours.
fn field_score(b: &Board, piece: Active, pending: u32, profile: Profile) -> f32 {
    if profile.spins {
        let quad_plan = field_score_plan(b, piece, pending, profile, true);
        let spin_plan = field_score_plan(b, piece, pending, profile, false);
        return quad_plan.max(spin_plan);
    }
    field_score_plan(b, piece, pending, profile, profile.stacks_for_quads)
}

/// `field_score` for one plan: `well` keeps the right column open for quads.
fn field_score_plan(b: &Board, piece: Active, pending: u32, profile: Profile, well: bool) -> f32 {
    let h = heights(b);
    let mut holes = 0;
    let mut covered = 0;
    for x in 0..WIDTH {
        let mut roof = 0;
        for y in (TOTAL_ROWS - h[x] as usize)..TOTAL_ROWS {
            if b.get(x, y).is_some() {
                roof += 1;
            } else {
                holes += 1;
                covered += roof;
            }
        }
    }
    let quads = profile.stacks_for_quads;
    let bump_cols = if well { WELL } else { WIDTH };
    let bump: i32 = (1..bump_cols).map(|x| (h[x] - h[x - 1]).abs()).sum();
    let agg: i32 = h.iter().sum();
    // One-wide shafts (other than the quad well) only an I can fill: deep ones are deadly.
    let mut shafts = 0.0;
    for x in 0..WIDTH {
        if well && x == WELL {
            continue;
        }
        let left = if x == 0 { 99 } else { h[x - 1] };
        let right = if x == WIDTH - 1 { 99 } else { h[x + 1] };
        let depth = left.min(right) - h[x];
        if depth >= 3 {
            shafts += (depth - 2) as f32 * (depth - 2) as f32;
        }
    }
    let landing = TOTAL_ROWS as i32 - piece.y;
    let danger = danger(b, pending, profile);
    // A quad builder doesn't mind height while it's safe: it's the quad in waiting.
    let building = quads && danger < 1.0;
    let (w_agg, w_land) = if building { (0.12, 0.2) } else { (0.5, 0.6) };
    let w_hole = if quads { 11.0 } else { 7.5 };
    let mut score = -w_agg * agg as f32 - w_hole * holes as f32 - 1.6 * covered as f32 - 0.5 * bump as f32
        - w_land * landing as f32
        - 2.0 * shafts
        - 2.5 * danger * danger;
    if building && profile.spins {
        // A spinner keeps a T-spin slot open for its next T: the slot's empty cells aren't
        // holes, and a ready slot is worth a lot.
        if let Some((rows, roofed)) = t_slot(b) {
            score += (w_hole + 1.6) * roofed as f32 + if rows >= 3 { 16.0 } else { 12.0 };
        }
    }
    if building && well {
        // Keep the well open and clean, and the stack beside it ready for a quad.
        let well_depth = (h[..WELL].iter().min().copied().unwrap_or(0) - h[WELL]).max(0);
        score += 0.8 * well_depth.min(4) as f32 - 3.0 * h[WELL] as f32;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays `profile` alone for `secs` and returns the game.
    fn solo(profile: Profile, secs: f32, seed: u64) -> Game {
        let mut g = Game::new(seed);
        g.set_level(1);
        let mut bot = Bot::new(profile, seed);
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        while t < secs && !g.game_over {
            bot.update(dt, &mut g);
            g.tick(dt);
            g.take_events();
            t += dt;
        }
        g
    }

    #[test]
    fn deon_plays_at_1_6_to_1_7_pieces_per_second() {
        for seed in 1..=4 {
            let g = solo(SHADOWS[4], 180.0, seed);
            assert!(!g.game_over, "seed {seed}: Deon topped out alone");
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!((96.0..=102.0).contains(&ppm), "seed {seed}: {ppm:.1} PPM (1.6-1.7 PPS)");
        }
    }

    #[test]
    fn every_shadow_keeps_its_pace_and_survives_alone() {
        for p in SHADOWS {
            let g = solo(p, 120.0, 7);
            assert!(!g.game_over, "{} topped out", p.name);
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!(ppm >= p.ppm.0 - 3.0 && ppm <= p.ppm.1 + 1.0, "{}: {ppm:.1} PPM, wants {:?}", p.name, p.ppm);
        }
    }

    /// Deon plays like a pro: nearly every line he clears is part of a quad.
    #[test]
    fn deon_clears_with_quads_not_random_lines() {
        let (mut lines, mut quad_lines) = (0, 0);
        for seed in [1, 2, 4, 6, 8, 10] {
            let g = solo(SHADOWS[4], 180.0, seed);
            lines += g.stats.lines;
            quad_lines += 4 * g.stats.quads;
        }
        let share = quad_lines as f32 / lines as f32;
        assert!(share >= 0.9, "only {:.0}% of Deon's lines came from quads", share * 100.0);
    }

    /// Deon Extreme averages 110-120 PPM and spins: T-spins in every long game.
    #[test]
    fn deon_extreme_is_fast_and_spins() {
        let mut spins = 0;
        for seed in [1, 2, 4, 6] {
            let g = solo(SHADOWS[5], 180.0, seed);
            assert!(!g.game_over, "seed {seed}: topped out alone");
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!((110.0..=120.0).contains(&ppm), "seed {seed}: {ppm:.1} PPM");
            spins += g.stats.t_spins;
        }
        assert!(spins >= 12, "only {spins} T-spins in 12 minutes");
    }

    /// Deon Nightmare: about 1.5x Deon Extreme's pace, and perfect clears in every game.
    #[test]
    fn deon_nightmare_is_fast_and_perfect_clears() {
        let mut pcs = 0;
        for seed in [1, 2, 4, 6] {
            let g = solo(SHADOWS[6], 120.0, seed);
            assert!(!g.game_over, "seed {seed}: topped out alone");
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!((160.0..=182.0).contains(&ppm), "seed {seed}: {ppm:.1} PPM");
            pcs += g.stats.perfect_clears;
        }
        assert!(pcs >= 4, "only {pcs} perfect clears in 8 minutes");
    }

    /// Jianjie plays about 10 pieces a minute.
    #[test]
    fn jianjie_is_a_sloth() {
        for seed in 1..=3 {
            let g = solo(SHADOWS[0], 300.0, seed);
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!((8.5..=11.5).contains(&ppm), "seed {seed}: {ppm:.1} PPM");
        }
    }

    /// The beginner shadows survive on their own (beating them should take playing).
    #[test]
    fn beginner_shadows_survive_alone() {
        for p in &SHADOWS[1..4] {
            for seed in 1..=8 {
                let g = solo(*p, 180.0, seed);
                assert!(!g.game_over, "{} seed {seed}: topped out alone", p.name);
            }
        }
    }

    #[test]
    fn deon_outpaces_luoyi() {
        let lines = |p: Profile| solo(p, 120.0, 11).stats.lines;
        let (luoyi, deon) = (lines(SHADOWS[2]), lines(SHADOWS[4]));
        assert!(deon > luoyi, "Deon {deon} lines vs Luoyi {luoyi}");
    }

    #[test]
    fn candidates_include_hold_and_every_column() {
        let g = Game::new(3);
        let c = candidates(&g, SHADOWS[4]);
        assert!(c.iter().any(|c| c.actions.first() == Some(&Action::Hold)));
        assert!(c.iter().all(|c| c.actions.last() == Some(&Action::HardDrop)));
        assert!(c.len() >= 20, "{} placements", c.len());
    }
}
#[cfg(test)]
mod probe {
    use super::*;
    /// Pace, survival and attack of every shadow alone (`cargo test --release probe -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn probe_survival() {
        let list = SHADOWS.to_vec();
        for (i, p) in list.iter().enumerate() {
            for seed in 1u64..=20 {
                let mut g = Game::new(seed);
                g.set_level(1);
                let mut bot = Bot::new(*p, seed);
                let dt = 1.0 / 60.0;
                let mut t = 0.0;
                while t < 300.0 && !g.game_over {
                    bot.update(dt, &mut g);
                    g.tick(dt);
                    g.take_events();
                    t += dt;
                }
                println!("{i} {} seed {seed}: t {t:.0} over {} pieces {} lines {} quads {} small-lines {} tspins {} pcs {} apm {:.1} ppm {:.1}", p.name, g.game_over, g.stats.pieces, g.stats.lines, g.stats.quads, g.stats.lines - 4 * g.stats.quads, g.stats.t_spins, g.stats.perfect_clears, g.stats.apm().unwrap_or(0.0), g.stats.pps().unwrap_or(0.0)*60.0);
            }
        }
    }
}
#[cfg(test)]
mod spin_tests {
    use super::*;

    /// Bottom two rows full but for a T-shaped gap at columns 3-5 / 4, with an overhang at
    /// column 3 on the row above: the textbook T-spin double slot.
    fn tsd_board() -> Board {
        let mut b = Board::new();
        let bottom = TOTAL_ROWS - 1;
        for x in 0..WIDTH {
            if x != 4 {
                b.set(x, bottom, PieceKind::J);
            }
            if !(3..=5).contains(&x) {
                b.set(x, bottom - 1, PieceKind::J);
            }
        }
        // Overhang over the slot's left wing, and a wall on the right of the row above.
        b.set(3, bottom - 2, PieceKind::L);
        b.set(2, bottom - 2, PieceKind::L);
        b.set(1, bottom - 2, PieceKind::L);
        b.set(0, bottom - 2, PieceKind::L);
        b
    }

    #[test]
    fn finds_the_t_spin_double() {
        let b = tsd_board();
        let found = spin_placements(&b, spawn(PieceKind::T), &[]);
        let tsd = found.iter().find(|p| p.t_spin && settle(&b, p.piece, true).is_some_and(|s| s.lines == 2));
        assert!(tsd.is_some(), "no TSD among {} placements", found.len());
        assert!(t_slot(&b).is_some_and(|(rows, _)| rows == 2), "slot not recognised: {:?}", t_slot(&b));
    }

    #[test]
    fn finds_a_one_piece_perfect_clear() {
        // Bottom row full except columns 0-3: a flat I clears the board.
        let mut g = Game::new(5);
        for x in 4..WIDTH {
            g.board.set(x, TOTAL_ROWS - 1, PieceKind::J);
        }
        g.active = spawn(PieceKind::I);
        let keys = perfect_clear_move(&g).expect("perfect clear not found");
        for k in keys {
            g.apply(k);
        }
        assert!(g.board.is_empty(), "board not cleared");
        assert_eq!(g.stats.perfect_clears, 1);
    }

    #[test]
    fn finds_a_perfect_clear_needing_two_pieces_and_hold() {
        // Two rows, missing an O-shaped 2x2 at columns 0-1 and an I-shaped gap at 6-9 on top.
        let mut g = Game::new(5);
        let bottom = TOTAL_ROWS - 1;
        for x in 2..WIDTH {
            g.board.set(x, bottom, PieceKind::J);
        }
        for x in 2..6 {
            g.board.set(x, bottom - 1, PieceKind::J);
        }
        // Needs an O (cols 0-1, both rows) and an I (cols 6-9, top row).
        g.active = spawn(PieceKind::I);
        g.next[0] = PieceKind::O;
        assert!(perfect_clear_move(&g).is_some());
    }
}
#[cfg(test)]
mod timing {
    use super::*;
    /// Worst and average time per decision for each shadow (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn decision_time() {
        for p in SHADOWS {
            let mut g = Game::new(3);
            g.set_level(1);
            let mut bot = Bot::new(p, 3);
            let (mut worst, mut total, mut n) = (0.0f64, 0.0f64, 0);
            for _ in 0..200 {
                if g.game_over { break; }
                let t = std::time::Instant::now();
                let keys = bot.choose(&g);
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                worst = worst.max(ms);
                total += ms;
                n += 1;
                for k in keys { g.apply(k); }
                g.take_events();
            }
            println!("{:15} avg {:.2} ms  worst {:.2} ms", p.name, total / n as f64, worst);
        }
    }
}
