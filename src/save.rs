//! Saving progress. A small versioned key=value text:
//!
//! ```text
//! block-brawl-save 1
//! xp=1234
//! classic_best=56000
//! battle_best=4
//! style=neon
//! theme=inferno
//! palette=contrast
//! shadow.deon=3,1,83.20,41.5,1.32      (wins, losses, best win secs, best APM, best PPS; "-" = none)
//! ```
//!
//! Unknown keys and bad values are skipped, so a damaged or newer save still loads what
//! it can. In the browser it lives in localStorage (a tiny JS plugin in web/index.html);
//! natively in a file in the user's config directory.

use crate::progress::{Progress, ShadowRecord};
use crate::skins::{BlockStyle, BoardTheme, Palette};

const HEADER: &str = "block-brawl-save";
const VERSION: u32 = 1;

fn opt(v: Option<f32>, decimals: usize) -> String {
    v.map_or("-".to_owned(), |x| format!("{x:.decimals$}"))
}

fn parse_opt(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|x| x.is_finite() && *x >= 0.0)
}

pub fn to_text(p: &Progress) -> String {
    let mut out = format!("{HEADER} {VERSION}\n");
    out += &format!("xp={}\nclassic_best={}\nbattle_best={}\n", p.xp, p.classic_best, p.battle_best);
    out += &format!("style={}\ntheme={}\npalette={}\n", p.skin.style.id(), p.skin.theme.id(), p.skin.palette.id());
    out += &p.settings.to_lines();
    for (key, r) in &p.records {
        out += &format!(
            "shadow.{key}={},{},{},{},{}\n",
            r.wins,
            r.losses,
            opt(r.best_time, 2),
            opt(r.best_apm, 1),
            opt(r.best_pps, 2)
        );
    }
    out
}

/// Reads a save; anything missing or unreadable is left at its default.
pub fn from_text(text: &str) -> Progress {
    let mut p = Progress::default();
    let mut lines = text.lines();
    // Not ours at all (or empty): start fresh.
    if !lines.next().is_some_and(|l| l.trim().starts_with(HEADER)) {
        return p;
    }
    for line in lines {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if p.settings.read_line(key, value) {
            continue;
        }
        match key {
            "xp" => p.xp = value.parse().unwrap_or(p.xp),
            "classic_best" => p.classic_best = value.parse().unwrap_or(p.classic_best),
            "battle_best" => p.battle_best = value.parse().unwrap_or(p.battle_best),
            "style" => {
                if let Some(s) = BlockStyle::ALL.into_iter().find(|s| s.id() == value) {
                    p.skin.style = s;
                }
            }
            "theme" => {
                if let Some(t) = BoardTheme::ALL.into_iter().find(|t| t.id() == value) {
                    p.skin.theme = t;
                }
            }
            "palette" => {
                if let Some(c) = Palette::ALL.into_iter().find(|c| c.id() == value) {
                    p.skin.palette = c;
                }
            }
            _ => {
                if let Some(name) = key.strip_prefix("shadow.").filter(|n| !n.is_empty()) {
                    let f: Vec<&str> = value.split(',').collect();
                    let (Some(Ok(wins)), Some(Ok(losses))) = (f.first().map(|s| s.trim().parse()), f.get(1).map(|s| s.trim().parse())) else {
                        continue;
                    };
                    let field = |i: usize| f.get(i).and_then(|s| parse_opt(s));
                    let record = ShadowRecord { wins, losses, best_time: field(2), best_apm: field(3), best_pps: field(4) };
                    p.records.insert(name.to_owned(), record);
                }
            }
        }
    }
    p.sanitize();
    p
}

/// Loads saved progress (fresh progress if there is none).
pub fn load() -> Progress {
    storage::read().map(|t| from_text(&t)).unwrap_or_default()
}

pub fn store(p: &Progress) {
    storage::write(&to_text(p));
}

#[cfg(target_arch = "wasm32")]
mod storage {
    //! localStorage through the `bb_storage` plugin in web/index.html.
    unsafe extern "C" {
        /// Reads the save into the plugin's buffer; returns its length in bytes, or -1.
        fn bb_storage_load() -> i32;
        /// Copies the buffered save to `ptr` (which has room for the length returned above).
        fn bb_storage_take(ptr: *mut u8);
        fn bb_storage_store(ptr: *const u8, len: u32);
    }

    pub fn read() -> Option<String> {
        let len = unsafe { bb_storage_load() };
        if len < 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        unsafe { bb_storage_take(buf.as_mut_ptr()) };
        String::from_utf8(buf).ok()
    }

    pub fn write(text: &str) {
        unsafe { bb_storage_store(text.as_ptr(), text.len() as u32) };
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod storage {
    //! A file in the user's config directory (next to the executable as a last resort).
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let dir = if cfg!(windows) {
            env("APPDATA")
        } else if cfg!(target_os = "macos") {
            env("HOME").map(|h| h.join("Library/Application Support"))
        } else {
            env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
        };
        let dir = dir.map(|d| d.join("block-brawl")).or_else(|| std::env::current_exe().ok()?.parent().map(PathBuf::from))?;
        Some(dir.join("save.txt"))
    }

    pub fn read() -> Option<String> {
        std::fs::read_to_string(path()?).ok()
    }

    pub fn write(text: &str) {
        let Some(path) = path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write then rename, so a crash mid-write never leaves half a save.
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{Run, RunKind};

    fn sample() -> Progress {
        let mut p = Progress { xp: 5000, classic_best: 12345, battle_best: 3, ..Default::default() };
        let run = Run { kind: RunKind::Shadow { shadow: 5, won: true, time: 83.25 }, lines: 40, attack: 30, pieces: 100, apm: Some(41.5), pps: Some(1.32) };
        p.finish(&run, Some("deon"));
        p.records.insert("luoyi".to_owned(), ShadowRecord { wins: 0, losses: 2, best_time: None, best_apm: Some(12.0), best_pps: None });
        p.skin.style = BlockStyle::Deon;
        p.skin.theme = BoardTheme::Sakura;
        p.skin.palette = Palette::Contrast;
        p
    }

    #[test]
    fn round_trips() {
        let p = sample();
        let text = to_text(&p);
        assert!(text.starts_with("block-brawl-save 1\n"));
        let back = from_text(&text);
        assert_eq!(back.xp, p.xp);
        assert_eq!(back.classic_best, p.classic_best);
        assert_eq!(back.battle_best, p.battle_best);
        assert_eq!(back.skin, p.skin);
        assert_eq!(back.record("luoyi"), p.record("luoyi"));
        let (a, b) = (back.record("deon"), p.record("deon"));
        assert_eq!((a.wins, a.losses), (b.wins, b.losses));
        assert!((a.best_time.unwrap() - 83.25).abs() < 0.01);
        assert!((a.best_pps.unwrap() - 1.32).abs() < 0.01);
    }

    #[test]
    fn garbage_and_missing_saves_start_fresh() {
        assert_eq!(from_text(""), Progress::default());
        assert_eq!(from_text("\u{0}\u{1}not a save"), Progress::default());
        assert_eq!(from_text("xp=500\n"), Progress::default());
    }

    #[test]
    fn damaged_lines_are_skipped_and_the_rest_kept() {
        let text = "block-brawl-save 7\nxp=abc\nclassic_best=900\nfuture_key=1\nshadow.deon=x,1\nshadow.luoyi=2,3,NaN,-4,1.5\nshadow.=1,1\nstyle=nonsense\nnoequals\n";
        let p = from_text(text);
        assert_eq!(p.xp, 0);
        assert_eq!(p.classic_best, 900);
        assert!(!p.records.contains_key("deon"));
        assert!(!p.records.contains_key(""));
        let r = p.record("luoyi");
        assert_eq!((r.wins, r.losses, r.best_time, r.best_apm, r.best_pps), (2, 3, None, None, Some(1.5)));
        assert_eq!(p.skin.style, BlockStyle::Classic);
    }

    #[test]
    fn a_save_cannot_equip_what_it_has_not_unlocked() {
        let p = from_text("block-brawl-save 1\nxp=0\ntheme=inferno\npalette=contrast\n");
        assert_eq!(p.skin.theme, BoardTheme::Midnight);
        assert_eq!(p.skin.palette, Palette::Contrast);
    }
}
