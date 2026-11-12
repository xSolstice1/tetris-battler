mod bag;
mod board;
#[cfg(debug_assertions)]
mod debug;
mod fx;
mod game;
mod input;
mod piece;
mod render;

use macroquad::prelude::*;

use fx::Fx;
use game::Game;
use input::Input;

fn window_conf() -> Conf {
    Conf {
        window_title: "Block Brawl".to_owned(),
        window_width: 760,
        window_height: 760,
        window_resizable: true,
        high_dpi: true,
        ..Default::default()
    }
}

fn new_seed() -> u64 {
    (macroquad::miniquad::date::now() * 1000.0) as u64
}

#[macroquad::main(window_conf)]
async fn main() {
    // Effects randomness only; the game has its own seeded RNG.
    macroquad::rand::srand(new_seed());

    let mut game = Game::new(new_seed());
    let mut input = Input::default();
    let mut fx = Fx::default();
    let mut paused = false;

    loop {
        // Clamp so a stalled frame (window drag, breakpoint) doesn't teleport pieces.
        let dt = get_frame_time().min(0.05);

        if is_key_pressed(KeyCode::M) {
            fx.set_motion(fx.motion().toggled());
        }

        if game.game_over {
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::R) {
                game = Game::new(new_seed());
                input.reset();
                fx = Fx::new(fx.motion());
            }
        } else if is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Escape) {
            paused = !paused;
            input.reset();
        }

        if !paused {
            #[cfg(debug_assertions)]
            debug::handle_keys(&mut game);

            if !game.game_over {
                for action in input.poll(dt) {
                    game.apply(action);
                }
                game.tick(dt);
            }
            for event in game.take_events() {
                fx.handle(&event);
            }
            fx.follow_active(game.active.y, game.grounded(), dt);
            fx.update(dt);
        }

        render::draw(&game, paused, &fx);
        next_frame().await;
    }
}
