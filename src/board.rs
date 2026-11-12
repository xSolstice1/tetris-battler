//! The playfield grid. Row 0 is the top. The top `HIDDEN_ROWS` rows are a buffer
//! above the visible field (pieces spawn there, and garbage can push blocks up into it).

use crate::piece::PieceKind;

pub const WIDTH: usize = 10;
pub const VISIBLE_ROWS: usize = 20;
pub const TOTAL_ROWS: usize = 40;
pub const HIDDEN_ROWS: usize = TOTAL_ROWS - VISIBLE_ROWS;

/// What occupies a cell: part of a locked piece, or a garbage block sent by an opponent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cell {
    Piece(PieceKind),
    Garbage,
}

pub type Row = [Option<Cell>; WIDTH];

#[derive(Clone, Debug, PartialEq)]
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

    pub fn get(&self, x: usize, y: usize) -> Option<Cell> {
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
            self.rows[(y + cy) as usize][(x + cx) as usize] = Some(Cell::Piece(kind));
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

    /// Pushes `lines` garbage rows (solid except column `hole`) up from the bottom.
    /// Returns false if that shoved any block off the top of the field (a top out).
    pub fn push_garbage(&mut self, lines: usize, hole: usize) -> bool {
        let mut fits = true;
        let mut row = [Some(Cell::Garbage); WIDTH];
        row[hole % WIDTH] = None;
        for _ in 0..lines {
            if self.rows[0].iter().any(Option::is_some) {
                fits = false;
            }
            self.rows.remove(0);
            self.rows.push(row);
        }
        fits
    }

    /// Direct cell write for tests and debug board presets.
    #[cfg(any(test, debug_assertions))]
    pub fn set(&mut self, x: usize, y: usize, kind: PieceKind) {
        self.rows[y][x] = Some(Cell::Piece(kind));
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
        assert_eq!(b.get(0, bottom), Some(Cell::Piece(PieceKind::T)));
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
        assert_eq!(b.get(3, bottom), Some(Cell::Piece(PieceKind::S)));
        assert_eq!(b.get(5, bottom - 1), Some(Cell::Piece(PieceKind::Z)));
    }

    #[test]
    fn garbage_rises_from_the_bottom_with_a_hole() {
        let mut b = Board::new();
        let bottom = TOTAL_ROWS - 1;
        b.set(2, bottom, PieceKind::O);
        assert!(b.push_garbage(2, 7));
        assert_eq!(b.get(2, bottom - 2), Some(Cell::Piece(PieceKind::O)), "stack pushed up");
        for y in [bottom - 1, bottom] {
            assert_eq!(b.get(7, y), None, "hole");
            assert_eq!(b.get(0, y), Some(Cell::Garbage));
        }
        assert_eq!(b.rows.len(), TOTAL_ROWS);
    }

    #[test]
    fn garbage_pushing_blocks_off_the_top_reports_top_out() {
        let mut b = Board::new();
        b.set(4, 0, PieceKind::I);
        assert!(!b.push_garbage(1, 0));
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
