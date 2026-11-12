//! Debug-build-only board presets for triggering effects on demand.
//!
//! F1  quad + perfect clear: rotate, hard drop into the right column
//! F2  T-spin double: rotate cw once, soft drop to the bottom, rotate cw again
//! F3  combo ladder: keep dropping pieces into the right column to chain combos
//! F4  receive 3 garbage lines (they rise on your next lock that clears nothing)
//! F5  (Battler) skip to the next monster

use macroquad::prelude::*;

use crate::board::{Board, TOTAL_ROWS, WIDTH};
use crate::game::Game;
use crate::piece::PieceKind;

const BOTTOM: usize = TOTAL_ROWS - 1;

fn fill_row_except(board: &mut Board, y: usize, gaps: &[usize]) {
    for x in (0..WIDTH).filter(|x| !gaps.contains(x)) {
        board.set(x, y, PieceKind::J);
    }
}

fn quad_perfect_clear() -> Board {
    let mut b = Board::new();
    for y in BOTTOM - 3..=BOTTOM {
        fill_row_except(&mut b, y, &[WIDTH - 1]);
    }
    b
}

/// Slot for a T pointing down at columns 3-5, with an overhang at column 3.
fn t_spin_double() -> Board {
    let mut b = Board::new();
    fill_row_except(&mut b, BOTTOM, &[4]);
    fill_row_except(&mut b, BOTTOM - 1, &[3, 4, 5]);
    for x in 0..=3 {
        b.set(x, BOTTOM - 2, PieceKind::L);
    }
    b
}

/// Eight rows, each one cell short in the rightmost column: drop vertical
/// pieces in to chain combos.
fn combo_ladder() -> Board {
    let mut b = Board::new();
    for y in BOTTOM - 7..=BOTTOM {
        fill_row_except(&mut b, y, &[WIDTH - 1]);
    }
    b
}

pub fn handle_keys(game: &mut Game) {
    if is_key_pressed(KeyCode::F4) {
        game.receive_garbage(3);
    }
    let preset = if is_key_pressed(KeyCode::F1) {
        Some((quad_perfect_clear(), PieceKind::I))
    } else if is_key_pressed(KeyCode::F2) {
        Some((t_spin_double(), PieceKind::T))
    } else if is_key_pressed(KeyCode::F3) {
        Some((combo_ladder(), PieceKind::I))
    } else {
        None
    };
    if let Some((board, kind)) = preset {
        game.load_preset(board, kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Action, Event};

    fn clear_events(game: &mut Game) -> Vec<(u32, bool, bool)> {
        game.take_events()
            .into_iter()
            .filter_map(|e| match e {
                Event::LinesCleared(c) => Some((c.lines(), c.t_spin, c.perfect_clear)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn f2_preset_follows_documented_inputs_to_a_t_spin_double() {
        let mut g = Game::new(1);
        g.load_preset(t_spin_double(), PieceKind::T);
        g.apply(Action::RotateCw);
        for _ in 0..30 {
            g.apply(Action::SoftDrop);
        }
        g.apply(Action::RotateCw);
        g.apply(Action::HardDrop);
        assert_eq!(clear_events(&mut g), vec![(2, true, false)]);
    }

    #[test]
    fn f1_preset_gives_quad_perfect_clear() {
        let mut g = Game::new(1);
        g.load_preset(quad_perfect_clear(), PieceKind::I);
        g.apply(Action::RotateCw);
        for _ in 0..10 {
            g.apply(Action::MoveRight);
        }
        g.apply(Action::HardDrop);
        assert_eq!(clear_events(&mut g), vec![(4, false, true)]);
    }
}
