//! Screen flow: main menu -> a play session (with pause / game over) -> menu.
//! Owns things that outlive a session: audio and the motion setting.

use macroquad::miniquad::CursorIcon;
use macroquad::miniquad::window::set_mouse_cursor;
use macroquad::prelude::*;

use crate::audio::Audio;
use crate::battle::Battle;
use crate::fx::{Fx, Motion};
use crate::game::Game;
use crate::input::Input;
use crate::render::{self, MenuHit, MenuItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
    Battler,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Classic, Mode::Battler];

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
}

impl Session {
    fn new(mode: Mode, motion: Motion) -> Self {
        let mut game = Game::new(new_seed());
        let battle = (mode == Mode::Battler).then(|| Battle::new(&mut game));
        Session {
            mode,
            game,
            fx: Fx::new(motion),
            input: Input::default(),
            paused: false,
            battle,
        }
    }

    /// The run has ended: topped out, or (Battler) out of time.
    fn over(&self) -> bool {
        self.game.game_over || self.battle.as_ref().is_some_and(Battle::over)
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

        if !self.over() {
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
            self.fx.handle(&event);
        }
        if let Some(battle) = &mut self.battle {
            battle.update(dt, &mut self.game);
            for event in battle.take_events() {
                audio.handle_battle(&event);
            }
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
        self.draw();
    }

    fn global_keys(&mut self) {
        if is_key_pressed(KeyCode::M) {
            self.toggle_motion();
        }
        if is_key_pressed(KeyCode::N) {
            self.audio.toggle_mute();
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
                    return Some(Screen::Play(Box::new(Session::new(mode, self.motion))));
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
                    return Some(Screen::Play(Box::new(Session::new(Mode::ALL[*selected], self.motion))));
                }
                None
            }
            Screen::Play(session) => {
                let to_menu = Screen::Menu {
                    selected: Mode::ALL.iter().position(|&m| m == session.mode).unwrap_or(0),
                };
                if session.over() {
                    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R) {
                        return Some(Screen::Play(Box::new(Session::new(session.mode, self.motion))));
                    }
                    if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
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

    fn draw(&self) {
        let sound_on = !self.audio.muted();
        match &self.screen {
            Screen::Menu { selected } => {
                let items: Vec<MenuItem> = Mode::ALL.iter().map(|m| m.menu_item()).collect();
                render::draw_menu(&items, *selected, self.hover, sound_on, self.motion);
            }
            Screen::Play(s) => render::draw(&s.game, s.paused, &s.fx, sound_on, s.battle.as_ref()),
        }
    }
}
