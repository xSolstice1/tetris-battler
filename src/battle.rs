//! Battler mode: a gauntlet of monsters. Each has HP and a time limit; your clears
//! damage it (and still cancel its garbage), while it periodically sends garbage
//! at you. Beat it before time runs out to face the next, tougher one. The run
//! ends on a timeout or a top out.
//!
//! Pure logic: driven by game events and `update`, no rendering or input.

use crate::monster_art::{self, Art};
use crate::game::{Event, Game, LineClear};

/// Breather between monsters, seconds.
pub const INTERMISSION: f32 = 2.0;
const DAMAGE_PER_LINE: u32 = 1;
const DAMAGE_PER_ATTACK: u32 = 2;
/// How long a damage number stays on screen.
pub const HIT_POPUP_TIME: f32 = 0.9;

/// Shield monsters start with a shield worth this share of their max HP.
const SHIELD_FRACTION: f32 = 0.4;
const FOG_EVERY: f32 = 12.0;
const FOG_TIME: f32 = 5.0;
const HASTE_EVERY: f32 = 14.0;
const HASTE_TIME: f32 = 4.0;
/// Gravity speed multiplier while Haste is active.
const HASTE_SCALE: f32 = 4.0;
/// Regen heals this share of max HP per second...
const REGEN_RATE: f32 = 0.015;
/// ...once the player has gone this long without landing a hit.
const REGEN_DELAY: f32 = 3.0;
/// Abilities start at this tier and cycle in `Ability::ALL` order.
const FIRST_ABILITY_TIER: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    Shield,
    Fog,
    Haste,
    Regen,
}

impl Ability {
    pub const ALL: [Ability; 4] = [Ability::Shield, Ability::Fog, Ability::Haste, Ability::Regen];

    pub fn name(self) -> &'static str {
        match self {
            Ability::Shield => "SHIELD",
            Ability::Fog => "FOG",
            Ability::Haste => "HASTE",
            Ability::Regen => "REGEN",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Ability::Shield => "absorbs damage before HP",
            Ability::Fog => "hides your next pieces",
            Ability::Haste => "speeds up your gravity",
            Ability::Regen => "heals if you stop hitting",
        }
    }

    /// (seconds between triggers, seconds active) for timed abilities.
    fn timing(self) -> Option<(f32, f32)> {
        match self {
            Ability::Fog => Some((FOG_EVERY, FOG_TIME)),
            Ability::Haste => Some((HASTE_EVERY, HASTE_TIME)),
            Ability::Shield | Ability::Regen => None,
        }
    }
}

const NAMES: [&str; 10] = [
    "Slimeling",
    "Bramble Imp",
    "Cinder Toad",
    "Frost Wisp",
    "Gloom Moth",
    "Iron Beetle",
    "Mire Hag",
    "Storm Wyrm",
    "Bone Colossus",
    "Void Lich",
];

/// Each pass through the ten monsters after the first tints them this much
/// further toward red ("elite" versions), up to `MAX_ELITE_TINT`.
const ELITE_TINT_PER_CYCLE: f32 = 0.22;
const MAX_ELITE_TINT: f32 = 0.6;
const ELITE_RED: u32 = 0xff3b4f;

pub fn damage_for(clear: &LineClear) -> u32 {
    clear.lines() * DAMAGE_PER_LINE + clear.attack * DAMAGE_PER_ATTACK
}

/// Blends two 0xRRGGBB colours.
fn blend(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, shift: u32| ((c >> shift) & 0xff) as f32;
    let mix = |shift| (ch(a, shift) + (ch(b, shift) - ch(a, shift)) * t).round() as u32;
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Monster {
    pub tier: u32,
    pub name: &'static str,
    /// 0 for the first ten tiers, then 1, 2, ... for each repeat of the roster.
    pub elite: u32,
    pub art: &'static Art,
    /// Body colour after any elite tint (0xRRGGBB).
    pub color: u32,
    pub max_hp: u32,
    pub time_limit: f32,
    /// Seconds between garbage attacks.
    pub attack_every: f32,
    pub attack_lines: u32,
    /// Gravity level the player's field runs at during this fight.
    pub gravity_level: u32,
    pub ability: Option<Ability>,
}

impl Monster {
    /// Name with an elite marker, e.g. "Slimeling +1".
    pub fn title(&self) -> String {
        if self.elite == 0 {
            self.name.to_owned()
        } else {
            format!("{} +{}", self.name, self.elite)
        }
    }
}

pub fn monster(tier: u32) -> Monster {
    let tier = tier.max(1);
    let t = (tier - 1) as f32;
    let i = (tier as usize - 1) % NAMES.len();
    let elite = (tier - 1) / NAMES.len() as u32;
    let art = monster_art::ALL[i];
    let tint = (elite as f32 * ELITE_TINT_PER_CYCLE).min(MAX_ELITE_TINT);
    Monster {
        tier,
        name: NAMES[i],
        elite,
        art,
        color: blend(art.body, ELITE_RED, tint),
        max_hp: (20.0 * 1.28f32.powf(t)).round() as u32,
        time_limit: (90.0 - 4.0 * t).max(50.0),
        attack_every: (10.0 - 0.6 * t).max(3.5),
        attack_lines: (1 + (tier - 1) / 2).min(6),
        gravity_level: tier.min(15),
        ability: (tier >= FIRST_ABILITY_TIER)
            .then(|| Ability::ALL[(tier - FIRST_ABILITY_TIER) as usize % Ability::ALL.len()]),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defeat {
    Timeout,
    ToppedOut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Fighting,
    /// Monster just died; the next appears when `left` reaches 0.
    Intermission { left: f32 },
    Over(Defeat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattleEvent {
    Hit { damage: u32 },
    MonsterAttack { lines: u32 },
    Defeated { tier: u32 },
    NewMonster { tier: u32 },
    AbilityStarted(Ability),
    Lost(Defeat),
}

pub struct Battle {
    pub monster: Monster,
    pub hp: u32,
    pub time_left: f32,
    /// Seconds until the monster's next garbage attack.
    pub attack_timer: f32,
    pub phase: Phase,
    pub defeated: u32,
    pub total_damage: u32,
    /// Recent hits for the UI: (damage, age in seconds).
    pub hits: Vec<(u32, f32)>,
    /// Remaining shield (Shield monsters).
    pub shield: u32,
    /// Seconds until a timed ability (Fog, Haste) next triggers.
    pub ability_cooldown: f32,
    /// Seconds a timed ability has left while active; 0 when inactive.
    pub ability_active: f32,
    since_hit: f32,
    regen_acc: f32,
    events: Vec<BattleEvent>,
}

impl Battle {
    /// Starts a run at tier 1 and sets the field's gravity to match.
    pub fn new(game: &mut Game) -> Self {
        let mut battle = Battle {
            monster: monster(1),
            hp: 0,
            time_left: 0.0,
            attack_timer: 0.0,
            phase: Phase::Fighting,
            defeated: 0,
            total_damage: 0,
            hits: Vec::new(),
            shield: 0,
            ability_cooldown: 0.0,
            ability_active: 0.0,
            since_hit: 0.0,
            regen_acc: 0.0,
            events: Vec::new(),
        };
        battle.enter(monster(1), game);
        battle
    }

    fn enter(&mut self, m: Monster, game: &mut Game) {
        self.hp = m.max_hp;
        self.time_left = m.time_limit;
        self.attack_timer = m.attack_every;
        self.shield = match m.ability {
            Some(Ability::Shield) => (m.max_hp as f32 * SHIELD_FRACTION).round() as u32,
            _ => 0,
        };
        self.ability_cooldown = m.ability.and_then(Ability::timing).map_or(0.0, |(every, _)| every);
        self.ability_active = 0.0;
        self.since_hit = 0.0;
        self.regen_acc = 0.0;
        game.set_level(m.gravity_level);
        game.gravity_scale = 1.0;
        self.monster = m;
        self.phase = Phase::Fighting;
    }

    /// Debug builds: jump straight to the next monster.
    #[cfg(debug_assertions)]
    pub fn skip_monster(&mut self, game: &mut Game) {
        if !self.over() {
            game.clear_pending_garbage();
            self.enter(monster(self.monster.tier + 1), game);
        }
    }

    /// Fog is hiding the player's next queue right now.
    pub fn next_hidden(&self) -> bool {
        self.phase == Phase::Fighting && self.monster.ability == Some(Ability::Fog) && self.ability_active > 0.0
    }

    /// Regen is currently healing (for the UI).
    pub fn regenerating(&self) -> bool {
        self.phase == Phase::Fighting
            && self.monster.ability == Some(Ability::Regen)
            && self.since_hit >= REGEN_DELAY
            && self.hp < self.monster.max_hp
    }

    /// A timed ability is currently in effect (for the UI).
    pub fn ability_in_effect(&self) -> bool {
        self.phase == Phase::Fighting && self.ability_active > 0.0
    }

    fn update_ability(&mut self, dt: f32, game: &mut Game) {
        let Some(ability) = self.monster.ability else {
            return;
        };
        if let Some((every, active_for)) = ability.timing() {
            if self.ability_active > 0.0 {
                self.ability_active = (self.ability_active - dt).max(0.0);
            } else {
                self.ability_cooldown -= dt;
                if self.ability_cooldown <= 0.0 {
                    self.ability_cooldown += every;
                    self.ability_active = active_for;
                    self.events.push(BattleEvent::AbilityStarted(ability));
                }
            }
        }
        game.gravity_scale = if ability == Ability::Haste && self.ability_active > 0.0 {
            HASTE_SCALE
        } else {
            1.0
        };
        if ability == Ability::Regen {
            self.since_hit += dt;
            if self.since_hit >= REGEN_DELAY && self.hp < self.monster.max_hp {
                self.regen_acc += REGEN_RATE * self.monster.max_hp as f32 * dt;
                let heal = self.regen_acc.floor();
                self.regen_acc -= heal;
                self.hp = (self.hp + heal as u32).min(self.monster.max_hp);
            }
        }
    }

    pub fn over(&self) -> bool {
        matches!(self.phase, Phase::Over(_))
    }

    pub fn take_events(&mut self) -> Vec<BattleEvent> {
        std::mem::take(&mut self.events)
    }

    /// Feed every game event through here.
    pub fn on_game_event(&mut self, event: &Event, game: &mut Game) {
        let Event::LinesCleared(clear) = event else {
            return;
        };
        if self.phase != Phase::Fighting {
            return;
        }
        let damage = damage_for(clear);
        if damage == 0 {
            return;
        }
        let absorbed = damage.min(self.shield);
        self.shield -= absorbed;
        let dealt = (damage - absorbed).min(self.hp);
        self.hp -= dealt;
        self.total_damage += absorbed + dealt;
        self.since_hit = 0.0;
        self.regen_acc = 0.0;
        self.hits.push((damage, 0.0));
        self.events.push(BattleEvent::Hit { damage });
        if self.hp == 0 {
            self.defeated += 1;
            self.events.push(BattleEvent::Defeated {
                tier: self.monster.tier,
            });
            game.clear_pending_garbage();
            game.gravity_scale = 1.0;
            self.ability_active = 0.0;
            self.phase = Phase::Intermission { left: INTERMISSION };
        }
    }

    pub fn update(&mut self, dt: f32, game: &mut Game) {
        self.hits.retain_mut(|(_, age)| {
            *age += dt;
            *age < HIT_POPUP_TIME
        });

        if self.over() {
            return;
        }
        if game.game_over {
            self.lose(Defeat::ToppedOut);
            return;
        }

        match self.phase {
            Phase::Fighting => {
                self.time_left -= dt;
                if self.time_left <= 0.0 {
                    self.time_left = 0.0;
                    self.lose(Defeat::Timeout);
                    return;
                }
                self.update_ability(dt, game);
                self.attack_timer -= dt;
                if self.attack_timer <= 0.0 {
                    self.attack_timer += self.monster.attack_every;
                    let lines = self.monster.attack_lines;
                    game.receive_garbage(lines);
                    self.events.push(BattleEvent::MonsterAttack { lines });
                }
            }
            Phase::Intermission { left } => {
                let left = left - dt;
                if left <= 0.0 {
                    let next = monster(self.monster.tier + 1);
                    self.events.push(BattleEvent::NewMonster { tier: next.tier });
                    self.enter(next, game);
                } else {
                    self.phase = Phase::Intermission { left };
                }
            }
            Phase::Over(_) => {}
        }
    }

    fn lose(&mut self, why: Defeat) {
        self.phase = Phase::Over(why);
        self.events.push(BattleEvent::Lost(why));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear(lines: usize, attack: u32) -> Event {
        Event::LinesCleared(LineClear {
            rows: (0..lines).collect(),
            snapshot: Vec::new(),
            t_spin: false,
            back_to_back: false,
            combo: 0,
            perfect_clear: false,
            attack,
            cancelled: 0,
        })
    }

    fn setup() -> (Game, Battle) {
        let mut game = Game::new(1);
        let battle = Battle::new(&mut game);
        (game, battle)
    }

    #[test]
    fn monsters_get_harder() {
        for tier in 1..30 {
            let (a, b) = (monster(tier), monster(tier + 1));
            assert!(b.max_hp > a.max_hp, "tier {tier}");
            assert!(b.time_limit <= a.time_limit);
            assert!(b.attack_every <= a.attack_every);
            assert!(b.attack_lines >= a.attack_lines);
            assert!(b.gravity_level >= a.gravity_level);
        }
        assert_eq!(monster(1).max_hp, 20);
        assert_eq!(monster(100).time_limit, 50.0, "time limit floors");
    }

    #[test]
    fn roster_cycles_with_elite_versions() {
        assert_eq!(monster(1).art, &monster_art::SLIMELING);
        assert_eq!(monster(10).art, &monster_art::VOID_LICH);
        let (first, elite) = (monster(1), monster(11));
        assert_eq!(elite.art, first.art);
        assert_eq!((first.elite, elite.elite), (0, 1));
        assert_eq!(first.title(), "Slimeling");
        assert_eq!(elite.title(), "Slimeling +1");
        assert_eq!(first.color, first.art.body, "no tint before the first repeat");
        assert_ne!(elite.color, first.color, "elites are tinted");
    }

    #[test]
    fn blend_endpoints() {
        assert_eq!(blend(0x102030, 0xffffff, 0.0), 0x102030);
        assert_eq!(blend(0x102030, 0xffffff, 1.0), 0xffffff);
    }

    #[test]
    fn damage_counts_lines_and_attack() {
        let quad = clear(4, 4);
        let Event::LinesCleared(c) = &quad else { unreachable!() };
        assert_eq!(damage_for(c), 4 + 8);
    }

    #[test]
    fn game_sets_gravity_from_the_monster() {
        let (game, _) = setup();
        assert_eq!(game.level, monster(1).gravity_level);
    }

    #[test]
    fn clears_damage_and_kill_leads_to_the_next_monster() {
        let (mut game, mut b) = setup();
        b.on_game_event(&clear(2, 1), &mut game); // 2 + 2
        assert_eq!(b.hp, 16);
        game.receive_garbage(3);
        for _ in 0..4 {
            b.on_game_event(&clear(4, 4), &mut game); // 12 each, overkill clamps
        }
        assert_eq!(b.hp, 0);
        assert_eq!(b.defeated, 1);
        assert_eq!(b.total_damage, 20);
        assert_eq!(game.pending_garbage(), 0, "garbage cleared on kill");
        assert!(matches!(b.phase, Phase::Intermission { .. }));

        b.on_game_event(&clear(4, 4), &mut game);
        assert_eq!(b.total_damage, 20, "no damage during the break");

        b.update(INTERMISSION + 0.01, &mut game);
        assert_eq!(b.monster.tier, 2);
        assert_eq!(b.hp, monster(2).max_hp);
        assert_eq!(game.level, monster(2).gravity_level);
        let events = b.take_events();
        assert!(events.contains(&BattleEvent::Defeated { tier: 1 }));
        assert!(events.contains(&BattleEvent::NewMonster { tier: 2 }));
    }

    #[test]
    fn monster_attacks_on_its_timer() {
        let (mut game, mut b) = setup();
        let every = b.monster.attack_every;
        b.update(every - 0.1, &mut game);
        assert_eq!(game.pending_garbage(), 0);
        b.update(0.2, &mut game);
        assert_eq!(game.pending_garbage(), b.monster.attack_lines);
        assert!(b.take_events().contains(&BattleEvent::MonsterAttack {
            lines: b.monster.attack_lines
        }));
    }

    #[test]
    fn running_out_of_time_ends_the_run() {
        let (mut game, mut b) = setup();
        let limit = b.monster.time_limit;
        let mut t = 0.0;
        while t < limit + 1.0 && !b.over() {
            b.update(0.5, &mut game);
            t += 0.5;
        }
        assert_eq!(b.phase, Phase::Over(Defeat::Timeout));
        assert_eq!(b.time_left, 0.0);
    }

    #[test]
    fn topping_out_ends_the_run() {
        let (mut game, mut b) = setup();
        game.game_over = true;
        b.update(0.016, &mut game);
        assert_eq!(b.phase, Phase::Over(Defeat::ToppedOut));
        assert!(b.take_events().contains(&BattleEvent::Lost(Defeat::ToppedOut)));
    }

    /// A battle already facing the monster at `tier`.
    fn at_tier(tier: u32) -> (Game, Battle) {
        let (mut game, mut b) = setup();
        b.enter(monster(tier), &mut game);
        (game, b)
    }

    #[test]
    fn abilities_start_at_tier_three_and_cycle() {
        assert_eq!(monster(1).ability, None);
        assert_eq!(monster(2).ability, None);
        let cycle: Vec<_> = (3..11).map(|t| monster(t).ability.unwrap()).collect();
        assert_eq!(cycle[..4], Ability::ALL);
        assert_eq!(cycle[4..], Ability::ALL);
    }

    #[test]
    fn shield_absorbs_damage_first() {
        let (mut game, mut b) = at_tier(3);
        let max = b.monster.max_hp;
        let shield = b.shield;
        assert_eq!(shield, (max as f32 * SHIELD_FRACTION).round() as u32);
        b.on_game_event(&clear(1, 0), &mut game);
        assert_eq!((b.shield, b.hp), (shield - 1, max), "HP untouched while shielded");
        b.on_game_event(&clear(4, 40), &mut game); // way more than the shield
        assert_eq!(b.shield, 0);
        assert!(b.hp < max);
    }

    #[test]
    fn fog_hides_the_queue_on_a_cycle() {
        let (mut game, mut b) = at_tier(4);
        assert!(!b.next_hidden());
        b.update(FOG_EVERY + 0.01, &mut game);
        assert!(b.next_hidden());
        assert!(b.take_events().contains(&BattleEvent::AbilityStarted(Ability::Fog)));
        b.update(FOG_TIME, &mut game);
        assert!(!b.next_hidden());
    }

    #[test]
    fn haste_speeds_gravity_temporarily() {
        let (mut game, mut b) = at_tier(5);
        b.update(HASTE_EVERY + 0.01, &mut game);
        assert_eq!(game.gravity_scale, HASTE_SCALE);
        b.update(HASTE_TIME + 0.01, &mut game);
        assert_eq!(game.gravity_scale, 1.0);
    }

    #[test]
    fn regen_heals_only_after_a_lull() {
        let (mut game, mut b) = at_tier(6);
        b.on_game_event(&clear(4, 4), &mut game);
        let after_hit = b.hp;
        b.update(REGEN_DELAY - 0.5, &mut game);
        assert_eq!(b.hp, after_hit, "no healing right after a hit");
        b.update(2.0, &mut game);
        assert!(b.hp > after_hit);
        for _ in 0..200 {
            b.update(0.5, &mut game);
        }
        assert!(b.hp <= b.monster.max_hp);
    }

    #[test]
    fn killing_a_haste_monster_restores_gravity() {
        let (mut game, mut b) = at_tier(5);
        b.update(HASTE_EVERY + 0.01, &mut game);
        for _ in 0..20 {
            b.on_game_event(&clear(4, 4), &mut game);
        }
        assert_eq!(game.gravity_scale, 1.0);
    }

    #[test]
    fn hit_popups_expire() {
        let (mut game, mut b) = setup();
        b.on_game_event(&clear(1, 0), &mut game);
        assert_eq!(b.hits.len(), 1);
        b.update(HIT_POPUP_TIME + 0.01, &mut game);
        assert!(b.hits.is_empty());
    }
}
