//! Screen flow: main menu -> a play session (with pause / game over) -> menu.
//! Owns things that outlive a session: audio and the motion / scene-effects settings.

use macroquad::miniquad::CursorIcon;
use macroquad::miniquad::window::set_mouse_cursor;
use macroquad::prelude::*;

use crate::audio::{Audio, Mood, Track};
use crate::battle::{Battle, BattleEvent};
use crate::bot::SHADOWS;
use crate::online::{Msg, PARTY_MAX, PROTOCOL, RoomKind, Snapshot};
use crate::versus::{Versus, VsEvent};
use crate::fx::{Effects, Fx};
use crate::settings::{Bind, Settings};
use crate::settings_ui::{self, Extra, Kind};
use crate::game::{Event, Game};
use crate::input::Input;
use crate::progress::{Progress, Reward, Run, RunKind};
use crate::render::{self, MenuHit, MenuItem, SkinsHit};
use crate::scene::{Scene, Theme};
use crate::skins;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
    Battler,
    /// A 1v1 against a shadow (index into `bot::SHADOWS`).
    Shadow,
    /// A 1v1 against another player, through a room (see `online.rs`).
    Online,
}

impl Mode {
    fn menu_item(self) -> MenuItem {
        match self {
            Mode::Classic => MenuItem {
                title: "CLASSIC",
                blurb: "Endless stacking. Clear lines, level up, chase a high score.",
            },
            Mode::Battler => MenuItem {
                title: "BATTLER",
                blurb: "Beat a gauntlet of monsters before time runs out. They hit back with garbage.",
            },
            Mode::Online => MenuItem {
                title: "ONLINE",
                blurb: "Battle friends live: a 1v1, or a party of up to 10. Share the room code.",
            },
            Mode::Shadow => MenuItem {
                title: "SHADOW 1V1",
                blurb: "Pick a shadow and play a true 1v1: same pieces, garbage both ways.",
            },
        }
    }
}

/// A main menu entry: a mode to play, or the skins screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Play(Mode),
    Skins,
    /// The settings screen (the button beside SOUND, not a list item).
    Settings,
}

const MENU: [Entry; 5] = [Entry::Play(Mode::Classic), Entry::Play(Mode::Battler), Entry::Play(Mode::Shadow), Entry::Play(Mode::Online), Entry::Skins];

impl Entry {
    fn menu_item(self) -> MenuItem {
        match self {
            Entry::Play(mode) => mode.menu_item(),
            Entry::Settings => MenuItem {
                title: "SETTINGS",
                blurb: "Handling, effects, audio and keys.",
            },
            Entry::Skins => MenuItem {
                title: "SKINS",
                blurb: "Block styles, boards and colours. Level up and beat shadows to unlock more.",
            },
        }
    }
}

fn menu_index(entry: Entry) -> usize {
    MENU.iter().position(|&e| e == entry).unwrap_or(0)
}

fn new_seed() -> u64 {
    (macroquad::miniquad::date::now() * 1000.0) as u64
}

/// One run of a mode, from start to game over.
struct Session {
    mode: Mode,
    game: Game,
    fx: Fx,
    input: Input,
    paused: bool,
    battle: Option<Battle>,
    versus: Option<Versus>,
    /// The shadow faced (Shadow mode), for a rematch.
    shadow: usize,
    /// Seconds until the next danger heartbeat.
    beat: f32,
    /// Everything off the field: backdrop, shots, side callouts, meters.
    scene: Scene,
    /// What the run earned, booked once it ends.
    reward: Option<Reward>,
    /// Online: the room, when our field last went out, whether it changed since, and
    /// who's ready for a rematch.
    room: Option<String>,
    snap_timer: f32,
    net_dirty: bool,
    rematch_me: bool,
    rematch_them: bool,
    /// The host's start for the next match (a rematch), once it arrives.
    next_start: Option<u64>,
    /// Party: our id and name, everyone else in the room (id, name; for the next match),
    /// and the host's next match once it arrives.
    me: String,
    my_name: String,
    names: Vec<(String, String)>,
    next_party: Option<(u64, Vec<(String, String)>)>,
}

impl Session {
    fn new(mode: Mode, opts: &Settings, shadow: usize) -> Self {
        let seed = new_seed();
        let mut game = Game::new(seed);
        let battle = (mode == Mode::Battler).then(|| Battle::new(&mut game));
        let versus = (mode == Mode::Shadow).then(|| Versus::new(SHADOWS[shadow.min(SHADOWS.len() - 1)], seed, &mut game));
        let theme = match (&battle, &versus) {
            (Some(b), _) => Theme::monster(b.monster.color),
            (_, Some(vs)) => Theme::shadow(&vs.profile),
            _ => Theme::classic(),
        };
        Session {
            mode,
            game,
            fx: Fx::new(opts.effects),
            input: Input::new(opts),
            paused: false,
            battle,
            versus,
            shadow,
            beat: 0.0,
            scene: Scene::new(opts.scene, theme),
            reward: None,
            room: None,
            snap_timer: 0.0,
            net_dirty: true,
            rematch_me: false,
            rematch_them: false,
            next_start: None,
            me: String::new(),
            my_name: String::new(),
            names: Vec::new(),
            next_party: None,
        }
    }

    /// A party match in `room`: everyone in `roster` (us included), all from `seed`.
    fn new_party(seed: u64, roster: &[(String, String)], room: String, me: String, my_name: String, opts: &Settings) -> Self {
        let mut game = Game::new(seed);
        let versus = Versus::new_party(&me, roster, seed, &mut game);
        let names = roster.iter().filter(|(id, _)| *id != me).cloned().collect();
        Session { versus: Some(versus), me, my_name, names, ..Session::new_online(seed, String::new(), room, opts) }
    }

    fn party(&self) -> bool {
        self.versus.as_ref().is_some_and(|v| v.party.is_some())
    }

    /// An online match in `room` against `opponent`, both fields from `seed`.
    fn new_online(seed: u64, opponent: String, room: String, opts: &Settings) -> Self {
        let mut game = Game::new(seed);
        let versus = Versus::new_remote(opponent, seed, &mut game);
        Session {
            mode: Mode::Online,
            game,
            fx: Fx::new(opts.effects),
            input: Input::new(opts),
            paused: false,
            battle: None,
            scene: Scene::new(opts.scene, Theme::shadow(&versus.profile)),
            versus: Some(versus),
            shadow: 0,
            beat: 0.0,
            reward: None,
            room: Some(room),
            snap_timer: 0.0,
            net_dirty: true,
            rematch_me: false,
            rematch_them: false,
            next_start: None,
            me: String::new(),
            my_name: String::new(),
            names: Vec::new(),
            next_party: None,
        }
    }

    /// Online: handle what arrived, send what's due (garbage, KO, our field).
    fn sync(&mut self, dt: f32) {
        if self.room.is_none() {
            return;
        }
        let party = self.party();
        while let Some((from, text)) = crate::net::poll() {
            let Some(msg) = Msg::decode(&text) else {
                continue;
            };
            match msg {
                Msg::Rematch => self.rematch_them = true,
                Msg::Start { seed } => self.next_start = Some(seed),
                // Party: someone new in the room (they'll be in the next match).
                Msg::Hello { name, protocol: PROTOCOL, .. } if party => {
                    if !self.names.iter().any(|(id, _)| *id == from) {
                        self.names.push((from, name));
                        crate::net::send(&Msg::Hello { name: self.my_name.clone(), protocol: PROTOCOL, room: RoomKind::Party }.encode());
                    }
                }
                Msg::Hello { .. } => {}
                Msg::PartyStart { seed, roster } => {
                    if roster.iter().any(|(id, _)| *id == self.me) {
                        self.next_party = Some((seed, roster));
                    }
                }
                m => {
                    if let Some(vs) = &mut self.versus {
                        if party {
                            vs.on_party(&from, &m, &mut self.game);
                        } else {
                            vs.on_remote(&m, &mut self.game);
                        }
                    }
                }
            }
        }
        if let Some(vs) = &mut self.versus {
            if party {
                let here = crate::net::roster();
                vs.party_present(&here);
            } else if crate::net::peers() < 2 && vs.fighting() {
                // Someone left the room mid-match: they forfeit.
                vs.opponent_left();
            }
            for m in vs.take_outbox() {
                crate::net::send(&m.encode());
            }
        }
        // Our field: on every change (locks, holds, garbage), and twice a second anyway
        // so their view of our falling piece keeps up. A party sends less (everyone gets
        // everyone's), and nothing once we're out.
        self.snap_timer -= dt;
        let (beat, gap) = if party { (1.0, 0.75) } else { (0.5, 0.0) };
        let due = (self.net_dirty && self.snap_timer <= gap) || self.snap_timer <= 0.0;
        if due && !(party && self.game.game_over && !self.net_dirty) {
            crate::net::send(&Msg::Snap(Snapshot::of(&self.game)).encode());
            self.net_dirty = false;
            self.snap_timer = beat;
        }
    }

    /// The finished run, for progression.
    fn run(&self) -> Run {
        let s = &self.game.stats;
        let kind = match (&self.battle, &self.versus) {
            (Some(b), _) => RunKind::Battler { defeated: b.defeated },
            (_, Some(vs)) if vs.party.is_some() => {
                let p = vs.party.as_ref().map_or((1, 2), |p| (p.my_place.unwrap_or(p.alive()), p.players()));
                RunKind::Party { place: p.0, players: p.1 }
            }
            (_, Some(vs)) if vs.online() => RunKind::Online { won: vs.player_won() },
            (_, Some(vs)) => RunKind::Shadow { shadow: self.shadow, won: vs.player_won(), time: vs.time },
            _ => RunKind::Classic { score: self.game.score },
        };
        Run { kind, lines: s.lines, attack: s.attack, pieces: s.pieces, apm: s.apm(), pps: s.pps() }
    }

    /// The run has ended: topped out, (Battler) out of time, or (Shadow) someone topped out.
    fn over(&self) -> bool {
        self.game.game_over || self.battle.as_ref().is_some_and(Battle::over) || self.versus.as_ref().is_some_and(Versus::over)
    }

    /// The player's pieces fall (not during a 1v1's countdown).
    fn live(&self) -> bool {
        !self.over() && self.versus.as_ref().is_none_or(Versus::fighting)
    }

    /// Takes changed settings (from the settings screen) without restarting the run.
    fn apply(&mut self, opts: &Settings) {
        self.input.apply(opts);
        self.fx.set_effects(opts.effects);
        self.scene.set_on(opts.scene);
    }

    fn update(&mut self, dt: f32, audio: &Audio, opts: &Settings) {
        if self.paused {
            return;
        }

        #[cfg(debug_assertions)]
        {
            crate::debug::handle_keys(&mut self.game);
            let skip = is_key_pressed(KeyCode::F5) || is_key_pressed(KeyCode::RightBracket);
            if let (true, Some(battle)) = (skip, &mut self.battle) {
                battle.skip_monster(&mut self.game);
            }
        }

        if self.live() {
            // IRS / IHS: keys already held (not just pressed) apply as the next piece spawns.
            self.game.initial = crate::game::Initial {
                rotate: if self.input.held(Bind::RotateCw) {
                    Some(true)
                } else if self.input.held(Bind::RotateCcw) {
                    Some(false)
                } else {
                    None
                },
                hold: self.input.held(Bind::Hold),
            };
            for action in self.input.poll(dt) {
                self.game.apply(action);
            }
            self.game.stats.key_presses = self.input.presses();
            self.game.tick(dt);
        }
        for event in self.game.take_events() {
            if let Some(battle) = &mut self.battle {
                battle.on_game_event(&event, &mut self.game);
            }
            if let Some(vs) = &mut self.versus {
                vs.on_player_event(&event);
            }
            if matches!(event, Event::Locked { .. } | Event::Held | Event::GarbageRose { .. } | Event::GameOver) {
                self.net_dirty = true;
            }
            self.fx.handle(&event);
        }
        if let Some(vs) = &mut self.versus {
            vs.update(dt, &mut self.game);
            for event in vs.take_events() {
                self.scene.on_versus(&event);
                match event {
                    VsEvent::Won => audio.handle_battle(&BattleEvent::Defeated { tier: 0 }),
                    // Topping out already played the field's own sting.
                    VsEvent::Lost | VsEvent::Go | VsEvent::Sent { .. } | VsEvent::Received { .. } => {}
                }
            }
        }
        if let Some(battle) = &mut self.battle {
            battle.update(dt, &mut self.game);
            for event in battle.take_events() {
                audio.handle_battle(&event);
            }
            self.scene.set_theme(Theme::monster(battle.monster.color));
        }
        // Near the top: a heartbeat, quickening as the danger grows.
        let danger = if self.live() && opts.danger { crate::danger::level(&self.game) } else { 0.0 };
        if danger > 0.0 && opts.heartbeat {
            self.beat -= dt;
            if self.beat <= 0.0 {
                audio.heartbeat(danger);
                self.beat = crate::danger::beat_interval(danger);
            }
        } else {
            self.beat = 0.0;
        }
        self.fx.follow_active(self.game.active.y, self.game.grounded(), dt);
        self.fx.update(dt);
        for event in self.fx.take_presented() {
            audio.handle(&event);
            self.scene.on_event(&event);
        }
        self.scene.update(dt, &self.game, danger);
    }
}

enum Screen {
    Menu { selected: usize },
    /// Choosing a shadow for a 1v1.
    Select { selected: usize },
    /// Browsing skins: the focused column and row.
    Skins { col: usize, row: usize },
    Play(Box<Session>),
    /// The settings screen: tab, focused row, focused key slot (controls), the action
    /// waiting for a key, and where Esc goes back to (a paused run, or the menu).
    Settings { tab: usize, row: usize, slot: usize, rebinding: Option<(Bind, usize)>, back: Option<Box<Session>> },
    /// Finding an online match.
    Online(Lobby),
}

/// The online lobby's steps.
#[derive(Clone, Debug, PartialEq)]
pub enum Lobby {
    /// Create a 1v1 room, create a party, or join one (focused option).
    Choose { focus: usize },
    /// Typing a friend's room code.
    Typing { code: String },
    /// In a room: what it's for (known to whoever made it; guests learn it from hellos),
    /// everyone else there whose name we know (id, name), and anything wrong.
    /// `here` is who's present right now (a name can arrive before its presence does).
    Room { code: String, kind: RoomKind, names: Vec<(String, String)>, here: Vec<String>, hello_in: f32, problem: Option<&'static str> },
}

/// Who plays the next party match: us first, then the others in the room (at most
/// `PARTY_MAX` in all).
fn party_roster(me: &str, my_name: &str, names: &[(String, String)]) -> Vec<(String, String)> {
    std::iter::once((me.to_owned(), my_name.to_owned())).chain(names.iter().cloned()).take(PARTY_MAX).collect()
}

/// A settings screen opened from the paused run (filled in once the run is moved over).
fn settings_screen(back: Option<Box<Session>>) -> Screen {
    Screen::Settings { tab: 0, row: 0, slot: 0, rebinding: None, back }
}

pub struct App {
    screen: Screen,
    audio: Audio,
    /// Fullscreen right now (not saved: the browser decides on the next visit).
    fullscreen: bool,
    /// Settings screen element under the mouse.
    settings_hover: Option<render::SettingsHit>,
    /// The run to put into the settings screen opened from pause.
    pending_back: bool,
    /// This player's name online.
    player_name: String,
    /// Menu element under the mouse this frame.
    hover: Option<MenuHit>,
    /// Skins screen element under the mouse.
    skins_hover: Option<SkinsHit>,
    last_mouse: Vec2,
    /// XP, records, unlocks and the equipped skin (saved).
    progress: Progress,
}

impl App {
    pub async fn new() -> Self {
        let progress = crate::save::load();
        skins::set_current(progress.skin);
        App {
            screen: Screen::Menu { selected: 0 },
            audio: Audio::load().await,
            fullscreen: false,
            settings_hover: None,
            pending_back: false,
            player_name: format!("PLAYER {}", 100 + (new_seed() % 900)),
            hover: None,
            skins_hover: None,
            last_mouse: Vec2::ZERO,
            progress,
        }
    }

    /// Books a run that just ended (once) and saves.
    fn book_run(&mut self) {
        let Screen::Play(s) = &mut self.screen else {
            return;
        };
        if s.reward.is_some() || !s.over() {
            return;
        }
        let key = (s.mode == Mode::Shadow).then(|| SHADOWS[s.shadow.min(SHADOWS.len() - 1)].portrait);
        // (Online matches only earn XP; they keep no per-opponent record.)
        s.reward = Some(self.progress.finish(&s.run(), key));
        crate::save::store(&self.progress);
    }

    pub fn frame(&mut self, dt: f32) {
        self.global_keys();
        let s = &self.progress.settings;
        self.audio.set_gains(s.sfx_gain(), s.music_gain());
        if let Some(next) = self.update(dt) {
            let old = std::mem::replace(&mut self.screen, next);
            // Opening settings from pause: the paused run comes along, to go back to.
            if std::mem::take(&mut self.pending_back) {
                if let (Screen::Settings { back, .. }, Screen::Play(run)) = (&mut self.screen, old) {
                    *back = Some(run);
                }
            }
        }
        let mood = self.mood();
        self.audio.update_music(dt, &mood);
        // On the menus, get the highlighted option's music ready before it's picked.
        let next = match self.screen {
            Screen::Menu { selected } => match MENU[selected] {
                Entry::Play(Mode::Classic) => Some(Track::Classic),
                Entry::Play(Mode::Battler) => Some(Track::Battler),
                _ => None,
            },
            Screen::Select { selected } => Some(Track::shadow(selected, SHADOWS.len())),
            Screen::Play(_) | Screen::Skins { .. } | Screen::Settings { .. } | Screen::Online(_) => None,
        };
        if let Some(track) = next {
            self.audio.prefetch_music(track);
        }
        self.draw();
    }

    /// What the music should be doing on this screen.
    fn mood(&self) -> Mood {
        let Screen::Play(s) = &self.screen else {
            return Mood::menu();
        };
        let track = match s.mode {
            Mode::Classic => Track::Classic,
            Mode::Battler => Track::Battler,
            Mode::Shadow => Track::shadow(s.shadow, SHADOWS.len()),
            Mode::Online => Track::shadow(5, SHADOWS.len()),
        };
        Mood::play(track, &s.game, s.battle.as_ref(), s.versus.as_ref(), s.paused)
    }

    /// Quick keys: M (competitive <-> everything), E (scene), N (mute), J (music). Not on
    /// the settings screen, and not if the player has bound that key to a game action.
    fn global_keys(&mut self) {
        if matches!(self.screen, Screen::Settings { .. }) {
            return;
        }
        let keys = self.progress.settings.keys.clone();
        let free = |k: KeyCode| is_key_pressed(k) && !keys.bound(k);
        if free(KeyCode::M) {
            self.toggle_motion();
        }
        if free(KeyCode::E) {
            self.toggle_scene();
        }
        if free(KeyCode::N) {
            self.audio.toggle_mute();
        }
        if free(KeyCode::J) {
            self.audio.toggle_music();
        }
    }

    /// Settings changed: the run in play (or paused behind the settings screen) takes them
    /// at once, and they're saved.
    fn settings_changed(&mut self) {
        let opts = self.progress.settings.clone();
        match &mut self.screen {
            Screen::Play(s) => s.apply(&opts),
            Screen::Settings { back: Some(s), .. } => s.apply(&opts),
            _ => {}
        }
        crate::save::store(&self.progress);
    }

    fn toggle_scene(&mut self) {
        self.progress.settings.scene = !self.progress.settings.scene;
        self.settings_changed();
    }

    fn toggle_motion(&mut self) {
        let e = &mut self.progress.settings.effects;
        *e = if *e == Effects::FULL { Effects::COMPETITIVE } else { Effects::FULL };
        self.settings_changed();
    }

    /// Mouse on the menu: hovering an option selects it (only when the mouse
    /// actually moved, so it doesn't fight the keyboard), clicking activates it.
    /// Returns the entry to open, if an option was clicked.
    fn menu_mouse(&mut self) -> Option<Entry> {
        let mouse = Vec2::from(mouse_position());
        let moved = mouse != self.last_mouse;
        self.last_mouse = mouse;
        self.hover = render::menu_hit(mouse, MENU.len());
        set_mouse_cursor(if self.hover.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });

        if let (true, Some(MenuHit::Item(i)), Screen::Menu { selected }) = (moved, self.hover, &mut self.screen) {
            *selected = i;
        }
        if !is_mouse_button_pressed(MouseButton::Left) {
            return None;
        }
        match self.hover? {
            MenuHit::Item(i) => Some(MENU[i]),
            MenuHit::Settings => Some(Entry::Settings),
            MenuHit::Sound => {
                self.audio.toggle_mute();
                None
            }
        }
    }

    /// Returns the screen to switch to, if any.
    fn update(&mut self, dt: f32) -> Option<Screen> {
        match &mut self.screen {
            Screen::Menu { .. } => {
                if let Some(entry) = self.menu_mouse() {
                    set_mouse_cursor(CursorIcon::Default);
                    return Some(self.open(entry));
                }
                let Screen::Menu { selected } = &mut self.screen else {
                    unreachable!()
                };
                let count = MENU.len();
                if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
                    *selected = (*selected + 1) % count;
                }
                if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
                    *selected = (*selected + count - 1) % count;
                }
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
                    let entry = MENU[*selected];
                    return Some(self.open(entry));
                }
                None
            }
            Screen::Select { .. } => self.select_update(),
            Screen::Skins { .. } => self.skins_update(),
            Screen::Settings { .. } => self.settings_update(),
            Screen::Play(session) if session.room.is_some() => {
                let room = session.room.clone().unwrap_or_default();
                session.update(dt, &self.audio, &self.progress.settings);
                session.sync(dt);
                self.book_run();
                let Screen::Play(session) = &mut self.screen else {
                    return None;
                };
                if session.party() {
                    // The host started the next match (we're in it).
                    if let Some((seed, roster)) = session.next_party.take() {
                        let (me, name) = (session.me.clone(), session.my_name.clone());
                        return Some(Screen::Play(Box::new(Session::new_party(seed, &roster, room, me, name, &self.progress.settings))));
                    }
                    if session.over() {
                        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
                            crate::net::leave();
                            return Some(Screen::Menu { selected: menu_index(Entry::Play(Mode::Online)) });
                        }
                        let finished = session.versus.as_ref().and_then(|v| v.party.as_ref()).is_some_and(|p| p.finished());
                        let here = crate::net::roster();
                        let names: Vec<(String, String)> = session.names.iter().filter(|(id, _)| here.contains(id)).cloned().collect();
                        let ready = finished && crate::net::is_host() && !names.is_empty();
                        if ready && (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R)) {
                            let roster = party_roster(&session.me, &session.my_name, &names);
                            let seed = new_seed();
                            crate::net::send(&Msg::PartyStart { seed, roster: roster.clone() }.encode());
                            let (me, name) = (session.me.clone(), session.my_name.clone());
                            return Some(Screen::Play(Box::new(Session::new_party(seed, &roster, room, me, name, &self.progress.settings))));
                        }
                    }
                    return None;
                }
                if session.over() {
                    // Rematch: both press Enter; the host then starts a fresh match.
                    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R) {
                        session.rematch_me = true;
                        crate::net::send(&Msg::Rematch.encode());
                    }
                    if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
                        crate::net::leave();
                        return Some(Screen::Menu { selected: menu_index(Entry::Play(Mode::Online)) });
                    }
                    let name = session.versus.as_ref().map(|v| v.name().to_owned()).unwrap_or_default();
                    if session.rematch_me && session.rematch_them && crate::net::is_host() {
                        let seed = new_seed();
                        crate::net::send(&Msg::Start { seed }.encode());
                        return Some(Screen::Play(Box::new(Session::new_online(seed, name, room, &self.progress.settings))));
                    }
                    // The guest starts when the host's start arrives.
                    if session.rematch_me && !crate::net::is_host() {
                        if let Some(seed) = session.next_start.take() {
                            return Some(Screen::Play(Box::new(Session::new_online(seed, name, room, &self.progress.settings))));
                        }
                    }
                }
                None
            }
            Screen::Online(_) => self.online_update(dt),
            Screen::Play(session) => {
                let to_menu = Screen::Menu { selected: menu_index(Entry::Play(session.mode)) };
                if session.over() {
                    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R) {
                        return Some(Screen::Play(Box::new(Session::new(session.mode, &self.progress.settings, session.shadow))));
                    }
                    if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
                        // From a 1v1, back to the shadow list.
                        if session.mode == Mode::Shadow {
                            return Some(Screen::Select { selected: session.shadow });
                        }
                        return Some(to_menu);
                    }
                } else if session.paused {
                    if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) {
                        session.paused = false;
                        session.input.reset();
                    } else if is_key_pressed(KeyCode::Q) {
                        return Some(to_menu);
                    } else if is_key_pressed(KeyCode::O) {
                        self.pending_back = true;
                        return Some(settings_screen(None));
                    }
                } else if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) {
                    session.paused = true;
                    session.input.reset();
                }
                session.update(dt, &self.audio, &self.progress.settings);
                self.book_run();
                None
            }
        }
    }

    fn open(&self, entry: Entry) -> Screen {
        match entry {
            Entry::Play(mode) => self.start(mode, 0),
            Entry::Settings => settings_screen(None),
            Entry::Skins => {
                // Start on the equipped block style.
                let row = render::skin_column(0).iter().position(|i| i.equipped_in(self.progress.skin)).unwrap_or(0);
                Screen::Skins { col: 0, row }
            }
        }
    }

    /// The skins screen: arrows / mouse to browse, Enter / click to equip, Esc back.
    fn skins_update(&mut self) -> Option<Screen> {
        let mouse = Vec2::from(mouse_position());
        let moved = mouse != self.last_mouse;
        self.last_mouse = mouse;
        self.skins_hover = render::skins_hit(mouse);
        set_mouse_cursor(if self.skins_hover.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
        let back = Screen::Menu { selected: menu_index(Entry::Skins) };
        let Screen::Skins { col, row } = &mut self.screen else {
            return None;
        };
        if let (true, Some(SkinsHit::Item { col: c, row: r })) = (moved, self.skins_hover) {
            (*col, *row) = (c, r);
        }
        let columns = render::SKIN_COLUMNS.len();
        let rows = |c: usize| render::skin_column(c).len();
        if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
            *col = (*col + 1) % columns;
            *row = (*row).min(rows(*col) - 1);
        }
        if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
            *col = (*col + columns - 1) % columns;
            *row = (*row).min(rows(*col) - 1);
        }
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            *row = (*row + 1) % rows(*col);
        }
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            *row = (*row + rows(*col) - 1) % rows(*col);
        }
        let clicked = is_mouse_button_pressed(MouseButton::Left).then_some(self.skins_hover).flatten();
        if clicked == Some(SkinsHit::Back) || is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            set_mouse_cursor(CursorIcon::Default);
            return Some(back);
        }
        let equip = matches!(clicked, Some(SkinsHit::Item { .. })) || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space);
        if equip {
            let item = render::skin_column(*col)[*row];
            if self.progress.equip(item) {
                skins::set_current(self.progress.skin);
                crate::save::store(&self.progress);
            }
        }
        None
    }

    /// Shadow mode opens the opponent list; the others start straight away.
    fn start(&self, mode: Mode, shadow: usize) -> Screen {
        if mode == Mode::Shadow {
            Screen::Select { selected: shadow }
        } else if mode == Mode::Online {
            Screen::Online(Lobby::Choose { focus: 0 })
        } else {
            Screen::Play(Box::new(Session::new(mode, &self.progress.settings, 0)))
        }
    }

    /// The opponent list: keys or mouse to choose, Enter / click to fight, Esc back.
    fn select_update(&mut self) -> Option<Screen> {
        let Screen::Select { selected } = &mut self.screen else {
            return None;
        };
        let count = SHADOWS.len();
        let mouse = Vec2::from(mouse_position());
        let moved = mouse != self.last_mouse;
        self.last_mouse = mouse;
        self.hover = render::select_hit(mouse, count);
        set_mouse_cursor(if self.hover.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
        if let (true, Some(MenuHit::Item(i))) = (moved, self.hover) {
            *selected = i;
        }
        let clicked = is_mouse_button_pressed(MouseButton::Left) && self.hover.is_some();
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            *selected = (*selected + 1) % count;
        }
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            *selected = (*selected + count - 1) % count;
        }
        if clicked || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            set_mouse_cursor(CursorIcon::Default);
            return Some(Screen::Play(Box::new(Session::new(Mode::Shadow, &self.progress.settings, *selected))));
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            return Some(Screen::Menu { selected: menu_index(Entry::Play(Mode::Shadow)) });
        }
        None
    }

    /// The online lobby: create a room (a code to share) or type a friend's code, then wait
    /// in the room until the other player is there; the host starts the match.
    fn online_update(&mut self, dt: f32) -> Option<Screen> {
        let Screen::Online(lobby) = &mut self.screen else {
            return None;
        };
        let back = Screen::Menu { selected: menu_index(Entry::Play(Mode::Online)) };
        match lobby {
            Lobby::Choose { focus } => {
                if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
                    *focus = (*focus + 1) % 3;
                }
                if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
                    *focus = (*focus + 2) % 3;
                }
                if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
                    return Some(back);
                }
                let hit = render::online_hit(Vec2::from(mouse_position()));
                if let Some(i) = hit {
                    *focus = i;
                }
                let pick = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) || (is_mouse_button_pressed(MouseButton::Left) && hit.is_some())).then_some(*focus);
                let room = |kind| {
                    let code = crate::online::room_code(new_seed());
                    crate::net::join(&code);
                    Lobby::Room { code, kind, names: Vec::new(), here: Vec::new(), hello_in: 0.0, problem: None }
                };
                match pick {
                    Some(0) => *lobby = room(RoomKind::Duel),
                    Some(1) => *lobby = room(RoomKind::Party),
                    Some(_) => *lobby = Lobby::Typing { code: String::new() },
                    None => {}
                }
                None
            }
            Lobby::Typing { code } => {
                while let Some(ch) = get_char_pressed() {
                    *code = crate::online::clean_code(&format!("{code}{ch}"));
                }
                if is_key_pressed(KeyCode::Backspace) {
                    code.pop();
                }
                if is_key_pressed(KeyCode::Escape) {
                    *lobby = Lobby::Choose { focus: 2 };
                } else if is_key_pressed(KeyCode::Enter) && code.len() == 4 {
                    let code = code.clone();
                    crate::net::join(&code);
                    *lobby = Lobby::Room { code, kind: RoomKind::Unknown, names: Vec::new(), here: Vec::new(), hello_in: 0.0, problem: None };
                }
                None
            }
            Lobby::Room { code, kind, names, here: present, hello_in, problem } => {
                if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
                    crate::net::leave();
                    return Some(back);
                }
                match crate::net::status() {
                    crate::net::Status::Unavailable => *problem = Some("Online play isn't set up on this build."),
                    crate::net::Status::Error => *problem = Some("Couldn't reach the server. Esc and try again."),
                    _ => {}
                }
                let joined = crate::net::status() == crate::net::Status::Joined;
                let me = crate::net::me();
                let here = crate::net::roster();
                present.clone_from(&here);
                let seat = here.iter().position(|id| *id == me).unwrap_or(0);
                match *kind {
                    RoomKind::Duel if here.len() > 2 && seat >= 2 => *problem = Some("That room is full. Esc and make a new one."),
                    RoomKind::Party if here.len() > PARTY_MAX && seat >= PARTY_MAX => *problem = Some("That party is full (10 players). Esc and try another."),
                    _ => {}
                }
                // Say hello (and keep saying it until we know who's there).
                *hello_in -= dt;
                let unknown = here.iter().any(|id| *id != me && !names.iter().any(|(n, _)| n == id));
                if joined && unknown && *hello_in <= 0.0 {
                    crate::net::send(&Msg::Hello { name: self.player_name.clone(), protocol: PROTOCOL, room: *kind }.encode());
                    *hello_in = 1.0;
                }
                let mut start = None;
                let mut party_start = None;
                while let Some((from, text)) = crate::net::poll() {
                    match Msg::decode(&text) {
                        Some(Msg::Hello { name, protocol, room }) => {
                            if protocol != PROTOCOL {
                                *problem = Some("Someone here has a different version. Everyone reload the page.");
                                continue;
                            }
                            if *kind == RoomKind::Unknown && room != RoomKind::Unknown {
                                *kind = room;
                            }
                            match names.iter_mut().find(|(id, _)| *id == from) {
                                Some(entry) => entry.1 = name,
                                None => {
                                    // Make sure they know us too.
                                    names.push((from, name));
                                    crate::net::send(&Msg::Hello { name: self.player_name.clone(), protocol: PROTOCOL, room: *kind }.encode());
                                }
                            }
                        }
                        Some(Msg::Start { seed }) => start = Some(seed),
                        Some(Msg::PartyStart { seed, roster }) if roster.iter().any(|(id, _)| *id == me) => party_start = Some((seed, roster)),
                        _ => {}
                    }
                }
                if problem.is_some() {
                    return None;
                }
                let room = code.clone();
                let opts = &self.progress.settings;
                let names: Vec<(String, String)> = names.iter().filter(|(id, _)| here.contains(id)).cloned().collect();
                match *kind {
                    RoomKind::Unknown => None,
                    RoomKind::Party => {
                        if let Some((seed, roster)) = party_start {
                            return Some(Screen::Play(Box::new(Session::new_party(seed, &roster, room, me, self.player_name.clone(), opts))));
                        }
                        // The host starts once someone else is here.
                        if crate::net::is_host() && !names.is_empty() && is_key_pressed(KeyCode::Enter) {
                            let roster = party_roster(&me, &self.player_name, &names);
                            let seed = new_seed();
                            crate::net::send(&Msg::PartyStart { seed, roster: roster.clone() }.encode());
                            return Some(Screen::Play(Box::new(Session::new_party(seed, &roster, room, me, self.player_name.clone(), opts))));
                        }
                        None
                    }
                    RoomKind::Duel => {
                        let name = names.first()?.1.clone();
                        if crate::net::is_host() {
                            let seed = new_seed();
                            crate::net::send(&Msg::Start { seed }.encode());
                            return Some(Screen::Play(Box::new(Session::new_online(seed, name, room, opts))));
                        }
                        let seed = start?;
                        Some(Screen::Play(Box::new(Session::new_online(seed, name, room, opts))))
                    }
                }
            }
        }
    }

    fn extra(&self) -> Extra {
        Extra { fullscreen: self.fullscreen, muted: self.audio.muted() }
    }

    /// The settings screen: Up / Down rows, Left / Right adjust (or pick a key slot),
    /// Enter toggles / rebinds, Tab or [ ] switch tabs, Esc back. Mouse: tabs, rows,
    /// slider drags, key slots, Back.
    fn settings_update(&mut self) -> Option<Screen> {
        let mouse = Vec2::from(mouse_position());
        let moved = mouse != self.last_mouse;
        self.last_mouse = mouse;
        let Screen::Settings { tab, row, rebinding, .. } = &self.screen else {
            return None;
        };
        let (tab_now, row_now) = (*tab, *row);
        self.settings_hover = render::settings_hit(mouse, tab_now);
        set_mouse_cursor(if self.settings_hover.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });

        // Waiting for a key to bind.
        if let Some((bind, slot)) = *rebinding {
            if let Some(key) = get_last_key_pressed() {
                let s = &mut self.progress.settings;
                match key {
                    KeyCode::Escape => {}
                    KeyCode::Backspace | KeyCode::Delete => {
                        // Never leave an action with no key at all.
                        if s.keys.get(bind)[1 - slot].is_some() {
                            s.keys.set(bind, slot, None);
                        }
                    }
                    k if crate::settings::BINDABLE.contains(&k) => s.keys.set(bind, slot, Some(k)),
                    _ => {}
                }
                if let Screen::Settings { rebinding, .. } = &mut self.screen {
                    *rebinding = None;
                }
                self.settings_changed();
            }
            return None;
        }

        let rows = settings_ui::rows(tab_now);
        let mut extra = self.extra();
        let before = (self.progress.settings.clone(), extra);
        let mut back = false;
        let mut new_tab = None;
        let mut new_row = None;
        let mut new_slot = None;
        let mut rebind = None;
        // Keyboard.
        if is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::RightBracket) {
            new_tab = Some((tab_now + if is_key_down(KeyCode::LeftShift) { settings_ui::TABS.len() - 1 } else { 1 }) % settings_ui::TABS.len());
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            new_tab = Some((tab_now + settings_ui::TABS.len() - 1) % settings_ui::TABS.len());
        }
        if is_key_pressed(KeyCode::Down) {
            new_row = Some((row_now + 1) % rows.len());
        }
        if is_key_pressed(KeyCode::Up) {
            new_row = Some((row_now + rows.len() - 1) % rows.len());
        }
        let focused = rows[row_now.min(rows.len() - 1)];
        let dir = is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32;
        if dir != 0 {
            match focused.kind() {
                Kind::Key(_) => new_slot = Some(if dir > 0 { 1 } else { 0 }),
                _ => focused.adjust(&mut self.progress.settings, &mut extra, dir),
            }
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            match focused.kind() {
                Kind::Key(b) => {
                    let slot = if let Screen::Settings { slot, .. } = &self.screen { *slot } else { 0 };
                    rebind = Some((b, slot));
                }
                _ => focused.activate(&mut self.progress.settings, &mut extra),
            }
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            back = true;
        }
        // Mouse.
        use render::SettingsHit as H;
        if let (true, Some(H::Row(r) | H::Slider(r, _) | H::KeySlot(r, _))) = (moved, self.settings_hover) {
            new_row = Some(r);
        }
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let dragging = is_mouse_button_down(MouseButton::Left);
        match self.settings_hover {
            Some(H::Tab(t)) if clicked => new_tab = Some(t),
            Some(H::Back) if clicked => back = true,
            Some(H::Slider(r, f)) if dragging => rows[r].set_fraction(&mut self.progress.settings, f),
            Some(H::KeySlot(r, slot)) if clicked => {
                if let Kind::Key(b) = rows[r].kind() {
                    rebind = Some((b, slot));
                    new_slot = Some(slot);
                }
            }
            Some(H::Row(r)) if clicked => rows[r].activate(&mut self.progress.settings, &mut extra),
            _ => {}
        }

        if extra.fullscreen != before.1.fullscreen {
            self.fullscreen = extra.fullscreen;
            set_fullscreen(self.fullscreen);
        }
        if extra.muted != before.1.muted {
            self.audio.toggle_mute();
        }
        if self.progress.settings != before.0 {
            self.settings_changed();
        }
        if let Screen::Settings { tab, row, slot, rebinding, .. } = &mut self.screen {
            if let Some(t) = new_tab {
                *tab = t;
                *row = 0;
            }
            if let Some(r) = new_row {
                *row = r.min(settings_ui::rows(*tab).len() - 1);
            }
            if let Some(s) = new_slot {
                *slot = s;
            }
            if rebind.is_some() {
                *rebinding = rebind;
            }
        }
        if back {
            set_mouse_cursor(CursorIcon::Default);
            let Screen::Settings { back, .. } = std::mem::replace(&mut self.screen, Screen::Menu { selected: 0 }) else {
                unreachable!()
            };
            return Some(match back {
                Some(run) => Screen::Play(run),
                None => Screen::Menu { selected: 0 },
            });
        }
        None
    }

    fn draw(&self) {
        let sound_on = !self.audio.muted();
        match &self.screen {
            Screen::Menu { selected } => {
                let items: Vec<MenuItem> = MENU.iter().map(|e| e.menu_item()).collect();
                render::draw_menu(&items, *selected, self.hover, sound_on, self.progress.settings.scene, &self.progress);
                render::warm_portraits();
            }
            Screen::Select { selected } => render::draw_select(&SHADOWS, *selected, self.hover, self.progress.settings.scene, &self.progress),
            Screen::Online(lobby) => render::draw_online(lobby, &self.player_name, &self.progress, crate::net::is_host()),
            Screen::Settings { tab, row, slot, rebinding, back } => {
                render::draw_settings(&self.progress.settings, self.extra(), *tab, *row, *slot, *rebinding, self.settings_hover, back.is_some())
            }
            Screen::Skins { col, row } => render::draw_skins(&self.progress, *col, *row, self.skins_hover),
            Screen::Play(s) => render::draw(&s.game, s.paused, &s.fx, &s.scene, sound_on, self.audio.music_on(), &self.progress.settings, s.battle.as_ref(), s.versus.as_ref(), s.reward.as_ref()),
        }
    }
}

