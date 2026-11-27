//! Shadow 1v1: you against a bot on its own field, exactly like a match against
//! a person. Both fields get the same pieces in the same order; whatever a clear
//! is worth after cancelling your own incoming garbage is sent to the other side;
//! the first to top out loses. Gravity creeps up after the first minute so every
//! match ends.
//!
//! Pure logic: the session feeds it the player's events and calls `update`.

use crate::bot::{Bot, Profile};
use crate::online::Msg;
use crate::party::Party;
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
    /// Online: the opponent's name. Their field is mirrored from the network into `shadow`
    /// (never ticked here) and garbage goes out through `outbox`.
    pub remote: Option<String>,
    outbox: Vec<Msg>,
    /// Party rooms: everyone else's fields and the standings (`shadow` goes unused).
    pub party: Option<Party>,
}

/// The stand-in profile for an online opponent (colour, labels; no bot, no portrait).
pub const REMOTE: Profile = Profile {
    name: "OPPONENT",
    blurb: "",
    rank: "ONLINE",
    ppm: (0.0, 0.0),
    key_gap: 0.0,
    sloppy: 0.0,
    misdrop: 0.0,
    stacks_for_quads: false,
    holds: false,
    lookahead: false,
    spins: false,
    pc_search: false,
    color: 0x7fd8ff,
    portrait: "",
};

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
            remote: None,
            outbox: Vec::new(),
            party: None,
        }
    }

    /// An online match against `name`: both fields from `seed`.
    pub fn new_remote(name: String, seed: u64, player: &mut Game) -> Self {
        Versus { remote: Some(name), ..Versus::new(REMOTE, seed, player) }
    }

    /// A party match: us (`me`) and the rest of `roster`, all from `seed`.
    pub fn new_party(me: &str, roster: &[(String, String)], seed: u64, player: &mut Game) -> Self {
        let party = Party::new(me, roster, seed);
        let rivals = party.members.len();
        let name = format!("{rivals} RIVAL{}", if rivals == 1 { "" } else { "S" });
        Versus { party: Some(party), ..Versus::new_remote(name, seed, player) }
    }

    /// Party: something arrived from `from`.
    pub fn on_party(&mut self, from: &str, msg: &Msg, player: &mut Game) {
        let fighting = self.fighting();
        let Some(party) = &mut self.party else {
            return;
        };
        match msg {
            Msg::Snap(snap) => party.on_snapshot(from, snap),
            Msg::Hit { to, lines } if fighting && *to == party.me => {
                party.attacked_by(from);
                self.received += lines;
                player.receive_garbage(*lines);
                self.since_sent = 0.0;
                self.last_sent = *lines;
                self.events.push(VsEvent::Received { lines: *lines });
            }
            Msg::Ko => {
                if party.knock_out(from) {
                    self.party_won();
                }
            }
            _ => {}
        }
    }

    /// Party: who's still in the room (players who left are out).
    pub fn party_present(&mut self, here: &[String]) {
        if let Some(party) = &mut self.party {
            if here.contains(&party.me) && party.present(here) {
                self.party_won();
            }
        }
    }

    fn party_won(&mut self) {
        if !self.over() {
            self.phase = Phase::Over { player_won: true };
            self.events.push(VsEvent::Won);
        }
    }

    /// The opponent's name as shown.
    pub fn name(&self) -> &str {
        self.remote.as_deref().unwrap_or(self.profile.name)
    }

    pub fn online(&self) -> bool {
        self.remote.is_some()
    }

    /// Messages for the other side (online), oldest first.
    pub fn take_outbox(&mut self) -> Vec<Msg> {
        std::mem::take(&mut self.outbox)
    }

    /// Something arrived from the online opponent.
    pub fn on_remote(&mut self, msg: &Msg, player: &mut Game) {
        match msg {
            Msg::Snap(snap) => snap.apply(&mut self.shadow),
            Msg::Garbage(lines) if self.fighting() => {
                self.received += lines;
                player.receive_garbage(*lines);
                self.since_sent = 0.0;
                self.last_sent = *lines;
                self.events.push(VsEvent::Received { lines: *lines });
            }
            Msg::Ko if !self.over() => {
                self.shadow.game_over = true;
                self.phase = Phase::Over { player_won: true };
                self.events.push(VsEvent::Won);
            }
            _ => {}
        }
    }

    /// The online opponent left mid-match: a win by forfeit.
    pub fn opponent_left(&mut self) {
        if !self.over() {
            self.phase = Phase::Over { player_won: true };
            self.events.push(VsEvent::Won);
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
            if let Some(party) = &mut self.party {
                if let Some(to) = party.pick_target() {
                    self.outbox.push(Msg::Hit { to, lines });
                }
            } else if self.online() {
                self.outbox.push(Msg::Garbage(lines));
            } else {
                self.shadow.receive_garbage(lines);
            }
            self.since_hit = 0.0;
            self.last_hit = lines;
            self.events.push(VsEvent::Sent { lines });
        }
    }

    pub fn update(&mut self, dt: f32, player: &mut Game) {
        if let Some(party) = &mut self.party {
            party.update(dt);
        }
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

        if self.online() {
            // Their field is theirs to play: only our own top out is decided here.
            if player.game_over {
                self.phase = Phase::Over { player_won: false };
                self.events.push(VsEvent::Lost);
                self.outbox.push(Msg::Ko);
                if let Some(party) = &mut self.party {
                    party.topped_out();
                }
            }
            return;
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
mod online_tests {
    use super::*;
    use crate::game::Action;
    use crate::online::Snapshot;

    fn fighting(name: &str) -> (Game, Versus) {
        let mut player = Game::new(5);
        let mut vs = Versus::new_remote(name.into(), 5, &mut player);
        vs.update(COUNTDOWN + 0.1, &mut player);
        (player, vs)
    }

    #[test]
    fn online_garbage_goes_out_and_comes_in() {
        let (mut player, mut vs) = fighting("Deon");
        assert_eq!(vs.name(), "Deon");
        let clear = crate::game::LineClear { rows: vec![36, 37, 38, 39], snapshot: vec![], t_spin: false, back_to_back: false, combo: 0, perfect_clear: false, attack: 4, cancelled: 0 };
        vs.on_player_event(&Event::LinesCleared(clear));
        assert_eq!(vs.take_outbox(), vec![Msg::Garbage(4)]);
        assert_eq!(vs.shadow.pending_garbage(), 0, "the mirror isn't hit locally");
        vs.on_remote(&Msg::Garbage(3), &mut player);
        assert_eq!(player.pending_garbage(), 3);
    }

    #[test]
    fn their_ko_is_our_win_and_ours_is_sent() {
        let (mut player, mut vs) = fighting("Deon");
        vs.on_remote(&Msg::Ko, &mut player);
        assert!(vs.player_won());
        let (mut player, mut vs) = fighting("Deon");
        player.game_over = true;
        vs.update(0.1, &mut player);
        assert_eq!(vs.phase, Phase::Over { player_won: false });
        assert!(vs.take_outbox().contains(&Msg::Ko));
    }

    #[test]
    fn snapshots_draw_their_field_and_leaving_forfeits() {
        let (mut player, mut vs) = fighting("Deon");
        let mut theirs = Game::new(5);
        theirs.apply(Action::HardDrop);
        vs.on_remote(&Msg::Snap(Snapshot::of(&theirs)), &mut player);
        assert_eq!(vs.shadow.stats.pieces, 1);
        vs.opponent_left();
        assert!(vs.player_won());
    }
}

#[cfg(test)]
mod party_tests {
    use super::*;

    fn roster() -> Vec<(String, String)> {
        ["a", "b", "c"].iter().map(|id| (id.to_string(), id.to_uppercase())).collect()
    }

    #[test]
    fn party_attacks_go_to_one_rival_and_kos_decide_the_match() {
        let mut player = Game::new(5);
        let mut vs = Versus::new_party("a", &roster(), 5, &mut player);
        assert_eq!(vs.name(), "2 RIVALS");
        vs.update(COUNTDOWN + 0.1, &mut player);
        let clear = crate::game::LineClear { rows: vec![36, 37], snapshot: vec![], t_spin: false, back_to_back: false, combo: 0, perfect_clear: false, attack: 2, cancelled: 0 };
        vs.on_player_event(&Event::LinesCleared(clear));
        let out = vs.take_outbox();
        assert!(matches!(&out[..], [Msg::Hit { to, lines: 2 }] if to == "b" || to == "c"), "{out:?}");
        vs.on_party("b", &Msg::Hit { to: "c".into(), lines: 4 }, &mut player);
        assert_eq!(player.pending_garbage(), 0, "not for us");
        vs.on_party("b", &Msg::Hit { to: "a".into(), lines: 4 }, &mut player);
        assert_eq!(player.pending_garbage(), 4);
        vs.on_party("b", &Msg::Ko, &mut player);
        assert!(!vs.over());
        vs.party_present(&["a".into()]);
        assert!(vs.player_won(), "the last rival left the room");
        assert_eq!(vs.party.as_ref().unwrap().my_place, Some(1));
    }

    #[test]
    fn topping_out_in_a_party_sends_ko_and_takes_a_place() {
        let mut player = Game::new(5);
        let mut vs = Versus::new_party("a", &roster(), 5, &mut player);
        vs.update(COUNTDOWN + 0.1, &mut player);
        player.game_over = true;
        vs.update(0.1, &mut player);
        assert!(vs.take_outbox().contains(&Msg::Ko));
        assert_eq!(vs.party.as_ref().unwrap().my_place, Some(3));
        assert!(!vs.player_won());
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
