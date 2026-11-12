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

    pub fn row(&self, y: usize) -> Row {
        self.rows[y]
    }

    /// Indices of completely filled rows, top to bottom.
    pub fn full_rows(&self) -> Vec<usize> {
        (0..TOTAL_ROWS)
            .filter(|&y| self.rows[y].iter().all(Option::is_some))
            .collect()
    }

    /// Removes the given rows (ascending order) and shifts everything above them down.
    pub fn remove_rows(&mut self, ys: &[usize]) {
        for &y in ys.iter().rev() {
            self.rows.remove(y);
        }
        for _ in ys {
            self.rows.insert(0, [None; WIDTH]);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rows.iter().flatten().all(Option::is_none)
    }

    /// Direct cell write for tests and debug board presets.
    #[cfg(any(test, debug_assertions))]
    pub fn set(&mut self, x: usize, y: usize, kind: PieceKind) {
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
            b.set(x, bottom, PieceKind::I);
        }
        b.set(0, bottom - 1, PieceKind::T);

        let full = b.full_rows();
        assert_eq!(full, vec![bottom]);
        b.remove_rows(&full);
        assert_eq!(b.get(0, bottom), Some(PieceKind::T));
        assert_eq!(b.get(1, bottom), None);
        assert_eq!(b.rows.len(), TOTAL_ROWS);
    }

    #[test]
    fn removes_non_adjacent_rows() {
        let mut b = Board::new();
        let bottom = TOTAL_ROWS - 1;
        for x in 0..WIDTH {
            b.set(x, bottom, PieceKind::I);
            b.set(x, bottom - 2, PieceKind::I);
        }
        b.set(3, bottom - 1, PieceKind::S); // survives between the two full rows
        b.set(5, bottom - 3, PieceKind::Z); // survives above both

        let full = b.full_rows();
        assert_eq!(full, vec![bottom - 2, bottom]);
        b.remove_rows(&full);
        assert_eq!(b.get(3, bottom), Some(PieceKind::S));
        assert_eq!(b.get(5, bottom - 1), Some(PieceKind::Z));
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
