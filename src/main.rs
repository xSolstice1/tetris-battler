mod app;
mod audio;
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

use app::App;

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

#[macroquad::main(window_conf)]
async fn main() {
    // Effects randomness only; each game has its own seeded RNG.
    macroquad::rand::srand((macroquad::miniquad::date::now() * 1000.0) as u64);

    let mut app = App::new().await;
    loop {
        // Clamp so a stalled frame (window drag, breakpoint) doesn't teleport pieces.
        app.frame(get_frame_time().min(0.05));
        next_frame().await;
    }
}
