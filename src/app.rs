//! Screen flow: main menu -> a play session (with pause / game over) -> menu.
//! Owns things that outlive a session: audio and the motion / scene-effects settings.

use macroquad::miniquad::CursorIcon;
use macroquad::miniquad::window::set_mouse_cursor;
use macroquad::prelude::*;

use crate::audio::Audio;
use crate::battle::{Battle, BattleEvent};
use crate::bot::SHADOWS;
use crate::versus::{Versus, VsEvent};
use crate::fx::{Fx, Motion};
use crate::game::Game;
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
}

const MENU: [Entry; 4] = [Entry::Play(Mode::Classic), Entry::Play(Mode::Battler), Entry::Play(Mode::Shadow), Entry::Skins];

impl Entry {
    fn menu_item(self) -> MenuItem {
        match self {
            Entry::Play(mode) => mode.menu_item(),
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
}

impl Session {
    fn new(mode: Mode, motion: Motion, scene_fx: bool, shadow: usize) -> Self {
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
            fx: Fx::new(motion),
            input: Input::default(),
            paused: false,
            battle,
            versus,
            shadow,
            beat: 0.0,
            scene: Scene::new(scene_fx, theme),
            reward: None,
        }
    }

    /// The finished run, for progression.
    fn run(&self) -> Run {
        let s = &self.game.stats;
        let kind = match (&self.battle, &self.versus) {
            (Some(b), _) => RunKind::Battler { defeated: b.defeated },
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

    fn update(&mut self, dt: f32, audio: &Audio) {
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
        let danger = if self.live() { crate::danger::level(&self.game) } else { 0.0 };
        if danger > 0.0 {
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
}

pub struct App {
    screen: Screen,
    audio: Audio,
    motion: Motion,
    /// Scene effects (everything off the field) animate. On by default.
    scene_fx: bool,
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
            motion: Motion::default(),
            scene_fx: true,
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
        s.reward = Some(self.progress.finish(&s.run(), key));
        crate::save::store(&self.progress);
    }

    pub fn frame(&mut self, dt: f32) {
        self.global_keys();
        if let Some(next) = self.update(dt) {
            self.screen = next;
        }
        self.draw();
    }

    fn global_keys(&mut self) {
        if is_key_pressed(KeyCode::M) {
            self.toggle_motion();
        }
        if is_key_pressed(KeyCode::E) {
            self.toggle_scene();
        }
        if is_key_pressed(KeyCode::N) {
            self.audio.toggle_mute();
        }
    }

    fn toggle_scene(&mut self) {
        self.scene_fx = !self.scene_fx;
        if let Screen::Play(s) = &mut self.screen {
            s.scene.set_on(self.scene_fx);
        }
    }

    fn toggle_motion(&mut self) {
        self.motion = self.motion.toggled();
        if let Screen::Play(s) = &mut self.screen {
            s.fx.set_motion(self.motion);
        }
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
            MenuHit::Motion => {
                self.toggle_motion();
                None
            }
            MenuHit::Scene => {
                self.toggle_scene();
                None
            }
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
            Screen::Play(session) => {
                let to_menu = Screen::Menu { selected: menu_index(Entry::Play(session.mode)) };
                if session.over() {
                    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R) {
                        return Some(Screen::Play(Box::new(Session::new(session.mode, self.motion, self.scene_fx, session.shadow))));
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
                    }
                } else if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) {
                    session.paused = true;
                    session.input.reset();
                }
                session.update(dt, &self.audio);
                self.book_run();
                None
            }
        }
    }

    fn open(&self, entry: Entry) -> Screen {
        match entry {
            Entry::Play(mode) => self.start(mode, 0),
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
        } else {
            Screen::Play(Box::new(Session::new(mode, self.motion, self.scene_fx, 0)))
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
            return Some(Screen::Play(Box::new(Session::new(Mode::Shadow, self.motion, self.scene_fx, *selected))));
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            return Some(Screen::Menu { selected: menu_index(Entry::Play(Mode::Shadow)) });
        }
        None
    }

    fn draw(&self) {
        let sound_on = !self.audio.muted();
        match &self.screen {
            Screen::Menu { selected } => {
                let items: Vec<MenuItem> = MENU.iter().map(|e| e.menu_item()).collect();
                render::draw_menu(&items, *selected, self.hover, sound_on, self.motion, self.scene_fx, &self.progress);
            }
            Screen::Select { selected } => render::draw_select(&SHADOWS, *selected, self.hover, self.scene_fx, &self.progress),
            Screen::Skins { col, row } => render::draw_skins(&self.progress, *col, *row, self.skins_hover),
            Screen::Play(s) => render::draw(&s.game, s.paused, &s.fx, &s.scene, sound_on, s.battle.as_ref(), s.versus.as_ref(), s.reward.as_ref()),
        }
    }
}
