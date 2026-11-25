//! How close a field is to topping out, for the danger warning (a red pulse, a
//! heartbeat and the next piece's outline at its spawn), in the spirit of how
//! competitive stackers warn you before the end instead of it coming out of nowhere.
//!
//! Pure: reads a `Game`, no drawing or sound.

use crate::board::{HIDDEN_ROWS, TOTAL_ROWS, VISIBLE_ROWS, WIDTH};
use crate::game::Game;

/// The warning starts once the stack (plus garbage on its way) is this many rows high...
const WARN_FROM: f32 = 13.0;
/// ...and is at full strength here.
const WARN_FULL: f32 = 18.0;
/// Where pieces spawn (matches `Game`).
const SPAWN_X: i32 = 3;
const SPAWN_Y: i32 = HIDDEN_ROWS as i32 - 1;

/// Rows filled from the floor up to the highest block.
pub fn stack_height(game: &Game) -> u32 {
    (0..TOTAL_ROWS)
        .find(|&y| (0..WIDTH).any(|x| game.board.get(x, y).is_some()))
        .map_or(0, |y| (TOTAL_ROWS - y) as u32)
}

/// 0 (safe) to 1 (about to top out). Incoming garbage counts as the rows it will add,
/// and a next piece with nowhere to spawn is full danger.
pub fn level(game: &Game) -> f32 {
    if game.game_over {
        return 0.0;
    }
    if next_spawn_blocked(game) {
        return 1.0;
    }
    let rows = stack_height(game) as f32 + game.pending_garbage().min(VISIBLE_ROWS as u32) as f32;
    ((rows - WARN_FROM) / (WARN_FULL - WARN_FROM)).clamp(0.0, 1.0)
}

/// The next piece would collide where it spawns (shown as a red outline there).
pub fn next_spawn_blocked(game: &Game) -> bool {
    !game.game_over && !game.board.fits(game.next[0], 0, SPAWN_X, SPAWN_Y)
}

/// Seconds between heartbeats at a danger level (faster as it rises).
pub fn beat_interval(level: f32) -> f32 {
    0.95 - 0.5 * level.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::PieceKind;

    fn stacked(rows: usize) -> Game {
        let mut g = Game::new(1);
        for y in TOTAL_ROWS - rows..TOTAL_ROWS {
            for x in 1..WIDTH {
                g.board.set(x, y, PieceKind::J);
            }
        }
        g
    }

    #[test]
    fn a_low_stack_is_safe_and_a_tall_one_is_danger() {
        assert_eq!(level(&stacked(4)), 0.0);
        let mid = level(&stacked(15));
        assert!(mid > 0.0 && mid < 1.0, "{mid}");
        assert_eq!(level(&stacked(19)), 1.0);
    }

    #[test]
    fn incoming_garbage_raises_the_danger() {
        let mut g = stacked(10);
        assert_eq!(level(&g), 0.0);
        g.receive_garbage(6);
        assert!(level(&g) > 0.5);
    }

    #[test]
    fn a_blocked_spawn_is_full_danger() {
        let mut g = Game::new(1);
        for x in 0..WIDTH {
            for y in SPAWN_Y as usize..SPAWN_Y as usize + 2 {
                g.board.set(x, y, PieceKind::J);
            }
        }
        assert!(next_spawn_blocked(&g));
        assert_eq!(level(&g), 1.0);
    }

    #[test]
    fn the_heartbeat_quickens() {
        assert!(beat_interval(1.0) < beat_interval(0.2));
    }
}
