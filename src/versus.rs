//! Shadow 1v1: you against a bot on its own field, exactly like a match against
//! a person. Both fields get the same pieces in the same order; whatever a clear
//! is worth after cancelling your own incoming garbage is sent to the other side;
//! the first to top out loses. Gravity creeps up after the first minute so every
//! match ends.
//!
//! Pure logic: the session feeds it the player's events and calls `update`.

use crate::bot::{Bot, Profile};
use crate::game::{Event, Game};

/// Seconds of "3, 2, 1" before pieces fall.
pub const COUNTDOWN: f32 = 3.0;
/// Gravity starts rising after this long...
const MARGIN_TIME: f32 = 60.0;
/// ...by one level every this many seconds...
const LEVEL_EVERY: f32 = 20.0;
/// ...up to this level.
const MAX_LEVEL: u32 = 15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Countdown { left: f32 },
    Fighting,
    Over { player_won: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VsEvent {
    /// The match started (countdown done).
    Go,
    Won,
    Lost,
    /// The player's clear sent garbage to the shadow.
    Sent { lines: u32 },
    /// The shadow's clear sent garbage to the player.
    Received { lines: u32 },
}

pub struct Versus {
    pub profile: Profile,
    /// The shadow's field.
    pub shadow: Game,
    bot: Bot,
    pub phase: Phase,
    /// Seconds of play since the countdown ended.
    pub time: f32,
    /// Garbage lines each side has sent so far.
    pub sent: u32,
    pub received: u32,
    /// Seconds since the player's garbage last hit the shadow, and how much it was.
    pub since_hit: f32,
    pub last_hit: u32,
    /// Seconds since the shadow last sent garbage, and how much.
    pub since_sent: f32,
    pub last_sent: u32,
    events: Vec<VsEvent>,
}

/// Garbage a clear sends: its attack minus what went into cancelling.
fn sent_by(event: &Event) -> u32 {
    match event {
        Event::LinesCleared(c) => c.attack.saturating_sub(c.cancelled),
        _ => 0,
    }
}

fn level_at(time: f32) -> u32 {
    let ramp = ((time - MARGIN_TIME).max(0.0) / LEVEL_EVERY) as u32;
    (1 + ramp).min(MAX_LEVEL)
}

impl Versus {
    /// `player` must have been made from `seed` so both fields share their pieces.
    pub fn new(profile: Profile, seed: u64, player: &mut Game) -> Self {
        let mut shadow = Game::new(seed);
        shadow.set_level(1);
        player.set_level(1);
        shadow.take_events();
        Versus {
            profile,
            shadow,
            bot: Bot::new(profile, seed.rotate_left(29)),
            phase: Phase::Countdown { left: COUNTDOWN },
            time: 0.0,
            sent: 0,
            received: 0,
            since_hit: f32::MAX,
            last_hit: 0,
            since_sent: f32::MAX,
            last_sent: 0,
            events: Vec::new(),
        }
    }

    /// Pieces only fall once the countdown is over and until someone tops out.
    pub fn fighting(&self) -> bool {
        self.phase == Phase::Fighting
    }

    pub fn over(&self) -> bool {
        matches!(self.phase, Phase::Over { .. })
    }

    pub fn player_won(&self) -> bool {
        self.phase == Phase::Over { player_won: true }
    }

    pub fn take_events(&mut self) -> Vec<VsEvent> {
        std::mem::take(&mut self.events)
    }

    /// The player's field did something: their clears attack the shadow.
    pub fn on_player_event(&mut self, event: &Event) {
        if !self.fighting() {
            return;
        }
        let lines = sent_by(event);
        if lines > 0 {
            self.sent += lines;
            self.shadow.receive_garbage(lines);
            self.since_hit = 0.0;
            self.last_hit = lines;
            self.events.push(VsEvent::Sent { lines });
        }
    }

    pub fn update(&mut self, dt: f32, player: &mut Game) {
        match self.phase {
            Phase::Countdown { left } => {
                let left = left - dt;
                self.phase = if left <= 0.0 {
                    self.events.push(VsEvent::Go);
                    Phase::Fighting
                } else {
                    Phase::Countdown { left }
                };
                return;
            }
            Phase::Over { .. } => return,
            Phase::Fighting => {}
        }
        self.time += dt;
        self.since_hit += dt;
        self.since_sent += dt;
        let level = level_at(self.time);
        if level != self.shadow.level {
            self.shadow.set_level(level);
            player.set_level(level);
        }

        self.bot.update(dt, &mut self.shadow);
        self.shadow.tick(dt);
        for event in self.shadow.take_events() {
            let lines = sent_by(&event);
            if lines > 0 {
                self.received += lines;
                player.receive_garbage(lines);
                self.since_sent = 0.0;
                self.last_sent = lines;
                self.events.push(VsEvent::Received { lines });
            }
        }

        // Whoever tops out first loses (the player is checked first: a draw goes to the shadow).
        if player.game_over {
            self.phase = Phase::Over { player_won: false };
            self.events.push(VsEvent::Lost);
        } else if self.shadow.game_over {
            self.phase = Phase::Over { player_won: true };
            self.events.push(VsEvent::Won);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::SHADOWS;
    use crate::game::Action;

    fn start(profile: Profile) -> (Game, Versus) {
        let mut player = Game::new(42);
        let vs = Versus::new(profile, 42, &mut player);
        (player, vs)
    }

    fn run(vs: &mut Versus, player: &mut Game, secs: f32) {
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        while t < secs && !vs.over() {
            if vs.fighting() {
                player.tick(dt);
            }
            for e in player.take_events() {
                vs.on_player_event(&e);
            }
            vs.update(dt, player);
            t += dt;
        }
    }

    #[test]
    fn both_fields_get_the_same_pieces() {
        let (player, vs) = start(SHADOWS[3]);
        assert_eq!(player.active.kind, vs.shadow.active.kind);
        assert_eq!(player.next, vs.shadow.next);
    }

    #[test]
    fn nothing_moves_during_the_countdown() {
        let (mut player, mut vs) = start(SHADOWS[5]);
        run(&mut vs, &mut player, COUNTDOWN - 0.1);
        assert_eq!(vs.shadow.stats.pieces, 0);
        assert!(!vs.fighting());
        run(&mut vs, &mut player, 0.2);
        assert!(vs.fighting());
        assert_eq!(vs.take_events(), vec![VsEvent::Go]);
    }

    #[test]
    fn a_player_who_does_nothing_loses_to_the_shadow() {
        let (mut player, mut vs) = start(SHADOWS[5]);
        run(&mut vs, &mut player, 600.0);
        assert_eq!(vs.phase, Phase::Over { player_won: false });
        let events = vs.take_events();
        assert!(events.contains(&VsEvent::Lost));
        assert!(events.iter().any(|e| matches!(e, VsEvent::Received { .. })), "the shadow's garbage is reported");
    }

    #[test]
    fn the_shadow_topping_out_is_a_win() {
        let (mut player, mut vs) = start(SHADOWS[3]);
        run(&mut vs, &mut player, COUNTDOWN + 0.1);
        vs.shadow.receive_garbage(60);
        // Each lock raises up to 8 rows: a few drops bury the shadow.
        for _ in 0..10 {
            vs.shadow.apply(Action::HardDrop);
        }
        run(&mut vs, &mut player, 0.1);
        assert!(vs.player_won());
    }

    #[test]
    fn clears_send_their_attack_minus_what_they_cancelled() {
        let (mut player, mut vs) = start(SHADOWS[3]);
        run(&mut vs, &mut player, COUNTDOWN + 0.1);
        let clear = crate::game::LineClear {
            rows: vec![36, 37, 38, 39],
            snapshot: vec![],
            t_spin: false,
            back_to_back: false,
            combo: 0,
            perfect_clear: false,
            attack: 4,
            cancelled: 1,
        };
        vs.take_events();
        vs.on_player_event(&Event::LinesCleared(clear));
        assert_eq!(vs.sent, 3);
        assert_eq!(vs.shadow.pending_garbage(), 3);
        assert_eq!(vs.take_events(), vec![VsEvent::Sent { lines: 3 }]);
    }

    #[test]
    fn gravity_ramps_after_the_margin() {
        assert_eq!(level_at(0.0), 1);
        assert_eq!(level_at(MARGIN_TIME + LEVEL_EVERY * 2.5), 3);
        assert_eq!(level_at(10_000.0), MAX_LEVEL);
    }

    /// A match between two shadows ends with a winner (the stronger, usually).
    #[test]
    fn a_sharper_shadow_beats_luoyi() {
        let sharp = Profile { ppm: (240.0, 260.0), key_gap: 0.03, sloppy: 0.0, misdrop: 0.0, stacks_for_quads: true, ..SHADOWS[5] };
        let mut deon_game = Game::new(9);
        let mut vs = Versus::new(sharp, 9, &mut deon_game);
        let mut deon = Bot::new(SHADOWS[3], 99);
        let dt = 1.0 / 60.0;
        for _ in 0..(60 * 600) {
            if vs.over() {
                break;
            }
            if vs.fighting() {
                deon.update(dt, &mut deon_game);
                deon_game.tick(dt);
            }
            for e in deon_game.take_events() {
                vs.on_player_event(&e);
            }
            vs.update(dt, &mut deon_game);
        }
        assert_eq!(vs.phase, Phase::Over { player_won: false }, "the sharp shadow should win");
    }
}
#[cfg(test)]
mod duel {
    use super::*;
    use crate::bot::SHADOWS;
    /// Deon (the shadow) against Luoyi (playing as "the player"), a few seeds.
    #[test]
    fn deon_beats_luoyi() {
        let mut wins = 0;
        for seed in 1..=20u64 {
            let mut lg = Game::new(seed);
            let mut vs = Versus::new(SHADOWS[5], seed, &mut lg);
            let mut luoyi = Bot::new(SHADOWS[3], seed * 7 + 1);
            let dt = 1.0 / 60.0;
            for _ in 0..(60 * 600) {
                if vs.over() {
                    break;
                }
                if vs.fighting() {
                    luoyi.update(dt, &mut lg);
                    lg.tick(dt);
                }
                for e in lg.take_events() {
                    vs.on_player_event(&e);
                }
                vs.update(dt, &mut lg);
            }
            if !vs.player_won() {
                wins += 1;
            }
        }
        assert!(wins >= 16, "Deon won {wins}/20 against Luoyi");
    }
}
