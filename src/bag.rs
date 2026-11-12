//! Seeded 7-bag randomiser. Deterministic for a given seed, which keeps replays
//! and (later) networked matches reproducible.

use crate::piece::PieceKind;

/// xorshift64: tiny, fast, good enough for shuffling pieces.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Mix the seed and make sure the state is never zero.
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform-ish value in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

pub struct Bag {
    rng: Rng,
    pending: Vec<PieceKind>,
}

impl Bag {
    pub fn new(seed: u64) -> Self {
        Bag {
            rng: Rng::new(seed),
            pending: Vec::with_capacity(7),
        }
    }

    pub fn next(&mut self) -> PieceKind {
        if self.pending.is_empty() {
            self.pending.extend_from_slice(&PieceKind::ALL);
            for i in (1..self.pending.len()).rev() {
                let j = self.rng.below(i + 1);
                self.pending.swap(i, j);
            }
        }
        self.pending.pop().expect("bag was just refilled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_bag_contains_each_piece_once() {
        let mut bag = Bag::new(42);
        for _ in 0..10 {
            let set: HashSet<_> = (0..7).map(|_| bag.next()).collect();
            assert_eq!(set.len(), 7);
        }
    }

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Bag::new(7), Bag::new(7));
        for _ in 0..50 {
            assert_eq!(a.next(), b.next());
        }
    }
}
