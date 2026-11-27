//! Party rooms: a free-for-all of up to `PARTY_MAX` players. Everyone gets the same pieces;
//! each attack goes to a random opponent still standing; whoever tops out (or leaves) is
//! out, placed by how many were still in; the last one standing wins.
//!
//! Pure bookkeeping: `Versus` drives it (phase, gravity, the outbox) and the session feeds
//! it what arrives. Each player's view of the standings is their own (KOs arrive in
//! slightly different orders on each side), which is fine for a party.

use crate::game::Game;
use crate::online::Snapshot;

/// Seconds an attack line (who you hit, who hit you) stays highlighted.
pub const HIT_SHOW: f32 = 1.2;

pub struct Member {
    pub id: String,
    pub name: String,
    /// Their field, mirrored from their snapshots (never ticked).
    pub field: Game,
    /// Where they finished, once they're out (1 = won).
    pub place: Option<u32>,
    /// Seconds since we last hit them / they last hit us.
    pub since_hit: f32,
    pub since_attacked: f32,
}

impl Member {
    pub fn alive(&self) -> bool {
        self.place.is_none()
    }
}

pub struct Party {
    pub me: String,
    /// Everyone else in the match, in roster order.
    pub members: Vec<Member>,
    /// Where we finished, once we're out (or have won).
    pub my_place: Option<u32>,
    rng: u64,
}

impl Party {
    /// `roster` is everyone in the match (us included), as the host sent it.
    pub fn new(me: &str, roster: &[(String, String)], seed: u64) -> Self {
        let members = roster
            .iter()
            .filter(|(id, _)| id != me)
            .map(|(id, name)| Member { id: id.clone(), name: name.clone(), field: Game::new(seed), place: None, since_hit: f32::MAX, since_attacked: f32::MAX })
            .collect();
        // Each player picks targets on their own stream.
        let rng = me.bytes().fold(seed ^ 0x2545_F491_4F6C_DD1D, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3)) | 1;
        Party { me: me.to_owned(), members, my_place: None, rng }
    }

    /// Players in the match, us included.
    pub fn players(&self) -> u32 {
        self.members.len() as u32 + 1
    }

    /// Players still standing, us included.
    pub fn alive(&self) -> u32 {
        self.members.iter().filter(|m| m.alive()).count() as u32 + self.my_place.is_none() as u32
    }

    /// The match is decided: at most one player left.
    pub fn finished(&self) -> bool {
        self.alive() <= 1
    }

    fn next_rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// A random opponent still standing, to send an attack to (and it's marked as hit).
    pub fn pick_target(&mut self) -> Option<String> {
        let alive: Vec<usize> = (0..self.members.len()).filter(|&i| self.members[i].alive()).collect();
        if alive.is_empty() {
            return None;
        }
        let i = alive[(self.next_rand() % alive.len() as u64) as usize];
        self.members[i].since_hit = 0.0;
        Some(self.members[i].id.clone())
    }

    fn member(&mut self, id: &str) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| m.id == id)
    }

    pub fn on_snapshot(&mut self, from: &str, snap: &Snapshot) {
        if let Some(m) = self.member(from) {
            if m.alive() {
                snap.apply(&mut m.field);
            }
        }
    }

    /// `from` attacked us.
    pub fn attacked_by(&mut self, from: &str) {
        if let Some(m) = self.member(from) {
            m.since_attacked = 0.0;
        }
    }

    /// The place the next player out takes.
    fn next_place(&self) -> u32 {
        self.alive()
    }

    /// `id` topped out or left. Returns whether that just won us the match.
    pub fn knock_out(&mut self, id: &str) -> bool {
        let place = self.next_place();
        let Some(m) = self.member(id) else {
            return false;
        };
        if !m.alive() {
            return false;
        }
        m.place = Some(place);
        m.field.game_over = true;
        if self.finished() {
            // Whoever's left won.
            if self.my_place.is_none() {
                self.my_place = Some(1);
                return true;
            }
            if let Some(last) = self.members.iter_mut().find(|m| m.alive()) {
                last.place = Some(1);
            }
        }
        false
    }

    /// We topped out: our place.
    pub fn topped_out(&mut self) -> u32 {
        if let Some(p) = self.my_place {
            return p;
        }
        let place = self.next_place();
        self.my_place = Some(place);
        if self.finished() {
            if let Some(last) = self.members.iter_mut().find(|m| m.alive()) {
                last.place = Some(1);
            }
        }
        place
    }

    /// Roster members no longer in the room are out.
    pub fn present(&mut self, here: &[String]) -> bool {
        let gone: Vec<String> = self.members.iter().filter(|m| m.alive() && !here.contains(&m.id)).map(|m| m.id.clone()).collect();
        let mut won = false;
        for id in gone {
            won |= self.knock_out(&id);
        }
        won
    }

    pub fn update(&mut self, dt: f32) {
        for m in &mut self.members {
            m.since_hit += dt;
            m.since_attacked += dt;
        }
    }

    /// Everyone by place: (name, place, is us). Players still in come first, unplaced.
    pub fn standings<'a>(&'a self, my_name: &'a str) -> Vec<(&'a str, Option<u32>, bool)> {
        let mut all: Vec<(&str, Option<u32>, bool)> = self.members.iter().map(|m| (m.name.as_str(), m.place, false)).collect();
        all.push((my_name, self.my_place, true));
        all.sort_by_key(|&(_, p, _)| p.unwrap_or(0));
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster(n: usize) -> Vec<(String, String)> {
        (0..n).map(|i| (format!("id{i}"), format!("P{i}"))).collect()
    }

    #[test]
    fn targets_are_random_living_opponents() {
        let mut p = Party::new("id0", &roster(5), 3);
        assert_eq!(p.players(), 5);
        p.knock_out("id2");
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let t = p.pick_target().unwrap();
            assert!(t != "id0" && t != "id2", "{t}");
            seen.insert(t);
        }
        assert_eq!(seen.len(), 3, "every living opponent gets hit sometimes");
    }

    #[test]
    fn places_count_down_and_the_last_one_wins() {
        let mut p = Party::new("id0", &roster(4), 1);
        assert!(!p.knock_out("id1"));
        assert_eq!(p.members[0].place, Some(4));
        assert!(!p.knock_out("id1"), "a second KO for the same player changes nothing");
        assert!(!p.knock_out("id3"));
        assert_eq!(p.members[2].place, Some(3));
        assert!(p.knock_out("id2"), "the last opponent out: we won");
        assert_eq!(p.my_place, Some(1));
        assert!(p.finished());
        assert_eq!(p.pick_target(), None);
    }

    #[test]
    fn topping_out_places_us_and_crowns_the_survivor() {
        let mut p = Party::new("id1", &roster(3), 1);
        assert_eq!(p.topped_out(), 3);
        assert!(!p.finished());
        p.knock_out("id2");
        assert_eq!(p.members.iter().find(|m| m.id == "id0").unwrap().place, Some(1));
        let s = p.standings("me");
        assert_eq!(s.iter().map(|e| e.1).collect::<Vec<_>>(), vec![Some(1), Some(2), Some(3)]);
        assert!(s[2].2, "we're third");
    }

    #[test]
    fn leaving_is_a_knock_out() {
        let mut p = Party::new("id0", &roster(3), 1);
        assert!(!p.present(&["id0".into(), "id1".into(), "id2".into()]));
        assert!(!p.present(&["id0".into(), "id2".into()]));
        assert_eq!(p.members[0].place, Some(3));
        assert!(p.present(&["id0".into()]));
        assert_eq!(p.my_place, Some(1));
    }
}
