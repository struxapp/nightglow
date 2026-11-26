//! The simulation. Units are buffer pixels and seconds. Drawing lives in `game/render.rs`.

mod render;

use render::Frame;
use std::f32::consts::TAU;
use std::ops::BitOrAssign;

/// Where the game is. The page reads it as a number (see the table in `lib.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum State {
    Title = 0,
    Playing = 1,
    Over = 2,
}

/// Something the page should play a sound for. Each is one bit of the number `tick` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Event {
    Pickup = 1,
    Hit = 2,
    Dash = 4,
    Night = 8,
    GameOver = 16,
}

/// The events of one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events(u32);

impl Events {
    pub fn has(self, event: Event) -> bool {
        self.0 & event as u32 != 0
    }

    pub fn bits(self) -> u32 {
        self.0
    }
}

impl From<Event> for Events {
    fn from(event: Event) -> Self {
        Events(event as u32)
    }
}

impl BitOrAssign for Events {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitOrAssign<Event> for Events {
    fn bitor_assign(&mut self, event: Event) {
        self.0 |= event as u32;
    }
}

const MAX_LIGHT: f32 = 100.0;
const PICKUP_LIGHT: f32 = 14.0;
const PICKUP_DISTANCE: f32 = 7.5;
const HIT_LIGHT: f32 = 22.0;
const HIT_DISTANCE: f32 = 8.0;
const DASH_COST: f32 = 6.0;
const DASH_COOLDOWN: f32 = 1.1;
const NIGHT_SECONDS: f32 = 25.0;
/// Above this much light the wisps back away from its edge instead of closing in.
const FEAR_LIGHT: f32 = 60.0;
const MAX_WISPS: usize = 12;
const MAX_PARTICLES: usize = 400;
const POINTS_PER_SECOND: u32 = 10;
const POINTS_PER_FIREFLY: u32 = 50;

/// xorshift32: small, fast and the same everywhere.
#[derive(Clone)]
pub(crate) struct Rng(u32);

impl Rng {
    pub(crate) fn new(seed: u32) -> Self {
        Rng(if seed == 0 { 0x9e37_79b9 } else { seed })
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0.0..1.0
    pub(crate) fn float(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub(crate) fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.float()
    }
}

#[derive(Clone, Copy)]
struct Firefly {
    x: f32,
    y: f32,
    phase: f32,
    vx: f32,
    vy: f32,
}

#[derive(Clone, Copy)]
struct Wisp {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    phase: f32,
}

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max_life: f32,
    color: [f32; 3],
}

pub struct Game {
    frame: Frame,
    rng: Rng,
    state: State,
    px: f32,
    py: f32,
    pvx: f32,
    pvy: f32,
    light: f32,
    invulnerable: f32,
    dash_cooldown: f32,
    fireflies: Vec<Firefly>,
    wisps: Vec<Wisp>,
    particles: Vec<Particle>,
    time: f32,
    clock: f32,
    over_time: f32,
    collected: u32,
    night: u32,
    shake: f32,
    /// The player asked for reduced motion: no shake, no flicker, no blinking.
    reduced_motion: bool,
}

impl Game {
    pub fn new(w: usize, h: usize, seed: u32) -> Self {
        let frame = Frame::new(w, h, seed);
        let (px, py) = (frame.w as f32 / 2.0, frame.h as f32 / 2.0);
        let mut game = Game {
            frame,
            rng: Rng::new(seed),
            state: State::Title,
            px,
            py,
            pvx: 0.0,
            pvy: 0.0,
            light: MAX_LIGHT,
            invulnerable: 0.0,
            dash_cooldown: 0.0,
            fireflies: Vec::new(),
            wisps: Vec::new(),
            particles: Vec::new(),
            time: 0.0,
            clock: 0.0,
            over_time: 0.0,
            collected: 0,
            night: 1,
            shake: 0.0,
            reduced_motion: false,
        };
        for _ in 0..8 {
            let f = game.new_firefly(0.0);
            game.fireflies.push(f);
        }
        game.render();
        game
    }

    /// Makes a new pixel buffer (clamped to 160..=480 × 120..=480) and moves everything to keep its place on it.
    /// The buffer holds a drawn frame straight away, so a paused game shows the scene, not black.
    pub fn resize(&mut self, w: usize, h: usize, seed: u32) {
        if !self.frame.needs_rebuild(w, h, seed) {
            return;
        }
        let (old_w, old_h) = (self.frame.w as f32, self.frame.h as f32);
        self.frame = Frame::new(w, h, seed);
        let (sx, sy) = (self.frame.w as f32 / old_w, self.frame.h as f32 / old_h);
        self.px *= sx;
        self.py *= sy;
        for f in &mut self.fireflies {
            f.x *= sx;
            f.y *= sy;
        }
        for wisp in &mut self.wisps {
            wisp.x *= sx;
            wisp.y *= sy;
        }
        for p in &mut self.particles {
            p.x *= sx;
            p.y *= sy;
        }
        self.render();
    }

    pub fn start(&mut self, seed: u32) {
        self.rng = Rng::new(seed);
        self.state = State::Playing;
        self.px = self.frame.w as f32 / 2.0;
        self.py = self.frame.h as f32 / 2.0;
        self.pvx = 0.0;
        self.pvy = 0.0;
        self.light = MAX_LIGHT;
        self.invulnerable = 1.0;
        self.dash_cooldown = 0.0;
        self.time = 0.0;
        self.over_time = 0.0;
        self.collected = 0;
        self.night = 1;
        self.shake = 0.0;
        self.particles.clear();
        self.wisps.clear();
        self.fireflies.clear();
        for _ in 0..6 {
            let f = self.new_firefly(50.0);
            self.fireflies.push(f);
        }
        for _ in 0..2 {
            let wisp = self.new_wisp();
            self.wisps.push(wisp);
        }
    }

    /// Ends the run now (the player quit): the game stops as when the light goes out, but with no game-over event.
    /// Returns false, and changes nothing, when no run is going.
    pub fn end_run(&mut self) -> bool {
        if self.state != State::Playing {
            return false;
        }
        self.state = State::Over;
        self.over_time = 0.0;
        self.pvx = 0.0;
        self.pvy = 0.0;
        true
    }

    /// Honours the player's reduced-motion setting: no screen shake, no light flicker, no blinking.
    pub fn set_reduced_motion(&mut self, on: bool) {
        self.reduced_motion = on;
    }

    pub fn pixels(&self) -> &[u8] {
        &self.frame.pixels
    }
    pub fn width(&self) -> usize {
        self.frame.w
    }
    pub fn height(&self) -> usize {
        self.frame.h
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn light(&self) -> f32 {
        self.light
    }
    pub fn night(&self) -> u32 {
        self.night
    }
    pub fn collected(&self) -> u32 {
        self.collected
    }
    pub fn time(&self) -> f32 {
        self.time
    }
    pub fn score(&self) -> u32 {
        self.time as u32 * POINTS_PER_SECOND + self.collected * POINTS_PER_FIREFLY
    }
    pub fn dash_ready(&self) -> f32 {
        1.0 - (self.dash_cooldown / DASH_COOLDOWN).clamp(0.0, 1.0)
    }

    fn new_firefly(&mut self, keep_away: f32) -> Firefly {
        let margin = 10.0;
        let (w, h) = (self.frame.w as f32, self.frame.h as f32);
        let (mut x, mut y) = (0.0, 0.0);
        for _ in 0..20 {
            x = self.rng.range(margin, w - margin);
            y = self.rng.range(margin, h - margin);
            if (x - self.px).hypot(y - self.py) >= keep_away {
                break;
            }
        }
        let phase = self.rng.range(0.0, TAU);
        Firefly { x, y, phase, vx: self.rng.range(-6.0, 6.0), vy: self.rng.range(-6.0, 6.0) }
    }

    /// A wisp comes in from the edge farthest from the player.
    fn new_wisp(&mut self) -> Wisp {
        let (w, h) = (self.frame.w as f32, self.frame.h as f32);
        let mut best = (0.0, 0.0);
        let mut best_distance = -1.0;
        for _ in 0..6 {
            let t = self.rng.float();
            let (x, y) = match self.rng.next_u32() % 4 {
                0 => (t * w, -6.0),
                1 => (w + 6.0, t * h),
                2 => (t * w, h + 6.0),
                _ => (-6.0, t * h),
            };
            let distance = (x - self.px).hypot(y - self.py);
            if distance > best_distance {
                best_distance = distance;
                best = (x, y);
            }
        }
        Wisp { x: best.0, y: best.1, vx: 0.0, vy: 0.0, phase: self.rng.range(0.0, TAU) }
    }

    fn light_radius(&self) -> f32 {
        16.0 + self.light * 0.52
    }

    fn burst(&mut self, x: f32, y: f32, count: usize, color: [f32; 3], speed: f32) {
        for _ in 0..count {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let angle = self.rng.range(0.0, TAU);
            let v = self.rng.range(speed * 0.3, speed);
            let life = self.rng.range(0.35, 0.8);
            self.particles.push(Particle {
                x,
                y,
                vx: angle.cos() * v,
                vy: angle.sin() * v,
                life,
                max_life: life,
                color,
            });
        }
    }

    /// One step: `dt` seconds, steering direction `(ix, iy)`, `dash` pressed this frame. Draws the frame and
    /// returns the events to play sounds for.
    pub fn tick(&mut self, dt: f32, ix: f32, iy: f32, dash: bool) -> Events {
        let dt = dt.clamp(0.0, 0.05);
        self.clock += dt;
        let mut events = Events::default();
        match self.state {
            State::Playing => events |= self.step_play(dt, ix, iy, dash),
            State::Over => self.over_time += dt,
            State::Title => {}
        }
        self.step_ambient(dt);
        self.render();
        events
    }

    fn step_play(&mut self, dt: f32, ix: f32, iy: f32, dash: bool) -> Events {
        let mut events = Events::default();
        self.time += dt;
        let night = 1 + (self.time / NIGHT_SECONDS) as u32;
        if night != self.night {
            self.night = night;
            events |= Event::Night;
        }
        let wanted = (1 + self.night as usize).min(MAX_WISPS);
        if self.wisps.len() < wanted {
            let wisp = self.new_wisp();
            self.wisps.push(wisp);
        }

        events |= self.step_player(dt, ix, iy, dash);
        // Light fades a little faster each night.
        self.light -= (2.2 + self.night as f32 * 0.25) * dt;
        events |= self.step_fireflies();
        events |= self.step_wisps(dt);
        self.shake = (self.shake - dt).max(0.0);

        if self.light <= 0.0 {
            self.light = 0.0;
            self.state = State::Over;
            self.over_time = 0.0;
            events |= Event::GameOver;
            let (px, py) = (self.px, self.py);
            self.burst(px, py, 60, [1.0, 0.6, 0.3], 90.0);
        }
        events
    }

    /// The player accelerates toward the input, with friction; a dash is a burst of speed that costs light.
    fn step_player(&mut self, dt: f32, ix: f32, iy: f32, dash: bool) -> Events {
        let mut events = Events::default();
        let (w, h) = (self.frame.w as f32, self.frame.h as f32);
        let length = ix.hypot(iy);
        let (dx, dy) = if length > 1.0 { (ix / length, iy / length) } else { (ix, iy) };
        let accel = 420.0;
        self.pvx += dx * accel * dt;
        self.pvy += dy * accel * dt;
        let friction = (1.0 - 5.5 * dt).max(0.0);
        self.pvx *= friction;
        self.pvy *= friction;
        self.dash_cooldown = (self.dash_cooldown - dt).max(0.0);
        if dash && self.dash_cooldown <= 0.0 && self.light > DASH_COST {
            // Dash where the player steers, or else where they are already going.
            let (mut ddx, mut ddy) = (dx, dy);
            if ddx.hypot(ddy) < 0.1 {
                let speed = self.pvx.hypot(self.pvy).max(0.001);
                ddx = self.pvx / speed;
                ddy = self.pvy / speed;
            }
            if ddx.hypot(ddy) > 0.1 {
                self.pvx = ddx * 230.0;
                self.pvy = ddy * 230.0;
                self.light -= DASH_COST;
                self.dash_cooldown = DASH_COOLDOWN;
                self.invulnerable = self.invulnerable.max(0.3);
                events |= Event::Dash;
                let (px, py) = (self.px, self.py);
                self.burst(px, py, 10, [1.0, 0.8, 0.5], 40.0);
            }
        }
        self.px = (self.px + self.pvx * dt).clamp(4.0, w - 4.0);
        self.py = (self.py + self.pvy * dt).clamp(4.0, h - 4.0);
        self.invulnerable = (self.invulnerable - dt).max(0.0);
        if self.pvx.hypot(self.pvy) > 25.0 && self.rng.float() < 0.5 {
            let (px, py) = (self.px, self.py);
            self.burst(px, py, 1, [1.0, 0.7, 0.4], 8.0);
        }
        events
    }

    /// Catching a firefly gives light and points; a new one appears away from the player.
    fn step_fireflies(&mut self) -> Events {
        let mut events = Events::default();
        for i in 0..self.fireflies.len() {
            let f = self.fireflies[i];
            if (f.x - self.px).hypot(f.y - self.py) >= PICKUP_DISTANCE {
                continue;
            }
            self.light = (self.light + PICKUP_LIGHT).min(MAX_LIGHT);
            self.collected += 1;
            events |= Event::Pickup;
            self.burst(f.x, f.y, 16, [0.8, 1.0, 0.5], 55.0);
            self.fireflies[i] = self.new_firefly(45.0);
        }
        let wanted = 6 + (self.night as usize / 2).min(4);
        while self.fireflies.len() < wanted {
            let f = self.new_firefly(45.0);
            self.fireflies.push(f);
        }
        events
    }

    /// Wisps hunt the player but fear a bright light: above `FEAR_LIGHT` they back away from its edge.
    fn step_wisps(&mut self, dt: f32) -> Events {
        let speed = (24.0 + self.night as f32 * 4.0).min(62.0);
        let radius = self.light_radius();
        let fear = (self.light > FEAR_LIGHT).then(|| 0.6 + (self.light - FEAR_LIGHT) / (MAX_LIGHT - FEAR_LIGHT));
        let (px, py) = (self.px, self.py);
        let mut can_hit = self.invulnerable <= 0.0;
        let mut hit = None;
        for i in 0..self.wisps.len() {
            let (sx, sy) = separation(&self.wisps, i);
            let wisp = &mut self.wisps[i];
            let (tx, ty) = (px - wisp.x, py - wisp.y);
            let distance = tx.hypot(ty).max(0.001);
            let (mut ax, mut ay) = (tx / distance, ty / distance);
            if let Some(fear) = fear.filter(|_| distance < radius * 0.85) {
                ax = -ax * fear;
                ay = -ay * fear;
            }
            ax += sx;
            ay += sy;
            wisp.phase += dt * 3.0;
            let wobble = wisp.phase.sin() * 0.35;
            let follow = (2.5 * dt).min(1.0);
            wisp.vx += ((ax - ay * wobble) * speed - wisp.vx) * follow;
            wisp.vy += ((ay + ax * wobble) * speed - wisp.vy) * follow;
            wisp.x += wisp.vx * dt;
            wisp.y += wisp.vy * dt;
            if distance < HIT_DISTANCE && can_hit {
                can_hit = false;
                wisp.vx = tx / distance * -60.0;
                wisp.vy = ty / distance * -60.0;
                hit = Some((tx / distance, ty / distance));
            }
        }
        let Some((nx, ny)) = hit else {
            return Events::default();
        };
        self.light -= HIT_LIGHT;
        self.invulnerable = 1.1;
        self.shake = 0.35;
        self.pvx = -nx * 160.0;
        self.pvy = -ny * 160.0;
        self.burst(px, py, 22, [0.6, 0.3, 0.9], 70.0);
        Event::Hit.into()
    }

    fn step_ambient(&mut self, dt: f32) {
        let (w, h) = (self.frame.w as f32, self.frame.h as f32);
        for f in &mut self.fireflies {
            f.phase += dt;
            f.vx += (f.phase * 1.3).sin() * 18.0 * dt;
            f.vy += (f.phase * 1.7 + 1.0).cos() * 18.0 * dt;
            f.vx = f.vx.clamp(-12.0, 12.0);
            f.vy = f.vy.clamp(-12.0, 12.0);
            f.x += f.vx * dt;
            f.y += f.vy * dt;
            if f.x < 8.0 || f.x > w - 8.0 {
                f.vx = -f.vx;
                f.x = f.x.clamp(8.0, w - 8.0);
            }
            if f.y < 8.0 || f.y > h - 8.0 {
                f.vy = -f.vy;
                f.y = f.y.clamp(8.0, h - 8.0);
            }
        }
        for p in &mut self.particles {
            p.life -= dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vx *= 1.0 - 2.5 * dt;
            p.vy *= 1.0 - 2.5 * dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }
}

/// How wisp `i` is pushed away from wisps that crowd it, so they don't stack into one.
fn separation(wisps: &[Wisp], i: usize) -> (f32, f32) {
    let me = wisps[i];
    let (mut x, mut y) = (0.0, 0.0);
    for (j, other) in wisps.iter().enumerate() {
        if i == j {
            continue;
        }
        let (ox, oy) = (me.x - other.x, me.y - other.y);
        let d = ox.hypot(oy);
        if d < 14.0 && d > 0.001 {
            x += ox / d * 0.8;
            y += oy / d * 0.8;
        }
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 1.0 / 60.0;

    fn run(game: &mut Game, seconds: f32, x: f32, y: f32) -> Events {
        let mut events = Events::default();
        let mut t = 0.0;
        while t < seconds {
            events |= game.tick(STEP, x, y, false);
            t += STEP;
        }
        events
    }

    /// A run with nothing in it: no wisps, no fireflies, full light.
    fn empty_run(seed: u32) -> Game {
        let mut game = Game::new(320, 240, seed);
        game.start(seed);
        game.wisps.clear();
        game.fireflies.clear();
        game
    }

    #[test]
    fn rng_is_repeatable() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let f = r.float();
            assert!((0.0..1.0).contains(&f));
        }
    }

    #[test]
    fn resize_clamps_and_fills_the_buffer() {
        let mut game = Game::new(1000, 10, 3);
        assert_eq!((game.width(), game.height()), (480, 120));
        assert_eq!(game.pixels().len(), 480 * 120 * 4);
        assert!(game.pixels().chunks(4).all(|p| p[3] == 255));
        game.resize(320, 240, 3);
        assert_eq!(game.pixels().len(), 320 * 240 * 4);
    }

    #[test]
    fn a_resized_buffer_is_drawn_at_once() {
        let mut game = Game::new(320, 240, 3);
        game.start(3);
        game.resize(200, 150, 3);
        // Nothing ticked since the resize, yet the scene is there (a paused game must not go black).
        assert!(game.pixels().chunks(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0));
        assert!(game.px <= 200.0 && game.py <= 150.0);
    }

    #[test]
    fn title_scene_never_ends() {
        let mut game = Game::new(320, 240, 1);
        let events = run(&mut game, 30.0, 1.0, 0.0);
        assert_eq!(game.state(), State::Title);
        assert_eq!(events, Events::default());
    }

    #[test]
    fn light_fades_until_the_game_ends() {
        let mut game = empty_run(5);
        // With no fireflies to catch the light runs out in well under a minute.
        let mut events = Events::default();
        for _ in 0..(60 * 60) {
            game.fireflies.clear();
            events |= game.tick(STEP, 0.0, 0.0, false);
            if game.state() == State::Over {
                break;
            }
        }
        assert_eq!(game.state(), State::Over);
        assert!(events.has(Event::GameOver));
        assert!(game.time() > 20.0 && game.time() < 50.0, "lasted {}", game.time());
    }

    #[test]
    fn score_is_ten_points_a_second() {
        let mut game = empty_run(6);
        for _ in 0..(60 * 7) {
            game.fireflies.clear();
            game.tick(STEP, 0.0, 0.0, false);
        }
        assert_eq!(game.state(), State::Playing);
        assert_eq!(game.score(), game.time() as u32 * 10);
        assert!(game.score() >= 60, "score {}", game.score());
    }

    #[test]
    fn catching_a_firefly_restores_light_and_scores() {
        let mut game = Game::new(320, 240, 9);
        game.start(9);
        game.wisps.clear();
        game.light = 50.0;
        game.fireflies[0].x = game.px + 3.0;
        game.fireflies[0].y = game.py;
        let events = game.tick(STEP, 0.0, 0.0, false);
        assert!(events.has(Event::Pickup));
        assert_eq!(game.collected(), 1);
        assert!(game.light() > 60.0);
        assert_eq!(game.score(), 50);
    }

    #[test]
    fn a_wisp_hit_costs_light_once_then_protects() {
        let mut game = Game::new(320, 240, 11);
        game.start(11);
        game.invulnerable = 0.0;
        game.light = 40.0;
        game.fireflies.clear();
        game.wisps.truncate(1);
        game.wisps[0].x = game.px + 2.0;
        game.wisps[0].y = game.py;
        let events = game.tick(STEP, 0.0, 0.0, false);
        assert!(events.has(Event::Hit));
        assert!(game.light() < 40.0 - HIT_LIGHT + 1.0);
        game.wisps[0].x = game.px;
        game.wisps[0].y = game.py;
        let events = game.tick(STEP, 0.0, 0.0, false);
        assert!(!events.has(Event::Hit));
    }

    #[test]
    fn two_wisps_at_once_hit_only_once() {
        let mut game = empty_run(12);
        game.invulnerable = 0.0;
        game.light = 80.0;
        for dx in [2.0, -2.0] {
            let wisp = Wisp { x: game.px + dx, y: game.py, vx: 0.0, vy: 0.0, phase: 0.0 };
            game.wisps.push(wisp);
        }
        game.tick(STEP, 0.0, 0.0, false);
        assert!(game.light() > 80.0 - 2.0 * HIT_LIGHT + 1.0);
    }

    /// Distance from the player to the only wisp after `seconds` with `light`, starting 30 px away.
    fn wisp_distance_after(light: f32, seconds: f32) -> f32 {
        let mut game = empty_run(13);
        game.invulnerable = 1000.0;
        game.light = light;
        let wisp = Wisp { x: game.px + 30.0, y: game.py, vx: 0.0, vy: 0.0, phase: 0.0 };
        game.wisps.push(wisp);
        for _ in 0..(seconds / STEP) as usize {
            game.fireflies.clear();
            game.wisps.truncate(1);
            game.light = light;
            game.tick(STEP, 0.0, 0.0, false);
        }
        (game.wisps[0].x - game.px).hypot(game.wisps[0].y - game.py)
    }

    #[test]
    fn wisps_back_away_from_a_bright_light() {
        // At full light the radius is 68 px, so a wisp 30 px away is inside it and retreats.
        assert!(wisp_distance_after(MAX_LIGHT, 1.0) > 35.0);
        // In a dim light the same wisp closes in.
        assert!(wisp_distance_after(FEAR_LIGHT - 20.0, 1.0) < 25.0);
    }

    #[test]
    fn wisps_outside_the_light_still_hunt() {
        let mut game = empty_run(14);
        game.invulnerable = 1000.0;
        let wisp = Wisp { x: game.px + 120.0, y: game.py, vx: 0.0, vy: 0.0, phase: 0.0 };
        game.wisps.push(wisp);
        for _ in 0..30 {
            game.wisps.truncate(1);
            game.light = MAX_LIGHT;
            game.tick(STEP, 0.0, 0.0, false);
        }
        assert!(game.wisps[0].x - game.px < 120.0);
    }

    #[test]
    fn dash_costs_light_and_has_a_cooldown() {
        let mut game = Game::new(320, 240, 2);
        game.start(2);
        game.wisps.clear();
        let before = game.light();
        let events = game.tick(STEP, 1.0, 0.0, true);
        assert!(events.has(Event::Dash));
        assert!(game.light() < before - DASH_COST + 0.5);
        assert!(!game.tick(STEP, 1.0, 0.0, true).has(Event::Dash));
        assert!(game.dash_ready() < 0.1);
    }

    #[test]
    fn nights_bring_more_wisps() {
        let mut game = Game::new(320, 240, 4);
        game.start(4);
        // Night n starts after (n - 1) × NIGHT_SECONDS.
        game.time = NIGHT_SECONDS * 2.0 - 0.01;
        game.light = MAX_LIGHT;
        let events = game.tick(STEP, 0.0, 0.0, false);
        assert!(events.has(Event::Night));
        assert_eq!(game.night(), 3);
        assert!(game.wisps.len() >= 3);
    }

    #[test]
    fn the_player_stays_on_screen() {
        let mut game = empty_run(8);
        run(&mut game, 5.0, 1.0, 1.0);
        assert!(game.px <= 316.0 && game.py <= 236.0);
    }

    #[test]
    fn a_new_run_after_game_over_starts_fresh() {
        let mut game = Game::new(320, 240, 15);
        game.start(15);
        game.collected = 7;
        game.time = 60.0;
        game.light = 0.5;
        game.wisps.clear();
        run(&mut game, 1.0, 0.0, 0.0);
        assert_eq!(game.state(), State::Over);
        game.start(16);
        assert_eq!(game.state(), State::Playing);
        assert_eq!((game.score(), game.collected(), game.night()), (0, 0, 1));
        assert_eq!(game.light(), MAX_LIGHT);
        assert_eq!(game.wisps.len(), 2);
        assert!(game.particles.is_empty());
    }

    #[test]
    fn ending_a_run_stops_the_game() {
        let mut game = Game::new(320, 240, 17);
        game.start(17);
        run(&mut game, 2.0, 1.0, 0.0);
        let (score, light, x, y) = (game.score(), game.light(), game.px, game.py);
        assert!(game.end_run());
        assert_eq!(game.state(), State::Over);
        // Nothing moves, scores or makes a sound after the player quit, whatever they press.
        game.wisps.iter_mut().for_each(|wisp| (wisp.x, wisp.y) = (x, y));
        let mut events = Events::default();
        for _ in 0..(60 * 5) {
            events |= game.tick(STEP, -1.0, 1.0, true);
        }
        assert_eq!(events, Events::default());
        assert_eq!((game.score(), game.light(), game.px, game.py), (score, light, x, y));
        assert_eq!(game.state(), State::Over);
    }

    #[test]
    fn ending_a_run_outside_a_run_does_nothing() {
        let mut game = Game::new(320, 240, 18);
        assert!(!game.end_run());
        assert_eq!(game.state(), State::Title);
        game.start(18);
        assert!(game.end_run());
        assert!(!game.end_run());
        game.start(19);
        assert_eq!(game.state(), State::Playing);
    }

    /// A run just hit by a wisp: shaking and blinking.
    fn just_hit(reduced_motion: bool) -> Game {
        let mut game = empty_run(20);
        game.set_reduced_motion(reduced_motion);
        game.invulnerable = 0.0;
        game.wisps.push(Wisp { x: game.px + 2.0, y: game.py, vx: 0.0, vy: 0.0, phase: 0.0 });
        assert!(game.tick(STEP, 0.0, 0.0, false).has(Event::Hit));
        game
    }

    #[test]
    fn a_hit_shakes_and_blinks() {
        let mut game = just_hit(false);
        let mut shaken = false;
        let mut alphas = Vec::new();
        for _ in 0..20 {
            game.tick(STEP, 0.0, 0.0, false);
            shaken |= game.camera_offset() != (0.0, 0.0);
            alphas.push(game.player_alpha());
        }
        assert!(shaken);
        assert!(alphas.contains(&0.0) && alphas.contains(&1.0));
    }

    #[test]
    fn reduced_motion_means_no_shake_and_no_blinking() {
        let mut game = just_hit(true);
        for _ in 0..20 {
            game.tick(STEP, 0.0, 0.0, false);
            assert_eq!(game.camera_offset(), (0.0, 0.0));
            assert_eq!(game.player_alpha(), 0.5);
        }
    }

    /// Whether a frame changes when only the flicker clocks move on.
    fn flickers(reduced_motion: bool) -> bool {
        let mut game = empty_run(21);
        game.set_reduced_motion(reduced_motion);
        game.tick(STEP, 0.0, 0.0, false);
        game.invulnerable = 0.0;
        game.particles.clear();
        game.render();
        let before = game.pixels().to_vec();
        game.clock += 0.37;
        game.fireflies.iter_mut().for_each(|f| f.phase += 0.37);
        game.render();
        game.pixels() != &before[..]
    }

    #[test]
    fn reduced_motion_holds_the_lights_still() {
        assert!(flickers(false));
        assert!(!flickers(true));
    }

    #[test]
    fn state_and_event_numbers_match_the_page() {
        // web/app.js mirrors these numbers; see the table in lib.rs.
        assert_eq!([State::Title as u32, State::Playing as u32, State::Over as u32], [0, 1, 2]);
        let events = [Event::Pickup, Event::Hit, Event::Dash, Event::Night, Event::GameOver];
        assert_eq!(events.map(|e| e as u32), [1, 2, 4, 8, 16]);
    }
}
