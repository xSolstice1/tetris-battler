//! The playfield grid. Row 0 is the top. The top `HIDDEN_ROWS` rows are a buffer
//! above the visible field (pieces spawn there, and garbage can push blocks up into it).

use crate::piece::PieceKind;

pub const WIDTH: usize = 10;
pub const VISIBLE_ROWS: usize = 20;
pub const TOTAL_ROWS: usize = 40;
pub const HIDDEN_ROWS: usize = TOTAL_ROWS - VISIBLE_ROWS;

pub type Row = [Option<PieceKind>; WIDTH];

#[derive(Clone)]
pub struct Board {
    rows: Vec<Row>,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Self {
        Board {
            rows: vec![[None; WIDTH]; TOTAL_ROWS],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<PieceKind> {
        self.rows[y][x]
    }

    /// In bounds and empty. Anything outside the field counts as solid.
    pub fn is_free(&self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < WIDTH
            && (y as usize) < TOTAL_ROWS
            && self.rows[y as usize][x as usize].is_none()
    }

    pub fn fits(&self, kind: PieceKind, rot: u8, x: i32, y: i32) -> bool {
        kind.cells(rot)
            .iter()
            .all(|&(cx, cy)| self.is_free(x + cx, y + cy))
    }

    /// Writes a piece into the grid. Caller must have checked `fits`.
    pub fn place(&mut self, kind: PieceKind, rot: u8, x: i32, y: i32) {
        for (cx, cy) in kind.cells(rot) {
            self.rows[(y + cy) as usize][(x + cx) as usize] = Some(kind);
        }
    }

    /// Removes full rows, shifts everything above down, returns how many were cleared.
    pub fn clear_full_rows(&mut self) -> usize {
        let before = self.rows.len();
        self.rows.retain(|row| row.iter().any(Option::is_none));
        let cleared = before - self.rows.len();
        for _ in 0..cleared {
            self.rows.insert(0, [None; WIDTH]);
        }
        cleared
    }

    pub fn is_empty(&self) -> bool {
        self.rows.iter().flatten().all(Option::is_none)
    }

    #[cfg(test)]
    pub fn set_for_test(&mut self, x: usize, y: usize, kind: PieceKind) {
        self.rows[y][x] = Some(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_full_rows_and_drops_the_rest() {
        let mut b = Board::new();
        let bottom = TOTAL_ROWS - 1;
        for x in 0..WIDTH {
            b.rows[bottom][x] = Some(PieceKind::I);
        }
        b.rows[bottom - 1][0] = Some(PieceKind::T);

        assert_eq!(b.clear_full_rows(), 1);
        assert_eq!(b.get(0, bottom), Some(PieceKind::T));
        assert_eq!(b.get(1, bottom), None);
        assert_eq!(b.rows.len(), TOTAL_ROWS);
    }

    #[test]
    fn walls_and_floor_are_solid() {
        let b = Board::new();
        assert!(!b.is_free(-1, 0));
        assert!(!b.is_free(WIDTH as i32, 0));
        assert!(!b.is_free(0, TOTAL_ROWS as i32));
        assert!(b.is_free(0, 0));
    }
}
