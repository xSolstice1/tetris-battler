//! Screen flow: main menu -> a play session (with pause / game over) -> menu.
//! Owns things that outlive a session: audio and the motion setting.

use macroquad::miniquad::CursorIcon;
use macroquad::miniquad::window::set_mouse_cursor;
use macroquad::prelude::*;

use crate::audio::{Audio, Mood, Track};
use crate::battle::{Battle, BattleEvent};
use crate::bot::SHADOWS;
use crate::versus::{Versus, VsEvent};
use crate::fx::{Fx, Motion};
use crate::game::Game;
use crate::input::Input;
use crate::render::{self, MenuHit, MenuItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
    Battler,
    /// A 1v1 against a shadow (index into `bot::SHADOWS`).
    Shadow,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Classic, Mode::Battler, Mode::Shadow];

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
}

impl Session {
    fn new(mode: Mode, motion: Motion, shadow: usize) -> Self {
        let seed = new_seed();
        let mut game = Game::new(seed);
        let battle = (mode == Mode::Battler).then(|| Battle::new(&mut game));
        let versus = (mode == Mode::Shadow).then(|| Versus::new(SHADOWS[shadow.min(SHADOWS.len() - 1)], seed, &mut game));
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
        }
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
                match event {
                    VsEvent::Won => audio.handle_battle(&BattleEvent::Defeated { tier: 0 }),
                    // Topping out already played the field's own sting.
                    VsEvent::Lost | VsEvent::Go => {}
                }
            }
        }
        if let Some(battle) = &mut self.battle {
            battle.update(dt, &mut self.game);
            for event in battle.take_events() {
                audio.handle_battle(&event);
            }
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
        }
    }
}

enum Screen {
    Menu { selected: usize },
    /// Choosing a shadow for a 1v1.
    Select { selected: usize },
    Play(Box<Session>),
}

pub struct App {
    screen: Screen,
    audio: Audio,
    motion: Motion,
    /// Menu element under the mouse this frame.
    hover: Option<MenuHit>,
    last_mouse: Vec2,
}

impl App {
    pub async fn new() -> Self {
        App {
            screen: Screen::Menu { selected: 0 },
            audio: Audio::load().await,
            motion: Motion::default(),
            hover: None,
            last_mouse: Vec2::ZERO,
        }
    }

    pub fn frame(&mut self, dt: f32) {
        self.global_keys();
        if let Some(next) = self.update(dt) {
            self.screen = next;
        }
        let mood = self.mood();
        self.audio.update_music(dt, &mood);
        // On the menus, get the highlighted option's music ready before it's picked.
        let next = match self.screen {
            Screen::Menu { selected } => match Mode::ALL[selected] {
                Mode::Classic => Some(Track::Classic),
                Mode::Battler => Some(Track::Battler),
                Mode::Shadow => None,
            },
            Screen::Select { selected } => Some(Track::shadow(selected, SHADOWS.len())),
            Screen::Play(_) => None,
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
        };
        Mood::play(track, &s.game, s.battle.as_ref(), s.versus.as_ref(), s.paused)
    }

    fn global_keys(&mut self) {
        if is_key_pressed(KeyCode::M) {
            self.toggle_motion();
        }
        if is_key_pressed(KeyCode::N) {
            self.audio.toggle_mute();
        }
        if is_key_pressed(KeyCode::J) {
            self.audio.toggle_music();
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
    /// Returns the mode to start, if an option was clicked.
    fn menu_mouse(&mut self) -> Option<Mode> {
        let mouse = Vec2::from(mouse_position());
        let moved = mouse != self.last_mouse;
        self.last_mouse = mouse;
        self.hover = render::menu_hit(mouse, Mode::ALL.len());
        set_mouse_cursor(if self.hover.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });

        if let (true, Some(MenuHit::Item(i)), Screen::Menu { selected }) = (moved, self.hover, &mut self.screen) {
            *selected = i;
        }
        if !is_mouse_button_pressed(MouseButton::Left) {
            return None;
        }
        match self.hover? {
            MenuHit::Item(i) => Some(Mode::ALL[i]),
            MenuHit::Motion => {
                self.toggle_motion();
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
                if let Some(mode) = self.menu_mouse() {
                    set_mouse_cursor(CursorIcon::Default);
                    return Some(self.start(mode, 0));
                }
                let Screen::Menu { selected } = &mut self.screen else {
                    unreachable!()
                };
                let count = Mode::ALL.len();
                if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
                    *selected = (*selected + 1) % count;
                }
                if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
                    *selected = (*selected + count - 1) % count;
                }
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
                    let mode = Mode::ALL[*selected];
                    return Some(self.start(mode, 0));
                }
                None
            }
            Screen::Select { .. } => self.select_update(),
            Screen::Play(session) => {
                let to_menu = Screen::Menu {
                    selected: Mode::ALL.iter().position(|&m| m == session.mode).unwrap_or(0),
                };
                if session.over() {
                    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R) {
                        return Some(Screen::Play(Box::new(Session::new(session.mode, self.motion, session.shadow))));
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
                None
            }
        }
    }

    /// Shadow mode opens the opponent list; the others start straight away.
    fn start(&self, mode: Mode, shadow: usize) -> Screen {
        if mode == Mode::Shadow {
            Screen::Select { selected: shadow }
        } else {
            Screen::Play(Box::new(Session::new(mode, self.motion, 0)))
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
            return Some(Screen::Play(Box::new(Session::new(Mode::Shadow, self.motion, *selected))));
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            let at = Mode::ALL.iter().position(|&m| m == Mode::Shadow).unwrap_or(0);
            return Some(Screen::Menu { selected: at });
        }
        None
    }

    fn draw(&self) {
        let sound_on = !self.audio.muted();
        match &self.screen {
            Screen::Menu { selected } => {
                let items: Vec<MenuItem> = Mode::ALL.iter().map(|m| m.menu_item()).collect();
                render::draw_menu(&items, *selected, self.hover, sound_on, self.motion);
            }
            Screen::Select { selected } => render::draw_select(&SHADOWS, *selected, self.hover),
            Screen::Play(s) => render::draw(&s.game, s.paused, &s.fx, sound_on, s.battle.as_ref(), s.versus.as_ref()),
        }
    }
}
