//! Nightglow: the whole game (simulation and rendering) in WebAssembly, with no imports.
//!
//! The page calls `resize` with the size of the pixel buffer it wants, then `tick` once a frame with the time passed
//! and the player's input, and copies `frame_ptr()` (RGBA, `width × height`) onto a canvas. `tick` returns sound
//! events as bit flags; everything else the page shows is read through small getters.

mod game;

pub use game::{Game, EVENT_DASH, EVENT_GAME_OVER, EVENT_HIT, EVENT_NIGHT, EVENT_PICKUP};

use std::cell::RefCell;

thread_local! {
    static GAME: RefCell<Game> = RefCell::new(Game::new(320, 240, 1));
}

fn with<T>(f: impl FnOnce(&mut Game) -> T) -> T {
    GAME.with(|game| f(&mut game.borrow_mut()))
}

/// Sets the pixel buffer size (clamped to 160..=480 × 120..=480) and starts the title scene.
#[no_mangle]
pub extern "C" fn resize(width: u32, height: u32, seed: u32) {
    with(|g| g.resize(width as usize, height as usize, seed));
}

#[no_mangle]
pub extern "C" fn start(seed: u32) {
    with(|g| g.start(seed));
}

/// Advances the game by `dt_ms` and draws a frame. `x`/`y` is the steering direction (-1..1 each);
/// `dash` is 1 on the frame the dash key or button is pressed.
#[no_mangle]
pub extern "C" fn tick(dt_ms: f32, x: f32, y: f32, dash: u32) -> u32 {
    with(|g| g.tick(dt_ms / 1000.0, x, y, dash != 0))
}

#[no_mangle]
pub extern "C" fn frame_ptr() -> *const u8 {
    with(|g| g.pixels().as_ptr())
}

#[no_mangle]
pub extern "C" fn width() -> u32 {
    with(|g| g.width() as u32)
}

#[no_mangle]
pub extern "C" fn height() -> u32 {
    with(|g| g.height() as u32)
}

/// 0 title, 1 playing, 2 game over.
#[no_mangle]
pub extern "C" fn state() -> u32 {
    with(|g| g.state())
}

#[no_mangle]
pub extern "C" fn score() -> u32 {
    with(|g| g.score())
}

/// The player's light, 0..=100.
#[no_mangle]
pub extern "C" fn light() -> f32 {
    with(|g| g.light())
}

#[no_mangle]
pub extern "C" fn night() -> u32 {
    with(|g| g.night())
}

#[no_mangle]
pub extern "C" fn fireflies_caught() -> u32 {
    with(|g| g.collected())
}

#[no_mangle]
pub extern "C" fn seconds() -> f32 {
    with(|g| g.time())
}

/// 0..=1: how ready the dash is.
#[no_mangle]
pub extern "C" fn dash_ready() -> f32 {
    with(|g| g.dash_ready())
}
