//! Online 1v1 over a relay (Supabase Realtime broadcast, see `net.rs` / web/net.js):
//! the message protocol and the snapshot of a player's field that the other side mirrors.
//!
//! Each player simulates their own field; the other side only shows it. Messages are short
//! text lines: `kind|fields`. Pure: no networking here, so it's all testable.

use crate::board::{Cell, TOTAL_ROWS, WIDTH};
use crate::game::{Active, Game, NEXT_PREVIEW};
use crate::piece::PieceKind;

/// Bumped when the protocol changes, so old and new builds don't play each other.
pub const PROTOCOL: u32 = 2;
/// Rows sent per snapshot: the visible field plus the buffer rows drawn above it.
const ROWS: usize = 23;

/// What one side sends the other.
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// "I'm here": name, protocol version and what the room is (sent on joining and when
    /// someone arrives).
    Hello { name: String, protocol: u32, room: RoomKind },
    /// The host starts a match: both fields use this seed.
    Start { seed: u64 },
    /// The sender's field, for the other side to draw.
    Snap(Snapshot),
    /// Garbage lines the sender sent (after cancelling their own).
    Garbage(u32),
    /// The sender topped out.
    Ko,
    /// Ready for another match.
    Rematch,
    /// Party: the host starts a match for these players (id, name), all from `seed`.
    PartyStart { seed: u64, roster: Vec<(String, String)> },
    /// Party: garbage the sender sent to the player with this id.
    Hit { to: String, lines: u32 },
}

/// What a room is for, as far as the sender knows (whoever made it knows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Unknown,
    Duel,
    Party,
}

/// Most players in a party room.
pub const PARTY_MAX: usize = 10;

/// A name or id safe to put in a message (no separators, at most 16 characters).
pub fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !"|;:,\n".contains(*c)).take(16).collect()
}

/// Everything the other side needs to draw a field and its numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The bottom `ROWS` rows, top to bottom: `.` empty, `G` garbage, else the piece letter.
    pub rows: String,
    pub active: Option<Active>,
    pub hold: Option<PieceKind>,
    pub next: String,
    pub pending: u32,
    pub pieces: u32,
    pub attack: u32,
    pub lines: u32,
    pub time: f32,
}

fn cell_char(c: Option<Cell>) -> char {
    match c {
        None => '.',
        Some(Cell::Garbage) => 'G',
        Some(Cell::Piece(k)) => k.letter(),
    }
}

fn char_cell(c: char) -> Option<Cell> {
    match c {
        'G' => Some(Cell::Garbage),
        _ => PieceKind::from_letter(c).map(Cell::Piece),
    }
}

impl Snapshot {
    pub fn of(g: &Game) -> Snapshot {
        let mut rows = String::with_capacity(ROWS * WIDTH);
        for y in TOTAL_ROWS - ROWS..TOTAL_ROWS {
            for x in 0..WIDTH {
                rows.push(cell_char(g.board.get(x, y)));
            }
        }
        Snapshot {
            rows,
            active: (!g.game_over).then_some(g.active),
            hold: g.hold,
            next: g.next.iter().map(|k| k.letter()).collect(),
            pending: g.pending_garbage(),
            pieces: g.stats.pieces,
            attack: g.stats.attack,
            lines: g.stats.lines,
            time: g.stats.time,
        }
    }

    /// Makes `g` (a mirror, never ticked) look like the field this came from.
    pub fn apply(&self, g: &mut Game) {
        let top = TOTAL_ROWS - ROWS;
        for y in 0..TOTAL_ROWS {
            for x in 0..WIDTH {
                let c = if y >= top { self.rows.chars().nth((y - top) * WIDTH + x).and_then(char_cell) } else { None };
                g.board.put(x, y, c);
            }
        }
        if let Some(a) = self.active {
            g.active = a;
        }
        g.hold = self.hold;
        let next: Vec<PieceKind> = self.next.chars().filter_map(PieceKind::from_letter).collect();
        if next.len() == NEXT_PREVIEW {
            g.next = next.into();
        }
        g.mirror_pending(self.pending);
        g.stats.pieces = self.pieces;
        g.stats.attack = self.attack;
        g.stats.lines = self.lines;
        g.stats.time = self.time;
    }

    fn encode(&self) -> String {
        let active = self.active.map_or("-".to_owned(), |a| format!("{}{},{},{}", a.kind.letter(), a.rot, a.x, a.y));
        let hold = self.hold.map_or('-', PieceKind::letter);
        format!("{}|{}|{}|{}|{}|{}|{}|{}|{:.2}", self.rows, active, hold, self.next, self.pending, self.pieces, self.attack, self.lines, self.time)
    }

    fn decode(f: &[&str]) -> Option<Snapshot> {
        let [rows, active, hold, next, pending, pieces, attack, lines, time] = f else {
            return None;
        };
        if rows.len() != ROWS * WIDTH {
            return None;
        }
        let active = match *active {
            "-" => None,
            a => {
                let kind = PieceKind::from_letter(a.chars().next()?)?;
                let mut n = a[1..].split(',').map(|v| v.parse::<i32>());
                let (rot, x, y) = (n.next()?.ok()?, n.next()?.ok()?, n.next()?.ok()?);
                // Only spots a piece could really be in.
                if !(0..4).contains(&rot) || !(-3..WIDTH as i32 + 3).contains(&x) || !(0..TOTAL_ROWS as i32).contains(&y) {
                    return None;
                }
                Some(Active { kind, rot: rot as u8, x, y })
            }
        };
        Some(Snapshot {
            rows: rows.to_string(),
            active,
            hold: hold.chars().next().and_then(PieceKind::from_letter),
            next: next.to_string(),
            pending: pending.parse().ok()?,
            pieces: pieces.parse().ok()?,
            attack: attack.parse().ok()?,
            lines: lines.parse().ok()?,
            time: time.parse().ok()?,
        })
    }
}

impl Msg {
    pub fn encode(&self) -> String {
        match self {
            Msg::Hello { name, protocol, room } => {
                let k = match room {
                    RoomKind::Unknown => 0,
                    RoomKind::Duel => 1,
                    RoomKind::Party => 2,
                };
                format!("hello|{}|{protocol}|{k}", clean_name(name))
            }
            Msg::PartyStart { seed, roster } => {
                let list: Vec<String> = roster.iter().map(|(id, name)| format!("{}:{}", clean_name(id), clean_name(name))).collect();
                format!("pstart|{seed}|{}", list.join(";"))
            }
            Msg::Hit { to, lines } => format!("hit|{}|{lines}", clean_name(to)),
            Msg::Start { seed } => format!("start|{seed}"),
            Msg::Snap(s) => format!("snap|{}", s.encode()),
            Msg::Garbage(n) => format!("garbage|{n}"),
            Msg::Ko => "ko".to_owned(),
            Msg::Rematch => "rematch".to_owned(),
        }
    }

    /// None for anything malformed or unknown (it's ignored).
    pub fn decode(text: &str) -> Option<Msg> {
        let f: Vec<&str> = text.split('|').collect();
        Some(match f.as_slice() {
            // (Protocol 1 sent no room kind: it decodes, and the version mismatch shows.)
            ["hello", name, protocol] => Msg::Hello { name: clean_name(name), protocol: protocol.parse().ok()?, room: RoomKind::Unknown },
            ["hello", name, protocol, k] => Msg::Hello {
                name: clean_name(name),
                protocol: protocol.parse().ok()?,
                room: match *k {
                    "1" => RoomKind::Duel,
                    "2" => RoomKind::Party,
                    _ => RoomKind::Unknown,
                },
            },
            ["pstart", seed, list] => {
                let roster: Vec<(String, String)> = list.split(';').filter_map(|e| e.split_once(':')).map(|(id, n)| (clean_name(id), clean_name(n))).take(PARTY_MAX).collect();
                if roster.len() < 2 {
                    return None;
                }
                Msg::PartyStart { seed: seed.parse().ok()?, roster }
            }
            ["hit", to, n] => Msg::Hit { to: clean_name(to), lines: n.parse::<u32>().ok()?.min(30) },
            ["start", seed] => Msg::Start { seed: seed.parse().ok()? },
            ["snap", rest @ ..] => Msg::Snap(Snapshot::decode(rest)?),
            // A single clear can't send more than a perfect clear quad with a big combo.
            ["garbage", n] => Msg::Garbage(n.parse::<u32>().ok()?.min(30)),
            ["ko"] => Msg::Ko,
            ["rematch"] => Msg::Rematch,
            _ => return None,
        })
    }
}

/// A short room code people can read out: 4 letters, no look-alikes.
pub fn room_code(seed: u64) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ";
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..4)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            LETTERS[(x % LETTERS.len() as u64) as usize] as char
        })
        .collect()
}

/// Only room-code letters, upper-cased, at most 4.
pub fn clean_code(input: &str) -> String {
    input.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_uppercase()).take(4).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Action;

    #[test]
    fn messages_round_trip() {
        let mut g = Game::new(7);
        g.apply(Action::HardDrop);
        g.receive_garbage(3);
        for m in [
            Msg::Hello { name: "Luoyi".into(), protocol: PROTOCOL, room: RoomKind::Party },
            Msg::Hello { name: "Deon".into(), protocol: PROTOCOL, room: RoomKind::Unknown },
            Msg::PartyStart { seed: 77, roster: vec![("ab12".into(), "PLAYER 101".into()), ("zz9".into(), "Luoyi".into())] },
            Msg::Hit { to: "ab12".into(), lines: 5 },
            Msg::Start { seed: 123456789 },
            Msg::Snap(Snapshot::of(&g)),
            Msg::Garbage(4),
            Msg::Ko,
            Msg::Rematch,
        ] {
            assert_eq!(Msg::decode(&m.encode()), Some(m.clone()), "{}", m.encode());
        }
    }

    #[test]
    fn a_mirror_shows_the_same_field() {
        let mut g = Game::new(11);
        for _ in 0..6 {
            g.apply(Action::HardDrop);
        }
        g.receive_garbage(2);
        let mut mirror = Game::new(99);
        Snapshot::of(&g).apply(&mut mirror);
        for y in TOTAL_ROWS - ROWS..TOTAL_ROWS {
            for x in 0..WIDTH {
                assert_eq!(mirror.board.get(x, y), g.board.get(x, y));
            }
        }
        assert_eq!((mirror.active, mirror.hold, mirror.next.clone()), (g.active, g.hold, g.next.clone()));
        assert_eq!(mirror.pending_garbage(), 2);
        assert_eq!(mirror.stats.pieces, 6);
    }

    #[test]
    fn junk_is_ignored_and_garbage_is_capped() {
        for bad in ["", "hello", "start|abc", "snap|short|-|-|IOTSZ|0|0|0|0|0", "garbage|-1", "whatever|1"] {
            assert_eq!(Msg::decode(bad), None, "{bad}");
        }
        assert_eq!(Msg::decode("garbage|9999"), Some(Msg::Garbage(30)));
        assert_eq!(Msg::decode("pstart|5|solo:me"), None, "a party needs two");
        assert_eq!(Msg::decode("hello|old|1"), Some(Msg::Hello { name: "old".into(), protocol: 1, room: RoomKind::Unknown }));
        let names = Msg::Hello { name: "a|b;c:d".into(), protocol: 2, room: RoomKind::Duel };
        assert_eq!(Msg::decode(&names.encode()), Some(Msg::Hello { name: "abcd".into(), protocol: 2, room: RoomKind::Duel }));
    }

    #[test]
    fn room_codes_are_four_clear_letters() {
        let c = room_code(42);
        assert_eq!(c.len(), 4);
        assert!(c.chars().all(|ch| ch.is_ascii_uppercase() && !"ILO".contains(ch)));
        assert_ne!(room_code(42), room_code(43));
        assert_eq!(clean_code(" ab-c1d e"), "ABCD");
    }
}
