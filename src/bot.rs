//! Shadow opponents: bots that play a real `Game` through the same actions a
//! player has (rotate, move, hold, hard drop), at a set pace and skill.
//!
//! Each piece the bot looks at every reachable placement of the current piece
//! and of the hold piece, scores the field each would leave (holes, height,
//! bumpiness, lines and their attack value), and plays the best. Weaker shadows
//! sometimes settle for a worse spot or misdrop. Pure logic, no rendering.

use crate::bag::Rng;
use crate::board::{Board, HIDDEN_ROWS, TOTAL_ROWS, WIDTH};
use crate::game::{Action, Active, Game, attack_for};

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
    /// UI accent colour (0xRRGGBB).
    pub color: u32,
}

pub const SHADOWS: [Profile; 4] = [
    Profile {
        name: "DEON",
        blurb: "Steady and patient. Clears what he can, rarely panics.",
        rank: "AVERAGE",
        ppm: (66.0, 74.0),
        key_gap: 0.07,
        sloppy: 0.18,
        misdrop: 0.04,
        stacks_for_quads: false,
        holds: true,
        color: 0x5cdb5c,
    },
    Profile {
        name: "MIRA",
        blurb: "Keeps a clean stack and starts sending quads.",
        rank: "SKILLED",
        ppm: (130.0, 145.0),
        key_gap: 0.055,
        sloppy: 0.1,
        misdrop: 0.02,
        stacks_for_quads: true,
        holds: true,
        color: 0x4f8bff,
    },
    Profile {
        name: "KAITO",
        blurb: "Fast hands, heavy attack. Punishes a messy board.",
        rank: "EXPERT",
        ppm: (175.0, 190.0),
        key_gap: 0.04,
        sloppy: 0.05,
        misdrop: 0.01,
        stacks_for_quads: true,
        holds: true,
        color: 0xff9a3c,
    },
    Profile {
        name: "VEX",
        blurb: "A relentless shadow. Almost never makes a mistake.",
        rank: "MASTER",
        ppm: (240.0, 260.0),
        key_gap: 0.03,
        sloppy: 0.02,
        misdrop: 0.0,
        stacks_for_quads: true,
        holds: true,
        color: 0xff4f6d,
    },
];

/// Where pieces appear (matches `Game`'s spawn).
const SPAWN_X: i32 = 3;
const SPAWN_Y: i32 = HIDDEN_ROWS as i32 - 1;
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
            let keys = self.plan.len().max(1) as f32;
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
            self.rng.below(options.len().min(3))
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

/// Every placement reachable by rotating, sliding and hard-dropping, for the
/// current piece and (if allowed) the hold piece.
fn candidates(game: &Game, profile: Profile) -> Vec<Candidate> {
    let mut out = Vec::new();
    let pending = game.pending_garbage();
    let mut starts = vec![(game.active, Vec::new())];
    if profile.holds && !game.hold_used {
        let kind = game.hold.unwrap_or(game.next[0]);
        if kind != game.active.kind {
            let a = Active { kind, rot: 0, x: SPAWN_X, y: SPAWN_Y };
            if game.board.fits(kind, 0, a.x, a.y) {
                starts.push((a, vec![Action::Hold]));
            }
        }
    }
    for (start, prefix) in starts {
        // Rotation states: none, cw, cw+cw, ccw.
        let turns: [&[bool]; 4] = [&[], &[true], &[true, true], &[false]];
        for turn in turns {
            let mut a = start;
            let mut keys = prefix.clone();
            let mut ok = true;
            for &cw in turn {
                match rotate(&game.board, a, cw) {
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
                let landed = Active { y: drop_y(&game.board, s), ..s };
                out.push(Candidate { score: evaluate(&game.board, landed, pending, profile), actions: keys });
            };
            push(a, keys.clone());
            for dir in [-1, 1] {
                let mut s = a;
                let mut slide = keys.clone();
                while game.board.fits(s.kind, s.rot, s.x + dir, s.y) {
                    s.x += dir;
                    slide.push(if dir < 0 { Action::MoveLeft } else { Action::MoveRight });
                    push(s, slide.clone());
                }
            }
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

/// How good the field is after `piece` lands. Higher is better.
fn evaluate(board: &Board, piece: Active, pending: u32, profile: Profile) -> f32 {
    let mut b = board.clone();
    // A placement entirely above the visible field would lock out.
    if piece.cells().iter().all(|&(_, y)| y < HIDDEN_ROWS as i32) {
        return -1e6;
    }
    b.place(piece.kind, piece.rot, piece.x, piece.y);
    let rows = b.full_rows();
    let lines = rows.len() as u32;
    b.remove_rows(&rows);

    let h = heights(&b);
    let max_h = *h.iter().max().unwrap_or(&0);
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
    let bump_cols = if quads { WELL } else { WIDTH };
    let bump: i32 = (1..bump_cols).map(|x| (h[x] - h[x - 1]).abs()).sum();
    let agg: i32 = h.iter().sum();
    // One-wide shafts (other than the quad well) only an I can fill: deep ones are deadly.
    let mut shafts = 0.0;
    for x in 0..WIDTH {
        if quads && x == WELL {
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

    // Danger rises sharply as the stack nears the top; garbage on the way adds to it.
    let danger = ((max_h + pending as i32) as f32 - 8.0).max(0.0);
    let mut score = -0.5 * agg as f32 - 7.5 * holes as f32 - 1.2 * covered as f32 - 0.5 * bump as f32
        - 0.6 * landing as f32
        - 2.0 * shafts
        - 2.5 * danger * danger;

    let attack = attack_for(lines, false, false, 0, b.is_empty()) as f32;
    if quads && danger < 1.0 {
        // Building: keep the well open and clean; only quads are worth clearing.
        let well_depth = (h[..WELL].iter().min().copied().unwrap_or(0) - h[WELL]).max(0);
        score += 0.8 * well_depth.min(4) as f32 - 3.0 * h[WELL] as f32;
        score += match lines {
            0 => 0.0,
            4 => 14.0,
            n => -3.0 + n as f32,
        };
    } else {
        // Surviving (or not a planner): any clear helps, bigger ones more.
        score += 3.4 * lines as f32 + 1.5 * attack;
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
    fn deon_plays_at_about_70_pieces_per_minute() {
        for seed in 1..=4 {
            let g = solo(SHADOWS[0], 180.0, seed);
            assert!(!g.game_over, "seed {seed}: Deon topped out alone");
            let ppm = g.stats.pps().unwrap() * 60.0;
            assert!((66.0..=74.0).contains(&ppm), "seed {seed}: {ppm:.1} PPM");
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

    #[test]
    fn stronger_shadows_attack_harder() {
        let apm = |p: Profile| solo(p, 120.0, 11).stats.apm().unwrap();
        let (deon, vex) = (apm(SHADOWS[0]), apm(SHADOWS[3]));
        assert!(vex > deon * 1.5, "Vex {vex:.1} APM vs Deon {deon:.1}");
    }

    #[test]
    fn candidates_include_hold_and_every_column() {
        let g = Game::new(3);
        let c = candidates(&g, SHADOWS[0]);
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
        list.push(Profile { sloppy: 0.0, misdrop: 0.0, name: "PERFECT", ..SHADOWS[3] });
        for (i, p) in list.iter().enumerate() {
            for seed in [1u64, 2, 4, 6, 8, 10] {
                let mut g = Game::new(seed);
                g.set_level(1);
                let mut bot = Bot::new(*p, seed);
                let dt = 1.0 / 60.0;
                let mut t = 0.0;
                while t < 120.0 && !g.game_over {
                    bot.update(dt, &mut g);
                    g.tick(dt);
                    g.take_events();
                    t += dt;
                }
                println!("{i} {} seed {seed}: t {t:.0} over {} pieces {} lines {} quads {} apm {:.1} ppm {:.1}", p.name, g.game_over, g.stats.pieces, g.stats.lines, g.stats.quads, g.stats.apm().unwrap_or(0.0), g.stats.pps().unwrap_or(0.0)*60.0);
            }
        }
    }
}
