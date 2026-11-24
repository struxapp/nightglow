//! Simulation and software rendering. Units are buffer pixels and seconds.

pub const EVENT_PICKUP: u32 = 1;
pub const EVENT_HIT: u32 = 2;
pub const EVENT_DASH: u32 = 4;
pub const EVENT_NIGHT: u32 = 8;
pub const EVENT_GAME_OVER: u32 = 16;

const TITLE: u32 = 0;
const PLAYING: u32 = 1;
const OVER: u32 = 2;

const MAX_LIGHT: f32 = 100.0;
const PICKUP_LIGHT: f32 = 14.0;
const HIT_LIGHT: f32 = 22.0;
const DASH_COST: f32 = 6.0;
const DASH_COOLDOWN: f32 = 1.1;
const NIGHT_SECONDS: f32 = 25.0;
const FEAR_LIGHT: f32 = 60.0;
const MAX_WISPS: usize = 12;
const MAX_PARTICLES: usize = 400;

/// xorshift32: small, fast and the same everywhere.
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(if seed == 0 { 0x9e37_79b9 } else { seed })
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0.0..1.0
    pub fn float(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, low: f32, high: f32) -> f32 {
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
    w: usize,
    h: usize,
    pixels: Vec<u8>,
    ground: Vec<[f32; 3]>,
    vignette: Vec<f32>,
    lightmap: Vec<[f32; 3]>,
    rng: Rng,
    state: u32,
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
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}

fn smooth_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

impl Game {
    pub fn new(w: usize, h: usize, seed: u32) -> Self {
        let mut game = Game {
            w: 0,
            h: 0,
            pixels: Vec::new(),
            ground: Vec::new(),
            vignette: Vec::new(),
            lightmap: Vec::new(),
            rng: Rng::new(seed),
            state: TITLE,
            px: 0.0,
            py: 0.0,
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
        };
        game.resize(w, h, seed);
        game
    }

    pub fn resize(&mut self, w: usize, h: usize, seed: u32) {
        let w = w.clamp(160, 480);
        let h = h.clamp(120, 480);
        let (old_w, old_h) = (self.w.max(1) as f32, self.h.max(1) as f32);
        self.w = w;
        self.h = h;
        self.pixels = vec![0; w * h * 4];
        self.lightmap = vec![[0.0; 3]; w * h];
        self.ground = Vec::with_capacity(w * h);
        self.vignette = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32, y as f32);
                let n = smooth_noise(fx / 22.0, fy / 22.0, seed) * 0.6
                    + smooth_noise(fx / 7.0, fy / 7.0, seed ^ 0x55) * 0.3
                    + smooth_noise(fx / 2.5, fy / 2.5, seed ^ 0xaa) * 0.1;
                let moss = smooth_noise(fx / 40.0 + 9.0, fy / 40.0, seed ^ 0x1234);
                let blade = if hash(x as i32, y as i32, seed ^ 0x77) > 0.93 { 0.25 } else { 0.0 };
                let stone = smooth_noise(fx / 11.0 + 50.0, fy / 11.0, seed ^ 0x99) > 0.78;
                let base = if stone {
                    [0.42 + n * 0.1, 0.42 + n * 0.1, 0.46 + n * 0.1]
                } else {
                    [0.16 + n * 0.16 + moss * 0.06, 0.34 + n * 0.28 + blade, 0.20 + n * 0.12 - moss * 0.04]
                };
                self.ground.push(base);
                let dx = (fx / w as f32 - 0.5) * 2.0;
                let dy = (fy / h as f32 - 0.5) * 2.0;
                self.vignette.push((1.0 - (dx * dx + dy * dy) * 0.28).clamp(0.45, 1.0));
            }
        }
        // Keep everything inside the new size.
        let (sx, sy) = (w as f32 / old_w, h as f32 / old_h);
        if self.fireflies.is_empty() {
            self.rng = Rng::new(seed);
            self.px = w as f32 / 2.0;
            self.py = h as f32 / 2.0;
            for _ in 0..8 {
                let f = self.new_firefly(0.0);
                self.fireflies.push(f);
            }
        } else {
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
        }
    }

    pub fn start(&mut self, seed: u32) {
        self.rng = Rng::new(seed);
        self.state = PLAYING;
        self.px = self.w as f32 / 2.0;
        self.py = self.h as f32 / 2.0;
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

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn width(&self) -> usize {
        self.w
    }
    pub fn height(&self) -> usize {
        self.h
    }
    pub fn state(&self) -> u32 {
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
        self.time as u32 * 10 + self.collected * 50
    }
    pub fn dash_ready(&self) -> f32 {
        1.0 - (self.dash_cooldown / DASH_COOLDOWN).clamp(0.0, 1.0)
    }

    fn new_firefly(&mut self, keep_away: f32) -> Firefly {
        let margin = 10.0;
        let (mut x, mut y) = (0.0, 0.0);
        for _ in 0..20 {
            x = self.rng.range(margin, self.w as f32 - margin);
            y = self.rng.range(margin, self.h as f32 - margin);
            if (x - self.px).hypot(y - self.py) >= keep_away {
                break;
            }
        }
        let phase = self.rng.range(0.0, 6.283);
        Firefly { x, y, phase, vx: self.rng.range(-6.0, 6.0), vy: self.rng.range(-6.0, 6.0) }
    }

    /// A wisp comes in from the edge farthest from the player.
    fn new_wisp(&mut self) -> Wisp {
        let (w, h) = (self.w as f32, self.h as f32);
        let mut best = (0.0, 0.0);
        let mut best_distance = -1.0;
        for _ in 0..6 {
            let t = self.rng.float();
            let (x, y) = match self.rng.next() % 4 {
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
        Wisp { x: best.0, y: best.1, vx: 0.0, vy: 0.0, phase: self.rng.range(0.0, 6.283) }
    }

    fn light_radius(&self) -> f32 {
        16.0 + self.light * 0.52
    }

    fn burst(&mut self, x: f32, y: f32, count: usize, color: [f32; 3], speed: f32) {
        for _ in 0..count {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let angle = self.rng.range(0.0, 6.283);
            let v = self.rng.range(speed * 0.3, speed);
            let life = self.rng.range(0.35, 0.8);
            self.particles.push(Particle { x, y, vx: angle.cos() * v, vy: angle.sin() * v, life, max_life: life, color });
        }
    }

    /// One step: `dt` seconds, steering direction `(ix, iy)`, `dash` pressed this frame. Returns sound events.
    pub fn tick(&mut self, dt: f32, ix: f32, iy: f32, dash: bool) -> u32 {
        let dt = dt.clamp(0.0, 0.05);
        self.clock += dt;
        let mut events = 0;
        if self.state == PLAYING {
            events |= self.step_play(dt, ix, iy, dash);
        } else if self.state == OVER {
            self.over_time += dt;
        }
        self.step_ambient(dt);
        self.render();
        events
    }

    fn step_play(&mut self, dt: f32, ix: f32, iy: f32, dash: bool) -> u32 {
        let mut events = 0;
        let (w, h) = (self.w as f32, self.h as f32);
        self.time += dt;
        let night = 1 + (self.time / NIGHT_SECONDS) as u32;
        if night != self.night {
            self.night = night;
            events |= EVENT_NIGHT;
        }
        let wanted = (1 + self.night as usize).min(MAX_WISPS);
        if self.wisps.len() < wanted {
            let wisp = self.new_wisp();
            self.wisps.push(wisp);
        }

        // Player: accelerate toward the input, with friction; dash is a burst of speed that costs light.
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
                events |= EVENT_DASH;
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

        // Light fades a little faster each night.
        self.light -= (2.2 + self.night as f32 * 0.25) * dt;

        // Fireflies.
        let mut caught = Vec::new();
        for (i, f) in self.fireflies.iter().enumerate() {
            if (f.x - self.px).hypot(f.y - self.py) < 7.5 {
                caught.push(i);
            }
        }
        for &i in caught.iter().rev() {
            let f = self.fireflies[i];
            self.light = (self.light + PICKUP_LIGHT).min(MAX_LIGHT);
            self.collected += 1;
            events |= EVENT_PICKUP;
            self.burst(f.x, f.y, 16, [0.8, 1.0, 0.5], 55.0);
            let replacement = self.new_firefly(45.0);
            self.fireflies[i] = replacement;
        }
        let wanted_fireflies = 6 + (self.night as usize / 2).min(4);
        while self.fireflies.len() < wanted_fireflies {
            let f = self.new_firefly(45.0);
            self.fireflies.push(f);
        }

        // Wisps hunt the player but fear a bright light: above FEAR_LIGHT they stop at its edge.
        let speed = (24.0 + self.night as f32 * 4.0).min(62.0);
        let radius = self.light_radius();
        let count = self.wisps.len();
        for i in 0..count {
            let mut wisp = self.wisps[i];
            let (tx, ty) = (self.px - wisp.x, self.py - wisp.y);
            let distance = tx.hypot(ty).max(0.001);
            let (mut ax, mut ay) = (tx / distance, ty / distance);
            if self.light > FEAR_LIGHT && distance < radius * 0.85 {
                let fear = (self.light - FEAR_LIGHT) / (MAX_LIGHT - FEAR_LIGHT);
                ax = -ax * (0.6 + fear);
                ay = -ay * (0.6 + fear);
            }
            for j in 0..count {
                if i != j {
                    let other = self.wisps[j];
                    let (ox, oy) = (wisp.x - other.x, wisp.y - other.y);
                    let d = ox.hypot(oy);
                    if d < 14.0 && d > 0.001 {
                        ax += ox / d * 0.8;
                        ay += oy / d * 0.8;
                    }
                }
            }
            wisp.phase += dt * 3.0;
            let wobble = wisp.phase.sin() * 0.35;
            wisp.vx += ((ax - ay * wobble) * speed - wisp.vx) * (2.5 * dt).min(1.0);
            wisp.vy += ((ay + ax * wobble) * speed - wisp.vy) * (2.5 * dt).min(1.0);
            wisp.x += wisp.vx * dt;
            wisp.y += wisp.vy * dt;
            if distance < 8.0 && self.invulnerable <= 0.0 {
                self.light -= HIT_LIGHT;
                self.invulnerable = 1.1;
                self.shake = 0.35;
                self.pvx = -tx / distance * 160.0;
                self.pvy = -ty / distance * 160.0;
                wisp.vx = tx / distance * -60.0;
                wisp.vy = ty / distance * -60.0;
                events |= EVENT_HIT;
                let (px, py) = (self.px, self.py);
                self.burst(px, py, 22, [0.6, 0.3, 0.9], 70.0);
            }
            self.wisps[i] = wisp;
        }
        self.shake = (self.shake - dt).max(0.0);

        if self.light <= 0.0 {
            self.light = 0.0;
            self.state = OVER;
            self.over_time = 0.0;
            events |= EVENT_GAME_OVER;
            let (px, py) = (self.px, self.py);
            self.burst(px, py, 60, [1.0, 0.6, 0.3], 90.0);
        }
        events
    }

    fn step_ambient(&mut self, dt: f32) {
        let (w, h) = (self.w as f32, self.h as f32);
        let clock = self.clock;
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
        let _ = clock;
        for p in &mut self.particles {
            p.life -= dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vx *= 1.0 - 2.5 * dt;
            p.vy *= 1.0 - 2.5 * dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    fn add_light(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 3], intensity: f32) {
        if radius <= 0.5 || intensity <= 0.0 {
            return;
        }
        let x0 = ((cx - radius).floor().max(0.0)) as usize;
        let y0 = ((cy - radius).floor().max(0.0)) as usize;
        let x1 = ((cx + radius).ceil().min(self.w as f32 - 1.0)).max(0.0) as usize;
        let y1 = ((cy + radius).ceil().min(self.h as f32 - 1.0)).max(0.0) as usize;
        let inv = 1.0 / (radius * radius);
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            let row = y * self.w;
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let t = 1.0 - (dx * dx + dy * dy) * inv;
                if t > 0.0 {
                    let f = t * t * intensity;
                    let cell = &mut self.lightmap[row + x];
                    cell[0] += color[0] * f;
                    cell[1] += color[1] * f;
                    cell[2] += color[2] * f;
                }
            }
        }
    }

    fn render(&mut self) {
        let (w, h) = (self.w, self.h);
        let t = self.clock;
        let (ox, oy) = if self.shake > 0.0 {
            ((t * 91.0).sin() * self.shake * 6.0, (t * 77.0).cos() * self.shake * 6.0)
        } else {
            (0.0, 0.0)
        };
        let ambient = if self.state == TITLE { [0.09, 0.10, 0.17] } else { [0.05, 0.06, 0.11] };
        for cell in self.lightmap.iter_mut() {
            *cell = ambient;
        }
        let fireflies = self.fireflies.clone();
        for f in &fireflies {
            let flicker = 0.75 + 0.25 * (f.phase * 7.0).sin();
            self.add_light(f.x + ox, f.y + oy, 15.0, [0.75, 1.0, 0.45], 0.75 * flicker);
        }
        if self.state != TITLE {
            let fade = if self.state == OVER { (1.0 - self.over_time * 0.8).max(0.0) } else { 1.0 };
            let flicker = 0.92 + 0.08 * (t * 13.0).sin() * (t * 7.3).cos();
            let radius = self.light_radius() * fade.max(0.25);
            self.add_light(self.px + ox, self.py + oy, radius, [1.0, 0.82, 0.55], 1.25 * flicker * fade.max(0.1));
        }
        let particles = self.particles.clone();
        for p in &particles {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            self.add_light(p.x + ox, p.y + oy, 4.0, p.color, 0.5 * k);
        }

        // Ground × light, then the vignette.
        for i in 0..w * h {
            let g = self.ground[i];
            let l = self.lightmap[i];
            let v = self.vignette[i];
            let mut c = [g[0] * l[0] * 1.7, g[1] * l[1] * 1.7, g[2] * l[2] * 1.7];
            if self.state == OVER {
                let grey = (c[0] + c[1] + c[2]) / 3.0;
                let k = (self.over_time * 0.6).min(0.7);
                c = [c[0] + (grey - c[0]) * k, c[1] + (grey - c[1]) * k, c[2] + (grey - c[2]) * k];
            }
            let o = i * 4;
            self.pixels[o] = (c[0] * v * 255.0).min(255.0) as u8;
            self.pixels[o + 1] = (c[1] * v * 255.0).min(255.0) as u8;
            self.pixels[o + 2] = (c[2] * v * 255.0).min(255.0) as u8;
            self.pixels[o + 3] = 255;
        }

        // Wisps: dark, wavy blobs with two glowing eyes, always visible so they are never unfair.
        let wisps = self.wisps.clone();
        for wisp in &wisps {
            let (cx, cy) = (wisp.x + ox, wisp.y + oy);
            for y in -9i32..=9 {
                for x in -9i32..=9 {
                    let angle = (y as f32).atan2(x as f32);
                    let edge = 6.5 + (angle * 3.0 + wisp.phase * 2.0).sin() * 1.3;
                    let d = ((x * x + y * y) as f32).sqrt();
                    if d < edge {
                        let k = (1.0 - d / edge).min(1.0) * 0.85 + 0.1;
                        self.darken(cx as i32 + x, cy as i32 + y, k);
                    }
                }
            }
            let look = (self.px - wisp.x).signum();
            let eye = [1.0, 0.36, 0.3];
            self.plot(cx as i32 - 2 + look as i32, cy as i32 - 1, eye, 0.95);
            self.plot(cx as i32 + 2 + look as i32, cy as i32 - 1, eye, 0.95);
        }

        for f in &fireflies {
            let flicker = 0.7 + 0.3 * (f.phase * 7.0).sin();
            self.plot((f.x + ox) as i32, (f.y + oy) as i32, [0.9, 1.0, 0.6], flicker);
            self.plot((f.x + ox) as i32 + 1, (f.y + oy) as i32, [0.7, 0.9, 0.4], flicker * 0.5);
        }
        for p in &particles {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            self.plot((p.x + ox) as i32, (p.y + oy) as i32, p.color, k);
        }
        if self.state == PLAYING || (self.state == OVER && self.over_time < 0.6) {
            let blink = self.invulnerable > 0.0 && (t * 20.0).sin() < 0.0;
            if !blink {
                let (cx, cy) = ((self.px + ox) as i32, (self.py + oy) as i32);
                for y in -2i32..=2 {
                    for x in -2i32..=2 {
                        let d = x * x + y * y;
                        if d <= 5 {
                            let core = if d <= 1 { [1.0, 1.0, 0.92] } else { [1.0, 0.85, 0.55] };
                            self.plot(cx + x, cy + y, core, 1.0);
                        }
                    }
                }
            }
        }
    }

    fn plot(&mut self, x: i32, y: i32, color: [f32; 3], alpha: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let o = (y as usize * self.w + x as usize) * 4;
        for k in 0..3 {
            let current = self.pixels[o + k] as f32;
            let target = color[k] * 255.0;
            self.pixels[o + k] = (current + (target - current) * alpha.clamp(0.0, 1.0)) as u8;
        }
    }

    fn darken(&mut self, x: i32, y: i32, amount: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let o = (y as usize * self.w + x as usize) * 4;
        let keep = 1.0 - amount.clamp(0.0, 1.0);
        self.pixels[o] = (self.pixels[o] as f32 * keep + 6.0 * amount) as u8;
        self.pixels[o + 1] = (self.pixels[o + 1] as f32 * keep + 4.0 * amount) as u8;
        self.pixels[o + 2] = (self.pixels[o + 2] as f32 * keep + 12.0 * amount) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(game: &mut Game, seconds: f32, x: f32, y: f32) -> u32 {
        let mut events = 0;
        let mut t = 0.0;
        while t < seconds {
            events |= game.tick(1.0 / 60.0, x, y, false);
            t += 1.0 / 60.0;
        }
        events
    }

    #[test]
    fn rng_is_repeatable() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..100 {
            assert_eq!(a.next(), b.next());
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
        game.tick(0.016, 0.0, 0.0, false);
        assert_eq!(game.pixels().len(), 480 * 120 * 4);
        assert!(game.pixels().chunks(4).all(|p| p[3] == 255));
        game.resize(320, 240, 3);
        game.tick(0.016, 0.0, 0.0, false);
        assert_eq!(game.pixels().len(), 320 * 240 * 4);
    }

    #[test]
    fn title_scene_never_ends() {
        let mut game = Game::new(320, 240, 1);
        let events = run(&mut game, 30.0, 1.0, 0.0);
        assert_eq!(game.state(), TITLE);
        assert_eq!(events, 0);
    }

    #[test]
    fn light_fades_until_the_game_ends() {
        let mut game = Game::new(320, 240, 5);
        game.start(5);
        game.wisps.clear();
        game.fireflies.clear();
        // With no fireflies to catch the light runs out in well under a minute.
        let mut events = 0;
        for _ in 0..(60 * 60) {
            game.fireflies.clear();
            events |= game.tick(1.0 / 60.0, 0.0, 0.0, false);
            if game.state() == OVER {
                break;
            }
        }
        assert_eq!(game.state(), OVER);
        assert!(events & EVENT_GAME_OVER != 0);
        assert!(game.time() > 20.0 && game.time() < 50.0, "lasted {}", game.time());
    }

    #[test]
    fn catching_a_firefly_restores_light_and_scores() {
        let mut game = Game::new(320, 240, 9);
        game.start(9);
        game.wisps.clear();
        game.light = 50.0;
        game.fireflies[0].x = game.px + 3.0;
        game.fireflies[0].y = game.py;
        let events = game.tick(1.0 / 60.0, 0.0, 0.0, false);
        assert!(events & EVENT_PICKUP != 0);
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
        let events = game.tick(1.0 / 60.0, 0.0, 0.0, false);
        assert!(events & EVENT_HIT != 0);
        let after = game.light();
        assert!(after < 40.0 - HIT_LIGHT + 1.0);
        game.wisps[0].x = game.px;
        game.wisps[0].y = game.py;
        let events = game.tick(1.0 / 60.0, 0.0, 0.0, false);
        assert_eq!(events & EVENT_HIT, 0);
    }

    #[test]
    fn dash_costs_light_and_has_a_cooldown() {
        let mut game = Game::new(320, 240, 2);
        game.start(2);
        game.wisps.clear();
        let before = game.light();
        let events = game.tick(1.0 / 60.0, 1.0, 0.0, true);
        assert!(events & EVENT_DASH != 0);
        assert!(game.light() < before - DASH_COST + 0.5);
        assert_eq!(game.tick(1.0 / 60.0, 1.0, 0.0, true) & EVENT_DASH, 0);
        assert!(game.dash_ready() < 0.1);
    }

    #[test]
    fn nights_bring_more_wisps() {
        let mut game = Game::new(320, 240, 4);
        game.start(4);
        // Night n starts after (n - 1) × NIGHT_SECONDS.
        game.time = NIGHT_SECONDS * 2.0 - 0.01;
        game.light = MAX_LIGHT;
        let events = game.tick(1.0 / 60.0, 0.0, 0.0, false);
        assert!(events & EVENT_NIGHT != 0);
        assert_eq!(game.night(), 3);
        assert!(game.wisps.len() >= 3);
    }

    #[test]
    fn the_player_stays_on_screen() {
        let mut game = Game::new(320, 240, 8);
        game.start(8);
        game.wisps.clear();
        run(&mut game, 5.0, 1.0, 1.0);
        assert!(game.px <= 316.0 && game.py <= 236.0);
    }
}
