//! What the music should be doing right now, read from the game: which track,
//! how intense, how dangerous. Pure, so the mapping is unit tested; `player`
//! turns a `Mood` into stem volumes.

use crate::battle::Battle;
use crate::game::Game;
use crate::versus::Versus;

use super::music::Song;
use super::songs;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Track {
    Menu,
    Classic,
    Battler,
    /// Shadow 1v1, by how hard the shadow is: 0 gentle, 1 mid, 2 the hardest.
    Shadow(u8),
}

impl Track {
    /// Shadow `index` out of `count` (easiest first): bottom three get the calm
    /// groove, the top two the nightmare, the rest the rival theme.
    pub fn shadow(index: usize, count: usize) -> Self {
        let from_top = count.saturating_sub(index + 1);
        Track::Shadow(if from_top < 2 {
            2
        } else if index < 3 {
            0
        } else {
            1
        })
    }

    pub fn song(self) -> &'static Song {
        match self {
            Track::Menu => &songs::LOBBY,
            Track::Classic => &songs::STACK_ATTACK,
            Track::Battler => &songs::MONSTER_RUSH,
            Track::Shadow(0) => &songs::SPARRING,
            Track::Shadow(1) => &songs::RIVAL,
            Track::Shadow(_) => &songs::NIGHTMARE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Won,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mood {
    pub track: Track,
    /// 0 (cruising) to 1 (on fire): combos, back-to-back, garbage flying.
    pub intensity: f32,
    /// 0 (safe) to 1 (about to top out, or out of time).
    pub danger: f32,
    pub paused: bool,
    /// Before the action starts (a 1v1 countdown): no drums yet.
    pub holding: bool,
    pub ended: Option<Ending>,
}

impl Mood {
    pub fn menu() -> Self {
        Mood {
            track: Track::Menu,
            intensity: 0.0,
            danger: 0.0,
            paused: false,
            holding: false,
            ended: None,
        }
    }

    /// The mood of a play session.
    pub fn play(track: Track, game: &Game, battle: Option<&Battle>, versus: Option<&Versus>, paused: bool) -> Self {
        let ended = if let Some(vs) = versus.filter(|v| v.over()) {
            Some(if vs.player_won() { Ending::Won } else { Ending::Lost })
        } else if game.game_over || battle.is_some_and(Battle::over) {
            Some(Ending::Lost)
        } else {
            None
        };
        let holding = versus.is_some_and(|v| !v.fighting() && !v.over());

        // Chaining clears: combo -1 means the last piece cleared nothing.
        let combo = (game.combo() + 1).max(0) as f32;
        let mut intensity = (combo * 0.13).min(0.6);
        if game.back_to_back() {
            intensity += 0.3;
        }
        let mut danger = if ended.is_none() { crate::danger::level(game) } else { 0.0 };

        match (battle, versus) {
            (Some(b), _) => {
                // Later monsters push harder; a monster nearly down is a moment.
                intensity += ((b.monster.tier.saturating_sub(1)) as f32 * 0.06).min(0.3);
                if b.hp * 4 < b.monster.max_hp {
                    intensity += 0.2;
                }
                // Running out of time is a danger of its own.
                const TIME_WARN: f32 = 15.0;
                if b.time_left < TIME_WARN && ended.is_none() {
                    danger = danger.max((1.0 - b.time_left / TIME_WARN) * 0.8);
                }
            }
            (None, Some(vs)) => intensity += heat(vs),
            // Classic: it builds as the levels go up.
            (None, None) => intensity += ((game.level.saturating_sub(1)) as f32 * 0.05).min(0.35),
        }

        Mood {
            track,
            intensity: intensity.clamp(0.0, 1.0),
            danger: danger.clamp(0.0, 1.0),
            paused,
            holding,
            ended,
        }
    }
}

/// How hot a 1v1 is: garbage just sent either way (fading over a few seconds),
/// garbage queued against the shadow, the shadow near the top, and the late game.
fn heat(vs: &Versus) -> f32 {
    const FADE: f32 = 4.0;
    let recent = |since: f32, lines: u32| {
        if since < FADE {
            (lines.min(6) as f32 * 0.12) * (1.0 - since / FADE)
        } else {
            0.0
        }
    };
    recent(vs.since_hit, vs.last_hit)
        + recent(vs.since_sent, vs.last_sent)
        + (vs.shadow.pending_garbage() as f32 * 0.05).min(0.3)
        + crate::danger::level(&vs.shadow) * 0.35
        + ((vs.time - 60.0) / 120.0).clamp(0.0, 1.0) * 0.25
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::SHADOWS;

    #[test]
    fn shadows_get_tracks_by_difficulty() {
        let n = SHADOWS.len();
        let tracks: Vec<Track> = (0..n).map(|i| Track::shadow(i, n)).collect();
        let name = |s: &str| SHADOWS.iter().position(|p| p.name == s).unwrap();
        assert_eq!(tracks[name("JIANJIE THE SLOTH")], Track::Shadow(0));
        assert_eq!(tracks[name("LUOYI ANGEL")], Track::Shadow(0));
        assert_eq!(tracks[name("DEON")], Track::Shadow(1));
        assert_eq!(tracks[name("DEON EXTREME")], Track::Shadow(2));
        assert_eq!(tracks[name("DEON NIGHTMARE")], Track::Shadow(2));
    }

    #[test]
    fn a_fresh_game_is_calm_and_safe() {
        let g = Game::new(1);
        let m = Mood::play(Track::Classic, &g, None, None, false);
        assert_eq!(m.intensity, 0.0);
        assert_eq!(m.danger, 0.0);
        assert_eq!(m.ended, None);
    }

    #[test]
    fn a_1v1_holds_during_the_countdown() {
        let mut g = Game::new(7);
        let vs = Versus::new(SHADOWS[0], 7, &mut g);
        let m = Mood::play(Track::Shadow(0), &g, None, Some(&vs), false);
        assert!(m.holding);
    }

    #[test]
    fn topping_out_ends_the_music_with_a_loss() {
        let mut g = Game::new(1);
        g.game_over = true;
        let m = Mood::play(Track::Classic, &g, None, None, false);
        assert_eq!(m.ended, Some(Ending::Lost));
        assert_eq!(m.danger, 0.0);
    }

    #[test]
    fn incoming_garbage_is_danger() {
        let mut g = Game::new(1);
        g.receive_garbage(18);
        let m = Mood::play(Track::Classic, &g, None, None, false);
        assert!(m.danger > 0.5, "{}", m.danger);
    }
}
