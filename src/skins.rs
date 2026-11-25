//! Cosmetic skins: how blocks are drawn (style), the field's colours (theme) and the
//! piece colours (palette). All static: a skin never adds motion to the board.
//!
//! The equipped skin is a per-thread global so every drawing path (the field, the queue,
//! effects) picks it up without threading it through each call.

use std::cell::Cell;

/// What it takes to unlock a skin item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlock {
    Free,
    /// Reach this player level.
    Level(u32),
    /// Beat the shadow with this portrait key (see `bot::SHADOWS`).
    Beat(&'static str),
}

/// How a single block is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlockStyle {
    /// Bevelled: dark rim, a light band along the top.
    #[default]
    Classic,
    Flat,
    Outline,
    /// Jelly: rounded shine, darker bottom.
    Glossy,
    /// 8-bit: light top-left edge, dark bottom-right, a corner glint.
    Pixel,
    /// Dark core with bright edges (a still glow, no flicker).
    Neon,
    /// Deon's circuit: a dark green chip with the piece colour at its heart.
    Deon,
    /// Faceted like a cut gem.
    Crystal,
}

impl BlockStyle {
    pub const ALL: [BlockStyle; 8] = [
        BlockStyle::Classic,
        BlockStyle::Flat,
        BlockStyle::Outline,
        BlockStyle::Glossy,
        BlockStyle::Pixel,
        BlockStyle::Neon,
        BlockStyle::Deon,
        BlockStyle::Crystal,
    ];

    pub fn id(self) -> &'static str {
        match self {
            BlockStyle::Classic => "classic",
            BlockStyle::Flat => "flat",
            BlockStyle::Outline => "outline",
            BlockStyle::Glossy => "glossy",
            BlockStyle::Pixel => "pixel",
            BlockStyle::Neon => "neon",
            BlockStyle::Deon => "deon",
            BlockStyle::Crystal => "crystal",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BlockStyle::Classic => "CLASSIC",
            BlockStyle::Flat => "FLAT",
            BlockStyle::Outline => "OUTLINE",
            BlockStyle::Glossy => "JELLY",
            BlockStyle::Pixel => "RETRO",
            BlockStyle::Neon => "NEON",
            BlockStyle::Deon => "DEON",
            BlockStyle::Crystal => "CRYSTAL",
        }
    }

    pub fn unlock(self) -> Unlock {
        match self {
            BlockStyle::Classic => Unlock::Free,
            BlockStyle::Flat => Unlock::Level(2),
            BlockStyle::Outline => Unlock::Level(4),
            BlockStyle::Glossy => Unlock::Level(8),
            BlockStyle::Pixel => Unlock::Level(12),
            BlockStyle::Neon => Unlock::Level(15),
            BlockStyle::Deon => Unlock::Beat("deon"),
            BlockStyle::Crystal => Unlock::Beat("deon_extreme"),
        }
    }
}

/// Colours of the field and the panels around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColors {
    /// Behind everything.
    pub backdrop: u32,
    pub field: u32,
    pub grid: u32,
    pub frame: u32,
    /// Hold / next panels.
    pub panel: u32,
    pub panel_line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BoardTheme {
    #[default]
    Midnight,
    Paper,
    Sakura,
    Terminal,
    Burrow,
    Jade,
    Glacier,
    Sunset,
    Inferno,
}

impl BoardTheme {
    pub const ALL: [BoardTheme; 9] = [
        BoardTheme::Midnight,
        BoardTheme::Paper,
        BoardTheme::Sakura,
        BoardTheme::Terminal,
        BoardTheme::Burrow,
        BoardTheme::Jade,
        BoardTheme::Glacier,
        BoardTheme::Sunset,
        BoardTheme::Inferno,
    ];

    pub fn id(self) -> &'static str {
        match self {
            BoardTheme::Midnight => "midnight",
            BoardTheme::Paper => "paper",
            BoardTheme::Sakura => "sakura",
            BoardTheme::Terminal => "terminal",
            BoardTheme::Burrow => "burrow",
            BoardTheme::Jade => "jade",
            BoardTheme::Glacier => "glacier",
            BoardTheme::Sunset => "sunset",
            BoardTheme::Inferno => "inferno",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BoardTheme::Midnight => "MIDNIGHT",
            BoardTheme::Paper => "PAPER",
            BoardTheme::Sakura => "SAKURA",
            BoardTheme::Terminal => "TERMINAL",
            BoardTheme::Burrow => "BURROW",
            BoardTheme::Jade => "JADE",
            BoardTheme::Glacier => "GLACIER",
            BoardTheme::Sunset => "SUNSET",
            BoardTheme::Inferno => "INFERNO",
        }
    }

    pub fn unlock(self) -> Unlock {
        match self {
            BoardTheme::Midnight => Unlock::Free,
            BoardTheme::Paper => Unlock::Level(3),
            BoardTheme::Sakura => Unlock::Level(6),
            BoardTheme::Terminal => Unlock::Level(10),
            BoardTheme::Burrow => Unlock::Beat("jianjie"),
            BoardTheme::Jade => Unlock::Beat("zhuang_hao"),
            BoardTheme::Glacier => Unlock::Beat("luoyi"),
            BoardTheme::Sunset => Unlock::Beat("luoyi_extreme"),
            BoardTheme::Inferno => Unlock::Beat("deon_nightmare"),
        }
    }

    pub fn colors(self) -> ThemeColors {
        let t = |backdrop, field, grid, frame, panel, panel_line| ThemeColors { backdrop, field, grid, frame, panel, panel_line };
        match self {
            BoardTheme::Midnight => t(0x0b0d14, 0x10131c, 0x181c28, 0x262c3d, 0x141824, 0x262c3d),
            BoardTheme::Paper => t(0x17150f, 0xece5d3, 0xdad1bb, 0xb3a68a, 0x221f18, 0x4a4334),
            BoardTheme::Sakura => t(0x140c12, 0x221420, 0x321f2d, 0xd97a9c, 0x1d1219, 0x5a3348),
            BoardTheme::Terminal => t(0x040905, 0x06110a, 0x0e2814, 0x23893c, 0x07130a, 0x1a4a26),
            BoardTheme::Burrow => t(0x120e0a, 0x1e1812, 0x2b2219, 0x8a6c50, 0x1a150f, 0x41342a),
            BoardTheme::Jade => t(0x07110d, 0x0c1b15, 0x163026, 0x4fae78, 0x0b1712, 0x24503b),
            BoardTheme::Glacier => t(0x09101a, 0x0e1926, 0x1a2b3e, 0x7fc8ec, 0x0d1622, 0x2a4560),
            BoardTheme::Sunset => t(0x140a0d, 0x22111a, 0x351b26, 0xff7a45, 0x1d0f15, 0x5a2a30),
            BoardTheme::Inferno => t(0x110403, 0x1b0705, 0x2e0e09, 0xff3b1f, 0x190605, 0x521a10),
        }
    }
}

/// Piece colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Palette {
    #[default]
    Guideline,
    /// Okabe-Ito based: tells pieces apart with the common colour blindnesses.
    Contrast,
    Vivid,
    /// Luoyi Angel's soft pastels.
    Halo,
}

impl Palette {
    pub const ALL: [Palette; 4] = [Palette::Guideline, Palette::Contrast, Palette::Vivid, Palette::Halo];

    pub fn id(self) -> &'static str {
        match self {
            Palette::Guideline => "guideline",
            Palette::Contrast => "contrast",
            Palette::Vivid => "vivid",
            Palette::Halo => "halo",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Palette::Guideline => "STANDARD",
            Palette::Contrast => "CONTRAST",
            Palette::Vivid => "VIVID",
            Palette::Halo => "HALO",
        }
    }

    pub fn unlock(self) -> Unlock {
        match self {
            // Accessibility is never locked.
            Palette::Guideline | Palette::Contrast => Unlock::Free,
            Palette::Vivid => Unlock::Level(5),
            Palette::Halo => Unlock::Beat("luoyi_angel"),
        }
    }

    /// Colours for I, O, T, S, Z, J, L (in `PieceKind` order).
    pub fn colors(self) -> [u32; 7] {
        match self {
            Palette::Guideline => [0x2ee6d6, 0xffd23f, 0xc061ff, 0x5cdb5c, 0xff4f6d, 0x4f8bff, 0xff9a3c],
            Palette::Contrast => [0x56b4e9, 0xf0e442, 0xcc79a7, 0x009e73, 0xd55e00, 0x0072b2, 0xe69f00],
            Palette::Vivid => [0x00f0ff, 0xffe600, 0xb026ff, 0x2bff4a, 0xff1f4b, 0x2958ff, 0xff7b00],
            Palette::Halo => [0xa8eef0, 0xffeaa0, 0xdcb8ff, 0xb8efb4, 0xffb6c4, 0xaec8ff, 0xffcf9e],
        }
    }
}

/// Everything the player has equipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Skin {
    pub style: BlockStyle,
    pub theme: BoardTheme,
    pub palette: Palette,
}

/// One unlockable thing, of any category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Style(BlockStyle),
    Theme(BoardTheme),
    Palette(Palette),
}

impl Item {
    pub fn all() -> impl Iterator<Item = Item> {
        BlockStyle::ALL
            .into_iter()
            .map(Item::Style)
            .chain(BoardTheme::ALL.into_iter().map(Item::Theme))
            .chain(Palette::ALL.into_iter().map(Item::Palette))
    }

    pub fn name(self) -> &'static str {
        match self {
            Item::Style(s) => s.name(),
            Item::Theme(t) => t.name(),
            Item::Palette(p) => p.name(),
        }
    }

    /// "blocks", "board" or "colours".
    pub fn kind(self) -> &'static str {
        match self {
            Item::Style(_) => "blocks",
            Item::Theme(_) => "board",
            Item::Palette(_) => "colours",
        }
    }

    pub fn unlock(self) -> Unlock {
        match self {
            Item::Style(s) => s.unlock(),
            Item::Theme(t) => t.unlock(),
            Item::Palette(p) => p.unlock(),
        }
    }

    /// `skin` with this item put on.
    pub fn apply(self, skin: Skin) -> Skin {
        match self {
            Item::Style(style) => Skin { style, ..skin },
            Item::Theme(theme) => Skin { theme, ..skin },
            Item::Palette(palette) => Skin { palette, ..skin },
        }
    }

    pub fn equipped_in(self, skin: Skin) -> bool {
        self.apply(skin) == skin
    }
}

thread_local! {
    static CURRENT: Cell<Skin> = const { Cell::new(Skin { style: BlockStyle::Classic, theme: BoardTheme::Midnight, palette: Palette::Guideline }) };
}

/// The skin everything is drawn with.
pub fn current() -> Skin {
    CURRENT.with(Cell::get)
}

pub fn set_current(skin: Skin) {
    CURRENT.with(|c| c.set(skin));
}

/// Runs `f` drawing with `skin` (e.g. a preview), then puts the equipped skin back.
pub fn with<R>(skin: Skin, f: impl FnOnce() -> R) -> R {
    let before = current();
    set_current(skin);
    let out = f();
    set_current(before);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_within_each_category() {
        let mut ids: Vec<_> = BlockStyle::ALL.iter().map(|s| s.id()).collect();
        ids.dedup();
        assert_eq!(ids.len(), BlockStyle::ALL.len());
        let themes: std::collections::HashSet<_> = BoardTheme::ALL.iter().map(|t| t.id()).collect();
        assert_eq!(themes.len(), BoardTheme::ALL.len());
        let palettes: std::collections::HashSet<_> = Palette::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(palettes.len(), Palette::ALL.len());
    }

    #[test]
    fn defaults_are_free_and_every_beat_unlock_names_a_real_shadow() {
        let d = Skin::default();
        for item in [Item::Style(d.style), Item::Theme(d.theme), Item::Palette(d.palette)] {
            assert_eq!(item.unlock(), Unlock::Free);
        }
        for item in Item::all() {
            if let Unlock::Beat(key) = item.unlock() {
                assert!(crate::bot::SHADOWS.iter().any(|p| p.portrait == key), "{key}");
            }
        }
    }

    #[test]
    fn every_shadow_unlocks_something() {
        for p in crate::bot::SHADOWS {
            assert!(Item::all().any(|i| i.unlock() == Unlock::Beat(p.portrait)), "{}", p.name);
        }
    }

    #[test]
    fn with_restores_the_equipped_skin() {
        let preview = Skin { style: BlockStyle::Neon, ..Skin::default() };
        set_current(Skin::default());
        assert_eq!(with(preview, current), preview);
        assert_eq!(current(), Skin::default());
    }
}
