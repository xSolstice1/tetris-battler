//! Hand-drawn monster pixel art, one per monster in the gauntlet.
//!
//! Each sprite is 16x14 pixels written as text. A row is either 16 characters,
//! or 8 characters that get mirrored to make a symmetric row.
//!
//! Legend:
//!   `.` empty      `o` outline     `b` body      `h` highlight   `s` shadow
//!   `m` dark (mouths, hollows)     `a` accent    `e` eye white   `p` pupil

pub const W: usize = 16;
pub const H: usize = 14;

#[derive(Debug, PartialEq, Eq)]
pub struct Art {
    rows: [&'static str; H],
    /// Base body colour (0xRRGGBB); outline, highlight, shadow and dark are derived from it.
    pub body: u32,
    pub accent: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pixel {
    Outline,
    Body,
    Highlight,
    Shadow,
    Dark,
    Accent,
    EyeWhite,
    Pupil,
}

fn parse(c: char) -> Option<Pixel> {
    match c {
        'o' => Some(Pixel::Outline),
        'b' => Some(Pixel::Body),
        'h' => Some(Pixel::Highlight),
        's' => Some(Pixel::Shadow),
        'm' => Some(Pixel::Dark),
        'a' => Some(Pixel::Accent),
        'e' => Some(Pixel::EyeWhite),
        'p' => Some(Pixel::Pupil),
        _ => None,
    }
}

impl Art {
    /// Pixel at (x, y), or `None` for transparent.
    pub fn pixel(&self, x: usize, y: usize) -> Option<Pixel> {
        let row = self.rows[y].as_bytes();
        let c = if row.len() == W / 2 {
            // Mirrored half-row.
            row[if x < W / 2 { x } else { W - 1 - x }]
        } else {
            row[x]
        };
        parse(c as char)
    }
}

pub const SLIMELING: Art = Art {
    body: 0x7dff8a,
    accent: 0x2ee6d6,
    rows: [
        "................",
        "................",
        "......oooo......",
        "....oohhbboo....",
        "...ohhbbbbbbo...",
        "..ohbbbbbbbbbo..",
        "..obbeebbeebbo..",
        ".obbbepbbepbbbo.",
        ".obbbbbbbbbbbbo.",
        ".obbbbmmmmbbbbo.",
        ".osbbbbmmbbbbso.",
        ".ossbbbbbbbbsso.",
        "..osssssssssso..",
        "...oooooooooo...",
    ],
};

pub const BRAMBLE_IMP: Art = Art {
    body: 0xd8504d,
    accent: 0xe8d8a8,
    rows: [
        "..a..........a..",
        "..aa........aa..",
        "...ao.oooo.oa...",
        "....obbbbbbo....",
        "...obhbbbbbbo...",
        "...obeebbeebo...",
        "...obpebbepbo...",
        "...obbbmmbbbo...",
        "....obmaambo....",
        "...oobbbbbboo...",
        "..obobbbbbbobo..",
        "..oo.obbbbo.oo..",
        ".....os..so.....",
        "....oo....oo....",
    ],
};

pub const CINDER_TOAD: Art = Art {
    body: 0xff9a3c,
    accent: 0xffe14f,
    rows: [
        "................",
        "................",
        "...ooo....ooo...",
        "..oeepo..opeeo..",
        "..oeeeobboeeeo..",
        ".obbbbbbbbbbbbo.",
        ".obhhbbabbbbbbo.",
        "obhbbbbbbbbabbbo",
        "obbmmmmmmmmmmbbo",
        "obbbmmmmmmmmbbbo",
        ".obbbbbbbbbbbbo.",
        ".osbbabbbbbbbso.",
        "obsoossssssoosbo",
        "oo..oo....oo..oo",
    ],
};

pub const FROST_WISP: Art = Art {
    body: 0x7fd8ff,
    accent: 0xffffff,
    rows: [
        ".......oo.......",
        "......ohho......",
        ".....ohhhbo.....",
        "....ohhbbbbo....",
        "...ohbbbbbbbo...",
        "...obeebbeebo...",
        "..obbpebbepbbo..",
        "..obbbbbbbbbbo..",
        "..obbbbmmbbbbo..",
        "..obbbbbbbbbbo..",
        "...obbbbbbbbo...",
        "...obsbbbbsbo...",
        "....os.bb.so....",
        ".....o..o.......",
    ],
};

pub const GLOOM_MOTH: Art = Art {
    body: 0xb08cff,
    accent: 0xffd23f,
    rows: [
        "oo....o.",
        "obo....o",
        "obbo..ob",
        "obabo.ob",
        "obaaboee",
        "obbbbobp",
        "obbbbobb",
        ".obbbobm",
        "..obbbob",
        "..obabob",
        "...obbos",
        "....oo.o",
        ".......o",
        "........",
    ],
};

pub const IRON_BEETLE: Art = Art {
    body: 0xa8b4c8,
    accent: 0xd0a060,
    rows: [
        ".......a",
        "......aa",
        "......oa",
        "...oooob",
        "..ohhbbb",
        ".ohbbebb",
        ".obbbpbb",
        "obsbbbbo",
        "obsbbbbo",
        "obssbbbo",
        ".ossssso",
        "o.oooooo",
        "o..o..o.",
        "........",
    ],
};

pub const MIRE_HAG: Art = Art {
    body: 0x4fbf8f,
    accent: 0xc8ff5a,
    rows: [
        "......oo",
        ".....obb",
        "....obhb",
        "...obbsm",
        "...obsma",
        "...obsmm",
        "..obbsmm",
        "..obbbsa",
        ".obbbbbb",
        "obabbbbb",
        "oobbbbbb",
        ".obbsbbs",
        ".ossssss",
        "..oooooo",
    ],
};

pub const STORM_WYRM: Art = Art {
    body: 0x4f8bff,
    accent: 0xffe14f,
    rows: [
        ".a......",
        ".ao.....",
        "..ao.ooo",
        "...oobbb",
        "..obhbbb",
        ".obbeebb",
        ".obbpebb",
        "obbbbbbb",
        "obmmmmmm",
        "obmamama",
        ".obbbbbb",
        "..osbbbb",
        "...osssb",
        "....oooo",
    ],
};

pub const BONE_COLOSSUS: Art = Art {
    body: 0xe8e0c8,
    accent: 0xff4f6d,
    rows: [
        "....oooo",
        "..oohhbb",
        ".ohbbbbb",
        "ohbbbbbb",
        "obbmmmbb",
        "obmmaamb",
        "obmmmmbb",
        "obbbbbbm",
        ".obbbbbm",
        "..obbbbb",
        "..obmbmb",
        "..obbbbb",
        "...osbsb",
        "....oooo",
    ],
};

pub const VOID_LICH: Art = Art {
    body: 0x6a3fb0,
    accent: 0xffd23f,
    rows: [
        "...a.a.a",
        "...aaaaa",
        "...ooooo",
        "..obsmmm",
        "..obsmee",
        "..obsmmm",
        "...obsmm",
        "..obbbsm",
        ".obhbbba",
        "obbbbbbb",
        "oabbbbbb",
        ".obbbsbb",
        ".obbsbbs",
        ".ooooooo",
    ],
};

/// In gauntlet order.
pub const ALL: [&Art; 10] = [
    &SLIMELING,
    &BRAMBLE_IMP,
    &CINDER_TOAD,
    &FROST_WISP,
    &GLOOM_MOTH,
    &IRON_BEETLE,
    &MIRE_HAG,
    &STORM_WYRM,
    &BONE_COLOSSUS,
    &VOID_LICH,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sprite_is_well_formed() {
        for (i, art) in ALL.iter().enumerate() {
            for (y, row) in art.rows.iter().enumerate() {
                assert!(
                    row.len() == W || row.len() == W / 2,
                    "sprite {i} row {y} is {} chars: {row:?}",
                    row.len()
                );
                for c in row.chars() {
                    assert!(c == '.' || parse(c).is_some(), "sprite {i} row {y}: bad char {c:?}");
                }
            }
            let filled = (0..H).flat_map(|y| (0..W).map(move |x| (x, y))).filter(|&(x, y)| art.pixel(x, y).is_some()).count();
            assert!(filled > 60, "sprite {i} is too sparse ({filled} pixels)");
        }
    }

    #[test]
    fn half_rows_mirror() {
        // Gloom Moth's first row is "oo....o." mirrored.
        let left: Vec<_> = (0..W / 2).map(|x| GLOOM_MOTH.pixel(x, 0)).collect();
        let right: Vec<_> = (W / 2..W).rev().map(|x| GLOOM_MOTH.pixel(x, 0)).collect();
        assert_eq!(left, right);
        assert_eq!(GLOOM_MOTH.pixel(0, 0), Some(Pixel::Outline));
        assert_eq!(GLOOM_MOTH.pixel(15, 0), Some(Pixel::Outline));
    }

    #[test]
    fn creatures_with_pupils_have_eyes_to_blink() {
        for art in ALL {
            let has = |p| (0..H).any(|y| (0..W).any(|x| art.pixel(x, y) == Some(p)));
            if has(Pixel::Pupil) {
                assert!(has(Pixel::EyeWhite));
            }
        }
    }
}
