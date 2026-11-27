//! Player progression: XP and level from play, a record per shadow, and the skin
//! unlocks they earn. Pure logic (no drawing, no storage; see `save` for that).

use std::collections::BTreeMap;

use crate::skins::{Item, Skin, Unlock};

/// Highest reachable level.
pub const MAX_LEVEL: u32 = 99;
/// Runs shorter than this many pieces earn nothing (no farming instant top-outs).
const MIN_PIECES: u32 = 5;

/// XP needed to go from `level` to the next.
pub fn xp_for_level(level: u32) -> u32 {
    100 + 50 * level.saturating_sub(1)
}

/// Level for a total XP, plus the XP into that level and the XP the level needs.
pub fn level_of(xp: u32) -> (u32, u32, u32) {
    let (mut level, mut left) = (1, xp);
    while level < MAX_LEVEL && left >= xp_for_level(level) {
        left -= xp_for_level(level);
        level += 1;
    }
    (level, left, xp_for_level(level))
}

/// Your history against one shadow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowRecord {
    pub wins: u32,
    pub losses: u32,
    /// Fastest win, seconds.
    pub best_time: Option<f32>,
    /// Your best APM / PPS in any match against them.
    pub best_apm: Option<f32>,
    pub best_pps: Option<f32>,
}

/// What a finished run was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RunKind {
    Classic { score: u64 },
    Battler { defeated: u32 },
    /// `shadow` is the index into `bot::SHADOWS`; `time` the match length.
    Shadow { shadow: usize, won: bool, time: f32 },
    /// An online 1v1 against another player.
    Online { won: bool },
    /// A party free-for-all: where we finished out of how many.
    Party { place: u32, players: u32 },
}

/// The numbers of a finished run that progression cares about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub kind: RunKind,
    pub lines: u32,
    pub attack: u32,
    pub pieces: u32,
    pub apm: Option<f32>,
    pub pps: Option<f32>,
}

/// What a run earned, for the results screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reward {
    pub xp: u32,
    pub level_before: u32,
    pub level: u32,
    /// XP into the level reached / XP that level needs.
    pub into: u32,
    pub need: u32,
    pub unlocks: Vec<Item>,
    /// New records worth calling out ("best score", "fastest win", ...).
    pub records: Vec<&'static str>,
}

impl Reward {
    pub fn leveled_up(&self) -> bool {
        self.level > self.level_before
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub xp: u32,
    pub classic_best: u64,
    /// Most monsters defeated in one Battler run.
    pub battle_best: u32,
    /// Keyed by the shadow's portrait key (stable across renames).
    pub records: BTreeMap<String, ShadowRecord>,
    pub skin: Skin,
    /// Handling, effects, audio and keys (the settings screen).
    pub settings: crate::settings::Settings,
}

/// XP a run is worth, before any first-win bonus.
fn run_xp(run: &Run) -> u32 {
    if run.pieces < MIN_PIECES {
        return 0;
    }
    let base = run.lines * 2 + run.attack * 3;
    let mode = match run.kind {
        RunKind::Classic { score } => (score / 1000).min(200) as u32,
        RunKind::Battler { defeated } => defeated * 25,
        RunKind::Shadow { shadow, won: true, .. } => 60 + 40 * shadow as u32,
        RunKind::Shadow { won: false, .. } => 10,
        RunKind::Online { won: true } => 120,
        RunKind::Online { won: false } => 20,
        // 150 for the win, down to 20 for last, by how many we outlasted.
        RunKind::Party { place, players } => {
            let beaten = players.saturating_sub(place.max(1)) as f32 / players.saturating_sub(1).max(1) as f32;
            if place == 1 { 150 } else { 20 + (100.0 * beaten) as u32 }
        }
    };
    base + mode
}

/// Bonus for beating a shadow for the first time.
const FIRST_WIN_XP: u32 = 100;

fn better(new: Option<f32>, old: Option<f32>, higher: bool) -> bool {
    match (new, old) {
        (Some(n), Some(o)) => if higher { n > o } else { n < o },
        (Some(_), None) => true,
        _ => false,
    }
}

impl Progress {
    pub fn level(&self) -> u32 {
        level_of(self.xp).0
    }

    pub fn record(&self, shadow_key: &str) -> ShadowRecord {
        self.records.get(shadow_key).copied().unwrap_or_default()
    }

    pub fn unlocked(&self, item: Item) -> bool {
        match item.unlock() {
            Unlock::Free => true,
            Unlock::Level(n) => self.level() >= n,
            Unlock::Beat(key) => self.record(key).wins > 0,
        }
    }

    fn unlocked_items(&self) -> Vec<Item> {
        Item::all().filter(|&i| self.unlocked(i)).collect()
    }

    /// Takes off anything equipped that isn't unlocked (e.g. an edited save).
    pub fn sanitize(&mut self) {
        let d = Skin::default();
        let s = self.skin;
        for (item, fallback) in [
            (Item::Style(s.style), Item::Style(d.style)),
            (Item::Theme(s.theme), Item::Theme(d.theme)),
            (Item::Palette(s.palette), Item::Palette(d.palette)),
        ] {
            if !self.unlocked(item) {
                self.skin = fallback.apply(self.skin);
            }
        }
    }

    /// Equips `item` if it's unlocked. Returns whether it was.
    pub fn equip(&mut self, item: Item) -> bool {
        let ok = self.unlocked(item);
        if ok {
            self.skin = item.apply(self.skin);
        }
        ok
    }

    /// Books a finished run. `shadow_key` is the portrait key of the shadow faced (Shadow runs).
    pub fn finish(&mut self, run: &Run, shadow_key: Option<&str>) -> Reward {
        let before_items = self.unlocked_items();
        let level_before = self.level();
        let mut xp = run_xp(run);
        let mut records = Vec::new();
        match run.kind {
            RunKind::Classic { score } => {
                if score > self.classic_best && score > 0 {
                    if self.classic_best > 0 {
                        records.push("NEW BEST SCORE");
                    }
                    self.classic_best = score;
                }
            }
            RunKind::Battler { defeated } => {
                if defeated > self.battle_best {
                    if self.battle_best > 0 {
                        records.push("MOST MONSTERS YET");
                    }
                    self.battle_best = defeated;
                }
            }
            // Online results only earn XP (no per-player records yet).
            RunKind::Online { .. } | RunKind::Party { .. } => {}
            RunKind::Shadow { won, time, .. } => {
                let key = shadow_key.unwrap_or("unknown").to_owned();
                let r = self.records.entry(key).or_default();
                if won {
                    if r.wins == 0 {
                        xp += FIRST_WIN_XP;
                        records.push("FIRST WIN");
                    } else if better(Some(time), r.best_time, false) {
                        records.push("FASTEST WIN");
                    }
                    r.wins += 1;
                    if better(Some(time), r.best_time, false) {
                        r.best_time = Some(time);
                    }
                } else {
                    r.losses += 1;
                }
                // Rates only count from real matches.
                if run.pieces >= MIN_PIECES {
                    if better(run.apm, r.best_apm, true) {
                        r.best_apm = run.apm;
                    }
                    if better(run.pps, r.best_pps, true) {
                        r.best_pps = run.pps;
                    }
                }
            }
        }
        self.xp = self.xp.saturating_add(xp);
        let (level, into, need) = level_of(self.xp);
        let unlocks = self.unlocked_items().into_iter().filter(|i| !before_items.contains(i)).collect();
        Reward { xp, level_before, level, into, need, unlocks, records }
    }
}

/// How to unlock `item`, for a locked item's description.
pub fn unlock_hint(item: Item) -> String {
    match item.unlock() {
        Unlock::Free => "free".to_owned(),
        Unlock::Level(n) => format!("reach level {n}"),
        Unlock::Beat(key) => {
            let name = crate::bot::SHADOWS.iter().find(|p| p.portrait == key).map_or(key, |p| p.name);
            format!("beat {name}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skins::{BlockStyle, BoardTheme, Palette};

    fn run(kind: RunKind) -> Run {
        Run { kind, lines: 10, attack: 4, pieces: 30, apm: Some(20.0), pps: Some(1.0) }
    }

    #[test]
    fn levels_follow_the_curve() {
        assert_eq!(level_of(0), (1, 0, 100));
        assert_eq!(level_of(99), (1, 99, 100));
        assert_eq!(level_of(100), (2, 0, 150));
        assert_eq!(level_of(250), (3, 0, 200));
        assert_eq!(level_of(u32::MAX).0, MAX_LEVEL);
    }

    #[test]
    fn xp_comes_from_lines_attack_and_the_mode() {
        let mut p = Progress::default();
        let r = p.finish(&run(RunKind::Classic { score: 5000 }), None);
        assert_eq!(r.xp, 10 * 2 + 4 * 3 + 5);
        let r = p.finish(&run(RunKind::Battler { defeated: 2 }), None);
        assert_eq!(r.xp, 32 + 50);
        // A loss is worth a little; a first win a lot more.
        let loss = p.finish(&run(RunKind::Shadow { shadow: 0, won: false, time: 60.0 }), Some("jianjie"));
        assert_eq!(loss.xp, 32 + 10);
        let win = p.finish(&run(RunKind::Shadow { shadow: 0, won: true, time: 60.0 }), Some("jianjie"));
        assert_eq!(win.xp, 32 + 60 + FIRST_WIN_XP);
        let again = p.finish(&run(RunKind::Shadow { shadow: 0, won: true, time: 50.0 }), Some("jianjie"));
        assert_eq!(again.xp, 32 + 60);
        assert_eq!(again.records, vec!["FASTEST WIN"]);
    }

    #[test]
    fn instant_top_outs_earn_nothing() {
        let mut p = Progress::default();
        let r = p.finish(&Run { pieces: 2, ..run(RunKind::Classic { score: 0 }) }, None);
        assert_eq!(r.xp, 0);
        assert_eq!(p.xp, 0);
    }

    #[test]
    fn shadow_records_track_wins_losses_and_bests() {
        let mut p = Progress::default();
        p.finish(&run(RunKind::Shadow { shadow: 5, won: false, time: 90.0 }), Some("deon"));
        p.finish(&Run { apm: Some(35.0), pps: Some(0.8), ..run(RunKind::Shadow { shadow: 5, won: true, time: 80.0 }) }, Some("deon"));
        p.finish(&Run { apm: Some(30.0), pps: Some(1.4), ..run(RunKind::Shadow { shadow: 5, won: true, time: 100.0 }) }, Some("deon"));
        let r = p.record("deon");
        assert_eq!((r.wins, r.losses), (2, 1));
        assert_eq!(r.best_time, Some(80.0));
        assert_eq!(r.best_apm, Some(35.0));
        assert_eq!(r.best_pps, Some(1.4));
        assert_eq!(p.record("luoyi"), ShadowRecord::default());
    }

    #[test]
    fn beating_a_shadow_unlocks_its_skin_once() {
        let mut p = Progress::default();
        assert!(!p.unlocked(Item::Style(BlockStyle::Deon)));
        let r = p.finish(&run(RunKind::Shadow { shadow: 5, won: true, time: 80.0 }), Some("deon"));
        assert!(r.unlocks.contains(&Item::Style(BlockStyle::Deon)));
        assert!(p.unlocked(Item::Style(BlockStyle::Deon)));
        let r = p.finish(&run(RunKind::Shadow { shadow: 5, won: true, time: 70.0 }), Some("deon"));
        assert!(!r.unlocks.contains(&Item::Style(BlockStyle::Deon)));
    }

    #[test]
    fn levelling_up_unlocks_level_items() {
        let mut p = Progress { xp: 99, ..Default::default() };
        let r = p.finish(&run(RunKind::Classic { score: 0 }), None);
        assert!(r.leveled_up());
        assert_eq!(r.unlocks, vec![Item::Style(BlockStyle::Flat)]);
    }

    #[test]
    fn locked_items_cannot_be_equipped_and_are_taken_off() {
        let mut p = Progress::default();
        assert!(!p.equip(Item::Theme(BoardTheme::Inferno)));
        assert!(p.equip(Item::Palette(Palette::Contrast)));
        assert_eq!(p.skin.palette, Palette::Contrast);
        p.skin.style = BlockStyle::Neon;
        p.sanitize();
        assert_eq!(p.skin.style, BlockStyle::Classic);
        assert_eq!(p.skin.palette, Palette::Contrast);
    }

    #[test]
    fn hints_name_the_shadow() {
        assert_eq!(unlock_hint(Item::Theme(BoardTheme::Inferno)), "beat DEON NIGHTMARE");
        assert_eq!(unlock_hint(Item::Style(BlockStyle::Neon)), "reach level 15");
    }
}
