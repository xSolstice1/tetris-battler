//! Per-run performance counters (PPS, APM, VS, KPP and highlights).
//! `Game` updates everything except `key_presses`, which the input layer
//! supplies because only it can tell a real press from auto-repeat.

/// Rates are hidden until this much play time, so the first second doesn't
/// show absurd numbers like 30 PPS.
pub const MIN_TIME_FOR_RATES: f32 = 1.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Seconds of actual play (paused time doesn't count).
    pub time: f32,
    pub pieces: u32,
    pub lines: u32,
    /// Garbage lines this player's clears were worth (sent or used to cancel).
    pub attack: u32,
    /// Garbage rows this player cleared from their own field.
    pub garbage_cleared: u32,
    /// Real key presses for game actions (not held-key auto-repeat).
    pub key_presses: u32,
    pub max_combo: u32,
    /// Longest run of back-to-back bonuses.
    pub max_back_to_back: u32,
    back_to_back_run: u32,
    pub quads: u32,
    /// T-spins that cleared at least one line.
    pub t_spins: u32,
    pub perfect_clears: u32,
}

impl Stats {
    fn rate(&self, amount: f32) -> Option<f32> {
        (self.time >= MIN_TIME_FOR_RATES).then(|| amount / self.time)
    }

    /// Pieces per second.
    pub fn pps(&self) -> Option<f32> {
        self.rate(self.pieces as f32)
    }

    /// Attack per minute.
    pub fn apm(&self) -> Option<f32> {
        self.rate(self.attack as f32 * 60.0)
    }

    /// Versus score: (attack + garbage cleared) per second, x100. Rewards both
    /// offence and digging out of garbage.
    pub fn vs(&self) -> Option<f32> {
        self.rate((self.attack + self.garbage_cleared) as f32 * 100.0)
    }

    /// Key presses per piece (lower is more efficient).
    pub fn kpp(&self) -> Option<f32> {
        (self.pieces > 0).then(|| self.key_presses as f32 / self.pieces as f32)
    }

    /// Records a line clear. `back_to_back` is whether this clear earned the bonus.
    pub fn record_clear(&mut self, lines: u32, t_spin: bool, back_to_back: bool, combo: u32, perfect: bool, attack: u32) {
        if lines == 0 {
            return;
        }
        self.lines += lines;
        self.attack += attack;
        self.max_combo = self.max_combo.max(combo);
        if lines >= 4 && !t_spin {
            self.quads += 1;
        }
        if t_spin {
            self.t_spins += 1;
        }
        if perfect {
            self.perfect_clears += 1;
        }
        let difficult = lines >= 4 || t_spin;
        if back_to_back {
            self.back_to_back_run += 1;
            self.max_back_to_back = self.max_back_to_back.max(self.back_to_back_run);
        } else if !difficult {
            self.back_to_back_run = 0;
        }
    }
}

/// "m:ss" for play time.
pub fn clock(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A rate with fixed decimals, or "-" while there isn't enough data yet.
pub fn fmt_rate(value: Option<f32>, decimals: usize) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v:.decimals$}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_need_a_second_of_play() {
        let mut s = Stats { pieces: 3, attack: 2, ..Default::default() };
        s.time = 0.5;
        assert_eq!(s.pps(), None);
        s.time = 2.0;
        assert_eq!(s.pps(), Some(1.5));
        assert_eq!(s.apm(), Some(60.0));
    }

    #[test]
    fn vs_counts_attack_and_garbage_cleared() {
        let s = Stats { time: 10.0, attack: 6, garbage_cleared: 4, ..Default::default() };
        assert_eq!(s.vs(), Some(100.0));
    }

    #[test]
    fn kpp_needs_a_piece() {
        let mut s = Stats { key_presses: 9, ..Default::default() };
        assert_eq!(s.kpp(), None);
        s.pieces = 3;
        assert_eq!(s.kpp(), Some(3.0));
    }

    #[test]
    fn highlights_track_best_runs() {
        let mut s = Stats::default();
        s.record_clear(4, false, false, 0, false, 4); // first quad: no bonus yet
        s.record_clear(4, false, true, 1, false, 5); // b2b 1
        s.record_clear(2, true, true, 2, false, 5); // b2b 2 (T-spin double)
        s.record_clear(1, false, false, 3, false, 1); // single breaks the chain
        s.record_clear(4, false, false, 4, true, 14);
        assert_eq!(s.quads, 3);
        assert_eq!(s.t_spins, 1);
        assert_eq!(s.max_combo, 4);
        assert_eq!(s.max_back_to_back, 2);
        assert_eq!(s.back_to_back_run, 0);
        assert_eq!(s.perfect_clears, 1);
        assert_eq!(s.lines, 15);
        assert_eq!(s.attack, 29);
    }

    #[test]
    fn formatting() {
        assert_eq!(clock(83.9), "1:23");
        assert_eq!(fmt_rate(Some(1.234), 2), "1.23");
        assert_eq!(fmt_rate(None, 2), "-");
    }
}
