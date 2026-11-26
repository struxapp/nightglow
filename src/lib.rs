//! Nightglow: the whole game (simulation and rendering) in WebAssembly, with no imports.
//!
//! The page calls `resize` with the size of the pixel buffer it wants, then `tick` once a frame with the time passed
//! and the player's input, and copies `frame_ptr()` (RGBA, `width × height`) onto a canvas. `tick` returns sound
//! events as bit flags; everything else the page shows is read through small getters.
//!
//! Numbers shared with the page (`web/app.js` mirrors them in `STATE` and `EVENT`; a test pins them):
//!
//! | `state()` | meaning   |     | `tick()` bit | event     |
//! |-----------|-----------|-----|--------------|-----------|
//! | 0         | title     |     | 1            | pickup    |
//! | 1         | playing   |     | 2            | hit       |
//! | 2         | game over |     | 4            | dash      |
//! |           |           |     | 8            | new night |
//! |           |           |     | 16           | game over |

mod game;

pub use game::{Event, Events, Game, State};

use std::cell::RefCell;

thread_local! {
    static GAME: RefCell<Game> = RefCell::new(Game::new(320, 240, 1));
}

fn with<T>(f: impl FnOnce(&mut Game) -> T) -> T {
    GAME.with(|game| f(&mut game.borrow_mut()))
}

/// Sets the pixel buffer size (clamped to 160..=480 × 120..=480) and draws a frame at that size.
#[no_mangle]
pub extern "C" fn resize(width: u32, height: u32, seed: u32) {
    with(|g| g.resize(width as usize, height as usize, seed));
}

/// Starts a new run.
#[no_mangle]
pub extern "C" fn start(seed: u32) {
    with(|g| g.start(seed));
}

/// Ends the current run (the player quit). The game stops as at game over, with no game-over event.
/// Does nothing outside a run.
#[no_mangle]
pub extern "C" fn end_run() {
    with(|g| {
        g.end_run();
    });
}

/// 1 when the player asked for reduced motion (`prefers-reduced-motion`): no screen shake, flicker or blinking.
#[no_mangle]
pub extern "C" fn set_reduced_motion(on: u32) {
    with(|g| g.set_reduced_motion(on != 0));
}

/// Advances the game by `dt_ms` and draws a frame. `x`/`y` is the steering direction (-1..1 each);
/// `dash` is 1 on the frame the dash key or button is pressed. Returns the event bits (see the table above).
#[no_mangle]
pub extern "C" fn tick(dt_ms: f32, x: f32, y: f32, dash: u32) -> u32 {
    with(|g| g.tick(dt_ms / 1000.0, x, y, dash != 0).bits())
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
    with(|g| g.state() as u32)
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
