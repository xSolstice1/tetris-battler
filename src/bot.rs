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
    /// UI accent colour (0xRRGGBB).
    pub color: u32,
    /// Portrait art key (assets/portraits/<key>.png).
    pub portrait: &'static str,
}

/// Easiest first.
pub const SHADOWS: [Profile; 3] = [
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
        color: 0x7fd8ff,
        portrait: "luoyi",
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
        color: 0xc061ff,
        portrait: "deon_extreme",
    },
];

/// Where pieces appear (matches `Game`'s spawn).
const SPAWN_X: i32 = 3;
const SPAWN_Y: i32 = HIDDEN_ROWS as i32 - 1;
/// A sloppy pick stays within this much of the best placement's score.
const SLOPPY_MARGIN: f32 = 6.0;
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
            self.key_timer = (total - (keys - 1.0) * self.profile.key_gap).max(0.0);
        }
        self.key_timer -= dt;
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

fn spawn(kind: PieceKind) -> Active {
    Active { kind, rot: 0, x: SPAWN_X, y: SPAWN_Y }
}

/// Every placement of the current piece and (if allowed) the hold piece, scored.
/// Shadows that look ahead also try the piece after it on each resulting field.
fn candidates(game: &Game, profile: Profile) -> Vec<Candidate> {
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
    let max_h = *heights(field).iter().max().unwrap_or(&0);
    let safe = if profile.stacks_for_quads { 11.0 } else { 8.0 };
    ((max_h + pending as i32) as f32 - safe).max(0.0)
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
            let g = solo(SHADOWS[1], 180.0, seed);
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
            let g = solo(SHADOWS[1], 180.0, seed);
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
            let g = solo(SHADOWS[2], 180.0, seed);
            assert!(!g.game_over, "seed {seed}: topped out alone");
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!((110.0..=120.0).contains(&ppm), "seed {seed}: {ppm:.1} PPM");
            spins += g.stats.t_spins;
        }
        assert!(spins >= 12, "only {spins} T-spins in 12 minutes");
    }

    #[test]
    fn deon_outpaces_luoyi() {
        let lines = |p: Profile| solo(p, 120.0, 11).stats.lines;
        let (luoyi, deon) = (lines(SHADOWS[0]), lines(SHADOWS[1]));
        assert!(deon > luoyi, "Deon {deon} lines vs Luoyi {luoyi}");
    }

    #[test]
    fn candidates_include_hold_and_every_column() {
        let g = Game::new(3);
        let c = candidates(&g, SHADOWS[1]);
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
        let mut list = SHADOWS.to_vec();
        for (i, p) in list.iter().enumerate() {
            for seed in [1u64, 2, 4, 6, 8, 10, 12, 14, 16, 18] {
                let mut g = Game::new(seed);
                g.set_level(1);
                let mut bot = Bot::new(*p, seed);
                let dt = 1.0 / 60.0;
                let mut t = 0.0;
                while t < 180.0 && !g.game_over {
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
}
