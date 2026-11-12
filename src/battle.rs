//! Battler mode: a gauntlet of monsters. Each has HP and a time limit; your clears
//! damage it (and still cancel its garbage), while it periodically sends garbage
//! at you. Beat it before time runs out to face the next, tougher one. The run
//! ends on a timeout or a top out.
//!
//! Pure logic: driven by game events and `update`, no rendering or input.

use crate::bag::Rng;
use crate::game::{Event, Game, LineClear};

/// Breather between monsters, seconds.
pub const INTERMISSION: f32 = 2.0;
const DAMAGE_PER_LINE: u32 = 1;
const DAMAGE_PER_ATTACK: u32 = 2;
/// How long a damage number stays on screen.
pub const HIT_POPUP_TIME: f32 = 0.9;

pub const SPRITE_W: usize = 10;
pub const SPRITE_H: usize = 8;

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

/// Body colours, cycled by tier.
const PALETTE: [u32; 10] = [
    0x7dff8a, 0x5cdb5c, 0xff9a3c, 0x7fd8ff, 0xb08cff, 0xa8b4c8, 0x4fbf8f, 0x4f8bff, 0xe8e0c8, 0xc061ff,
];

pub fn damage_for(clear: &LineClear) -> u32 {
    clear.lines() * DAMAGE_PER_LINE + clear.attack * DAMAGE_PER_ATTACK
}

#[derive(Clone, Debug, PartialEq)]
pub struct Monster {
    pub tier: u32,
    pub name: &'static str,
    pub color: u32,
    pub max_hp: u32,
    pub time_limit: f32,
    /// Seconds between garbage attacks.
    pub attack_every: f32,
    pub attack_lines: u32,
    /// Gravity level the player's field runs at during this fight.
    pub gravity_level: u32,
    pub sprite: [[bool; SPRITE_W]; SPRITE_H],
}

/// Symmetric random creature: generate the left half, mirror it, and make sure
/// it has a solid core so it never comes out as scattered dots.
fn sprite_for(tier: u32) -> [[bool; SPRITE_W]; SPRITE_H] {
    let mut rng = Rng::new(0x5EED_0000 + tier as u64 * 7919);
    let mut grid = [[false; SPRITE_W]; SPRITE_H];
    let half = SPRITE_W / 2;
    for (y, row) in grid.iter_mut().enumerate() {
        for x in 0..half {
            // Denser toward the centre column and the middle rows.
            let centre = x as f32 / (half - 1) as f32;
            let middle = 1.0 - ((y as f32 - 3.5).abs() / 4.0);
            let chance = 0.25 + 0.45 * centre * middle + 0.2 * middle;
            let filled = (rng.next_u64() % 1000) as f32 / 1000.0 < chance;
            row[x] = filled;
            row[SPRITE_W - 1 - x] = filled;
        }
    }
    for row in grid.iter_mut().take(6).skip(2) {
        for cell in row.iter_mut().take(7).skip(3) {
            *cell = true;
        }
    }
    grid
}

/// Eye cells (always drawn on the solid core).
pub const EYES: [(usize, usize); 2] = [(3, 3), (6, 3)];

pub fn monster(tier: u32) -> Monster {
    let tier = tier.max(1);
    let t = (tier - 1) as f32;
    let i = (tier as usize - 1) % NAMES.len();
    Monster {
        tier,
        name: NAMES[i],
        color: PALETTE[i],
        max_hp: (20.0 * 1.28f32.powf(t)).round() as u32,
        time_limit: (90.0 - 4.0 * t).max(50.0),
        attack_every: (10.0 - 0.6 * t).max(3.5),
        attack_lines: (1 + (tier - 1) / 2).min(6),
        gravity_level: tier.min(15),
        sprite: sprite_for(tier),
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
            events: Vec::new(),
        };
        battle.enter(monster(1), game);
        battle
    }

    fn enter(&mut self, m: Monster, game: &mut Game) {
        self.hp = m.max_hp;
        self.time_left = m.time_limit;
        self.attack_timer = m.attack_every;
        game.set_level(m.gravity_level);
        self.monster = m;
        self.phase = Phase::Fighting;
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
        let dealt = damage.min(self.hp);
        self.hp -= dealt;
        self.total_damage += dealt;
        self.hits.push((damage, 0.0));
        self.events.push(BattleEvent::Hit { damage });
        if self.hp == 0 {
            self.defeated += 1;
            self.events.push(BattleEvent::Defeated {
                tier: self.monster.tier,
            });
            game.clear_pending_garbage();
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
    fn sprites_are_symmetric_with_a_solid_core() {
        for tier in 1..20 {
            let s = monster(tier).sprite;
            for row in s {
                for x in 0..SPRITE_W {
                    assert_eq!(row[x], row[SPRITE_W - 1 - x]);
                }
            }
            for (x, y) in EYES {
                assert!(s[y][x], "eyes sit on the body");
            }
        }
        assert_eq!(monster(4).sprite, monster(4).sprite, "deterministic");
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

    #[test]
    fn hit_popups_expire() {
        let (mut game, mut b) = setup();
        b.on_game_event(&clear(1, 0), &mut game);
        assert_eq!(b.hits.len(), 1);
        b.update(HIT_POPUP_TIME + 0.01, &mut game);
        assert!(b.hits.is_empty());
    }
}
