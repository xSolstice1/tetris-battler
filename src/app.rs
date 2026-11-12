//! Screen flow: main menu -> a play session (with pause / game over) -> menu.
//! Owns things that outlive a session: audio and the motion setting.

use macroquad::prelude::*;

use crate::audio::Audio;
use crate::fx::{Fx, Motion};
use crate::game::Game;
use crate::input::Input;
use crate::render::{self, MenuItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
}

impl Mode {
    pub const ALL: [Mode; 1] = [Mode::Classic];

    fn menu_item(self) -> MenuItem {
        match self {
            Mode::Classic => MenuItem {
                title: "CLASSIC",
                blurb: "Endless stacking. Clear lines, level up, chase a high score.",
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
}

impl Session {
    fn new(mode: Mode, motion: Motion) -> Self {
        Session {
            mode,
            game: Game::new(new_seed()),
            fx: Fx::new(motion),
            input: Input::default(),
            paused: false,
        }
    }

    fn over(&self) -> bool {
        self.game.game_over
    }

    fn update(&mut self, dt: f32, audio: &Audio) {
        if self.paused {
            return;
        }

        #[cfg(debug_assertions)]
        crate::debug::handle_keys(&mut self.game);

        if !self.game.game_over {
            for action in self.input.poll(dt) {
                self.game.apply(action);
            }
            self.game.tick(dt);
        }
        for event in self.game.take_events() {
            self.fx.handle(&event);
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
}

impl App {
    pub async fn new() -> Self {
        App {
            screen: Screen::Menu { selected: 0 },
            audio: Audio::load().await,
            motion: Motion::default(),
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
            self.motion = self.motion.toggled();
            if let Screen::Play(s) = &mut self.screen {
                s.fx.set_motion(self.motion);
            }
        }
        if is_key_pressed(KeyCode::N) {
            self.audio.toggle_mute();
        }
    }

    /// Returns the screen to switch to, if any.
    fn update(&mut self, dt: f32) -> Option<Screen> {
        match &mut self.screen {
            Screen::Menu { selected } => {
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
                render::draw_menu(&items, *selected, sound_on, self.motion);
            }
            Screen::Play(s) => render::draw(&s.game, s.paused, &s.fx, sound_on),
        }
    }
}
