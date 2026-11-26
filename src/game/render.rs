//! Software rendering. The meadow is lit pixel by pixel: every light adds to a light map, and each pixel is
//! ground × light plus a little of the light itself (so a light shows its own colour on any ground), then
//! tone-mapped. The wisps, fireflies, sparks and the player are drawn on top.

use super::{Game, State};

/// The player's light: warm gold, like the start screen and the icon.
const GOLD: [f32; 3] = [1.0, 0.72, 0.36];
const FIREFLY: [f32; 3] = [0.98, 1.0, 0.55];
const WISP_EYE: [f32; 3] = [1.0, 0.30, 0.24];
const WISP_RIM: [f32; 3] = [0.58, 0.34, 0.95];
/// Moonlight: what the meadow gets with no light nearby.
const AMBIENT_PLAY: [f32; 3] = [0.085, 0.11, 0.19];
const AMBIENT_TITLE: [f32; 3] = [0.12, 0.15, 0.25];
/// How much of a light shows by itself, on top of what the ground reflects.
const GLOW: f32 = 0.2;

/// The pixel buffer and what is baked for its size: the ground colours and the vignette.
pub(super) struct Frame {
    pub w: usize,
    pub h: usize,
    seed: u32,
    /// RGBA, `w × h`.
    pub pixels: Vec<u8>,
    ground: Vec<[f32; 3]>,
    vignette: Vec<f32>,
    light: Vec<[f32; 3]>,
}

fn clamp_size(w: usize, h: usize) -> (usize, usize) {
    (w.clamp(160, 480), h.clamp(120, 480))
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
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

/// Soft tone-map: lifts the dark meadow a little and rolls bright light off instead of clipping it.
fn tone(x: f32) -> f32 {
    1.3 * x / (x + 0.55)
}

const FLOWERS: [[f32; 3]; 3] = [[0.98, 0.90, 0.58], [0.90, 0.90, 0.96], [0.78, 0.62, 0.98]];
/// Flowers sit one at most in each cell of this many pixels.
const FLOWER_CELL: i32 = 9;

/// Pebbles sit one at most in each cell of this many pixels.
const PEBBLE_CELL: i32 = 15;

/// The shade of a pebble at this pixel, if there is one: a small ellipse lit from the top-left, with a dark rim.
fn pebble_at(x: i32, y: i32, seed: u32) -> Option<f32> {
    let (cx, cy) = (x.div_euclid(PEBBLE_CELL), y.div_euclid(PEBBLE_CELL));
    if hash(cx, cy, seed ^ 0xb1) < 0.72 {
        return None;
    }
    let rx = 1.6 + hash(cx, cy, seed ^ 0xb2) * 2.6;
    let ry = rx * (0.55 + hash(cx, cy, seed ^ 0xb3) * 0.35);
    let room = PEBBLE_CELL as f32 - 2.0 * (rx + 1.0);
    let px = (cx * PEBBLE_CELL) as f32 + rx + 1.0 + hash(cx, cy, seed ^ 0xb4) * room;
    let py = (cy * PEBBLE_CELL) as f32 + rx + 1.0 + hash(cx, cy, seed ^ 0xb5) * room;
    let (dx, dy) = ((x as f32 + 0.5 - px) / rx, (y as f32 + 0.5 - py) / ry);
    let d = dx * dx + dy * dy;
    if d > 1.0 {
        return None;
    }
    // Facing the top-left is lighter; the outer ring is the rim.
    let facing = (-(dx + dy) * 0.35 + 1.0).clamp(0.65, 1.35);
    Some(if d > 0.7 { facing * 0.6 } else { facing })
}

/// The meadow's colour (how much light it reflects) at one pixel.
fn ground_at(x: i32, y: i32, seed: u32) -> [f32; 3] {
    let (fx, fy) = (x as f32, y as f32);
    let n = smooth_noise(fx / 26.0, fy / 26.0, seed) * 0.7 + smooth_noise(fx / 8.0, fy / 8.0, seed ^ 0x55) * 0.3;
    if let Some(stone) = pebble_at(x, y, seed) {
        let k = stone * (0.36 + n * 0.08);
        return [k * 1.04, k, k * 0.92];
    }
    // Grass: clumps of lighter and darker tufts, a fine grain that only just shows, and flowers here and there.
    let tuft = smooth_noise(fx / 3.2, fy / 3.2, seed ^ 0x77);
    let tuft = if tuft > 0.68 {
        0.07
    } else if tuft < 0.25 {
        -0.05
    } else {
        0.0
    };
    let grain = (hash(x, y, seed ^ 0x5a) - 0.5) * 0.035;
    let dry = smooth_noise(fx / 45.0 + 9.0, fy / 45.0, seed ^ 0x1234);
    let base = 0.26 + n * 0.12 + tuft + grain;
    let mut grass = [base * (0.92 + dry * 0.2), base * 1.06, base * (0.74 - dry * 0.1)];
    let (cx, cy) = (x.div_euclid(FLOWER_CELL), y.div_euclid(FLOWER_CELL));
    if hash(cx, cy, seed ^ 0xf1) > 0.9 {
        let pick = hash(cx, cy, seed ^ 0xf2);
        let fx0 = cx * FLOWER_CELL + 2 + (pick * 5.0) as i32;
        let fy0 = cy * FLOWER_CELL + 2 + (hash(cx, cy, seed ^ 0xf3) * 5.0) as i32;
        let (dx, dy) = ((x - fx0).abs(), (y - fy0).abs());
        let petals = FLOWERS[(pick * 3.0) as usize % 3];
        if dx == 0 && dy == 0 {
            grass = [0.95, 0.8, 0.35];
        } else if dx + dy == 1 {
            grass = petals.map(|c| c * 0.6);
        }
    }
    grass
}

impl Frame {
    /// A buffer of `w × h` (clamped to 160..=480 × 120..=480) with the meadow for `seed`.
    pub fn new(w: usize, h: usize, seed: u32) -> Self {
        let (w, h) = clamp_size(w, h);
        let mut ground = Vec::with_capacity(w * h);
        let mut vignette = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                ground.push(ground_at(x as i32, y as i32, seed));
                let dx = (x as f32 / w as f32 - 0.5) * 2.0;
                let dy = (y as f32 / h as f32 - 0.5) * 2.0;
                vignette.push((1.0 - (dx * dx + dy * dy) * 0.22).clamp(0.55, 1.0));
            }
        }
        Frame { w, h, seed, pixels: vec![0; w * h * 4], ground, vignette, light: vec![[0.0; 3]; w * h] }
    }

    /// False when a buffer for these arguments would be the same as this one.
    pub fn needs_rebuild(&self, w: usize, h: usize, seed: u32) -> bool {
        clamp_size(w, h) != (self.w, self.h) || seed != self.seed
    }

    fn clear_light(&mut self, ambient: [f32; 3]) {
        self.light.fill(ambient);
    }

    /// Adds a round light that falls off with the square of the distance.
    fn add_light(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 3], intensity: f32) {
        if radius <= 0.5 || intensity <= 0.0 {
            return;
        }
        let x0 = (cx - radius).floor().max(0.0) as usize;
        let y0 = (cy - radius).floor().max(0.0) as usize;
        let x1 = (cx + radius).ceil().min(self.w as f32 - 1.0).max(0.0) as usize;
        let y1 = (cy + radius).ceil().min(self.h as f32 - 1.0).max(0.0) as usize;
        let inv = 1.0 / (radius * radius);
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            let row = y * self.w;
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let t = 1.0 - (dx * dx + dy * dy) * inv;
                if t > 0.0 {
                    let f = t * t * intensity;
                    let cell = &mut self.light[row + x];
                    cell[0] += color[0] * f;
                    cell[1] += color[1] * f;
                    cell[2] += color[2] * f;
                }
            }
        }
    }

    /// Ground × light plus the glow of the lights above `ambient`, greyed by `grey` (0..1), tone-mapped, then the
    /// vignette.
    fn compose(&mut self, ambient: [f32; 3], grey: f32) {
        let cells = self.ground.iter().zip(&self.light).zip(&self.vignette);
        for (pixel, ((g, l), v)) in self.pixels.as_chunks_mut::<4>().0.iter_mut().zip(cells) {
            let mut c = [0.0; 3];
            for k in 0..3 {
                c[k] = g[k] * l[k] * 1.9 + (l[k] - ambient[k]).max(0.0) * GLOW;
            }
            if grey > 0.0 {
                let mean = (c[0] + c[1] + c[2]) / 3.0;
                c = c.map(|channel| channel + (mean - channel) * grey);
            }
            // Tone-map half by brightness and half by channel: a bright gold stays gold instead of going cream,
            // without over-saturating the darker colours.
            let lum = (c[0] * 0.3 + c[1] * 0.55 + c[2] * 0.15).max(1e-4);
            let scale = tone(lum) / lum * v * 255.0;
            for k in 0..3 {
                pixel[k] = ((c[k] * scale + tone(c[k]) * v * 255.0) * 0.5).min(255.0) as u8;
            }
            pixel[3] = 255;
        }
    }

    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return None;
        }
        Some((y as usize * self.w + x as usize) * 4)
    }

    /// Blends `color` over one pixel.
    fn plot(&mut self, x: i32, y: i32, color: [f32; 3], alpha: f32) {
        let Some(o) = self.offset(x, y) else { return };
        let alpha = alpha.clamp(0.0, 1.0);
        for (k, channel) in color.iter().enumerate() {
            let current = self.pixels[o + k] as f32;
            self.pixels[o + k] = (current + (channel * 255.0 - current) * alpha) as u8;
        }
    }

    /// Adds `color × amount` to one pixel: a glow.
    fn glow(&mut self, x: i32, y: i32, color: [f32; 3], amount: f32) {
        let Some(o) = self.offset(x, y) else { return };
        for (k, channel) in color.iter().enumerate() {
            self.pixels[o + k] = (self.pixels[o + k] as f32 + channel * amount.max(0.0) * 255.0).min(255.0) as u8;
        }
    }

    /// Pulls one pixel toward the wisps' near-black violet.
    fn darken(&mut self, x: i32, y: i32, amount: f32) {
        let Some(o) = self.offset(x, y) else { return };
        let amount = amount.clamp(0.0, 1.0);
        let keep = 1.0 - amount;
        for (k, shadow) in [10.0, 5.0, 20.0].iter().enumerate() {
            self.pixels[o + k] = (self.pixels[o + k] as f32 * keep + shadow * amount) as u8;
        }
    }

    /// A soft round glow of `radius` pixels.
    fn halo(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 3], amount: f32) {
        let r = radius.ceil() as i32;
        let (x0, y0) = (cx.floor() as i32, cy.floor() as i32);
        for y in -r..=r {
            for x in -r..=r {
                let d = ((x0 + x) as f32 + 0.5 - cx).hypot((y0 + y) as f32 + 0.5 - cy) / radius;
                if d < 1.0 {
                    self.glow(x0 + x, y0 + y, color, (1.0 - d) * (1.0 - d) * amount);
                }
            }
        }
    }

    /// A filled disc with a soft edge, white at the centre and `color` at the rim.
    /// A filled disc with a soft edge and a thin dark outline (so it reads on bright ground), white at the centre
    /// and `color` at the rim.
    fn orb(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 3], alpha: f32) {
        let r = radius.ceil() as i32 + 1;
        let (x0, y0) = (cx.floor() as i32, cy.floor() as i32);
        for y in -r..=r {
            for x in -r..=r {
                let d = ((x0 + x) as f32 + 0.5 - cx).hypot((y0 + y) as f32 + 0.5 - cy);
                let outline = 1.0 - (d - radius - 0.4).abs();
                if outline > 0.0 {
                    self.plot(x0 + x, y0 + y, [0.32, 0.14, 0.04], outline * 0.55 * alpha);
                }
                let edge = (radius - d + 0.5).clamp(0.0, 1.0);
                if edge > 0.0 {
                    let k = (d / radius).powi(2).min(1.0);
                    let c = [1.0 + (color[0] - 1.0) * k, 0.98 + (color[1] - 0.98) * k, 0.9 + (color[2] - 0.9) * k];
                    self.plot(x0 + x, y0 + y, c, edge * alpha);
                }
            }
        }
    }
}

impl Game {
    /// How far the hit shake moves the scene this frame. No shake when the player asked for reduced motion.
    pub(super) fn camera_offset(&self) -> (f32, f32) {
        if self.shake <= 0.0 || self.reduced_motion {
            return (0.0, 0.0);
        }
        let t = self.clock;
        ((t * 91.0).sin() * self.shake * 6.0, (t * 77.0).cos() * self.shake * 6.0)
    }

    /// How strongly the player is drawn: it blinks while it can't be hit, or, with reduced motion, it is drawn at
    /// half strength instead of blinking.
    pub(super) fn player_alpha(&self) -> f32 {
        let visible = self.state == State::Playing || (self.state == State::Over && self.over_time < 0.6);
        if !visible {
            0.0
        } else if self.invulnerable <= 0.0 {
            1.0
        } else if self.reduced_motion {
            0.5
        } else if (self.clock * 20.0).sin() < 0.0 {
            0.0
        } else {
            1.0
        }
    }

    pub(super) fn render(&mut self) {
        let t = self.clock;
        let (ox, oy) = self.camera_offset();
        let light_radius = self.light_radius();
        let player_alpha = self.player_alpha();
        // With reduced motion every flicker holds still at its usual brightness.
        let still = self.reduced_motion;
        let firefly_flicker = |phase: f32| if still { 0.85 } else { 0.7 + 0.3 * (phase * 7.0).sin() };
        let frame = &mut self.frame;

        // Light: moonlight, the fireflies, the wisps' eyes, the player and the sparks.
        let ambient = if self.state == State::Title { AMBIENT_TITLE } else { AMBIENT_PLAY };
        frame.clear_light(ambient);
        for f in &self.fireflies {
            frame.add_light(f.x + ox, f.y + oy, 12.0, FIREFLY, 0.5 * firefly_flicker(f.phase));
        }
        for wisp in &self.wisps {
            frame.add_light(wisp.x + ox, wisp.y + oy - 2.0, 9.0, WISP_EYE, 0.22);
        }
        if self.state != State::Title {
            let fade = if self.state == State::Over { (1.0 - self.over_time * 0.8).max(0.0) } else { 1.0 };
            let flicker = if still { 0.94 } else { 0.94 + 0.06 * (t * 13.0).sin() * (t * 7.3).cos() };
            let (x, y) = (self.px + ox, self.py + oy);
            let strength = flicker * fade.max(0.1);
            frame.add_light(x, y, light_radius * fade.max(0.25), GOLD, 1.0 * strength);
            // A hot centre, so the light reads as a flame and not a flat disc.
            frame.add_light(x, y, 12.0, GOLD, 0.5 * strength);
        }
        for p in &self.particles {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            frame.add_light(p.x + ox, p.y + oy, 5.0, p.color, 0.6 * k);
        }
        let grey = if self.state == State::Over { (self.over_time * 0.6).min(0.7) } else { 0.0 };
        frame.compose(ambient, grey);

        // Wisps: shadows with a violet rim, a trail and red eyes; always visible, so they are never unfair.
        for wisp in &self.wisps {
            let (cx, cy) = (wisp.x + ox, wisp.y + oy);
            let speed = wisp.vx.hypot(wisp.vy).max(0.001);
            let (bx, by) = (-wisp.vx / speed, -wisp.vy / speed);
            for (step, (size, amount)) in [(4.5, 0.55), (3.5, 0.4), (2.5, 0.25)].into_iter().enumerate() {
                let back = 4.0 + step as f32 * 3.5;
                let wave = (wisp.phase * 2.0 + step as f32).sin() * 1.2;
                let (tx, ty) = (cx + bx * back - by * wave, cy + by * back + bx * wave);
                shadow(frame, tx, ty, size, amount);
            }
            let (ix, iy) = (cx as i32, cy as i32);
            for y in -10i32..=10 {
                for x in -10i32..=10 {
                    let angle = (y as f32).atan2(x as f32);
                    let edge = 7.0 + (angle * 3.0 + wisp.phase * 2.0).sin() * 1.1;
                    let d = ((x * x + y * y) as f32).sqrt();
                    if d < edge {
                        frame.darken(ix + x, iy + y, 0.92 - (d / edge).powi(3) * 0.3);
                    }
                    let rim = 1.0 - (d - edge + 0.6).abs() / 1.2;
                    if rim > 0.0 {
                        frame.glow(ix + x, iy + y, WISP_RIM, rim * 0.32);
                    }
                }
            }
            let look = (self.px - wisp.x).signum() as i32;
            for (ex, ey) in [(-3, -2), (-2, -2), (-3, -1), (-2, -1), (2, -2), (3, -2), (2, -1), (3, -1)] {
                frame.plot(ix + ex + look, iy + ey, WISP_EYE, 1.0);
            }
            frame.glow(ix - 3 + look, iy - 2, [1.0, 0.8, 0.7], 0.35);
            frame.glow(ix + 2 + look, iy - 2, [1.0, 0.8, 0.7], 0.35);
        }

        for f in &self.fireflies {
            let flicker = firefly_flicker(f.phase);
            let (x, y) = (f.x + ox, f.y + oy);
            frame.halo(x, y, 3.5, FIREFLY, 0.7 * flicker);
            frame.plot(x as i32, y as i32, [1.0, 1.0, 0.82], 0.6 + 0.4 * flicker);
        }
        for p in &self.particles {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            frame.glow((p.x + ox) as i32, (p.y + oy) as i32, p.color, k);
        }

        // The player: a bright core with a soft golden halo.
        if player_alpha > 0.0 {
            let (x, y) = (self.px + ox, self.py + oy);
            frame.halo(x, y, 8.0, GOLD, 0.55 * player_alpha);
            frame.orb(x, y, 3.9, [1.0, 0.58, 0.16], player_alpha);
        }
    }
}

/// A soft dark disc: part of a wisp's trail.
fn shadow(frame: &mut Frame, cx: f32, cy: f32, radius: f32, amount: f32) {
    let r = radius.ceil() as i32;
    let (x0, y0) = (cx as i32, cy as i32);
    for y in -r..=r {
        for x in -r..=r {
            let d = ((x * x + y * y) as f32).sqrt() / radius;
            if d < 1.0 {
                frame.darken(x0 + x, y0 + y, (1.0 - d * d) * amount);
            }
        }
    }
}
