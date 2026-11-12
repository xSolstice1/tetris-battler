//! Piece shapes, rotation states and SRS wall-kick tables.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PieceKind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

/// Kick tables in the usual SRS notation: (dx, dy) with y pointing UP.
/// Indexed by the rotation state being rotated out of (0 = spawn, 1 = R, 2 = 180, 3 = L).
const JLSTZ_CW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 0 -> R
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // R -> 2
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 2 -> L
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // L -> 0
];
const JLSTZ_CCW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 0 -> L
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // R -> 0
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 2 -> R
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // L -> 2
];
const I_CW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)], // 0 -> R
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)], // R -> 2
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)], // 2 -> L
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)], // L -> 0
];
const I_CCW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)], // 0 -> L
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)], // R -> 0
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)], // 2 -> R
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)], // L -> 2
];

impl PieceKind {
    pub const ALL: [PieceKind; 7] = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ];

    /// Bounding-box size and spawn-orientation cells as (x, y), y pointing down.
    fn spawn_shape(self) -> (i32, [(i32, i32); 4]) {
        match self {
            PieceKind::I => (4, [(0, 1), (1, 1), (2, 1), (3, 1)]),
            // O sits in a 4-wide box so every piece shares the same spawn column.
            PieceKind::O => (4, [(1, 0), (2, 0), (1, 1), (2, 1)]),
            PieceKind::T => (3, [(1, 0), (0, 1), (1, 1), (2, 1)]),
            PieceKind::S => (3, [(1, 0), (2, 0), (0, 1), (1, 1)]),
            PieceKind::Z => (3, [(0, 0), (1, 0), (1, 1), (2, 1)]),
            PieceKind::J => (3, [(0, 0), (0, 1), (1, 1), (2, 1)]),
            PieceKind::L => (3, [(2, 0), (0, 1), (1, 1), (2, 1)]),
        }
    }

    /// Cells for rotation state `rot`, relative to the bounding box's top-left corner.
    pub fn cells(self, rot: u8) -> [(i32, i32); 4] {
        let (size, mut cells) = self.spawn_shape();
        if self == PieceKind::O {
            return cells;
        }
        for _ in 0..rot % 4 {
            for c in &mut cells {
                *c = (size - 1 - c.1, c.0);
            }
        }
        cells
    }

    /// Kick offsets to try, in order, when rotating out of `from`. Returned with y pointing down.
    pub fn kicks(self, from: u8, clockwise: bool) -> [(i32, i32); 5] {
        let table = match (self, clockwise) {
            (PieceKind::O, _) => return [(0, 0); 5],
            (PieceKind::I, true) => &I_CW,
            (PieceKind::I, false) => &I_CCW,
            (_, true) => &JLSTZ_CW,
            (_, false) => &JLSTZ_CCW,
        };
        table[(from % 4) as usize].map(|(x, y)| (x, -y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_rotations_return_to_spawn() {
        for kind in PieceKind::ALL {
            assert_eq!(kind.cells(0), kind.cells(4), "{kind:?}");
        }
    }

    #[test]
    fn t_rotated_clockwise_points_right() {
        let mut cells = PieceKind::T.cells(1);
        cells.sort();
        assert_eq!(cells, [(1, 0), (1, 1), (1, 2), (2, 1)]);
    }
}
