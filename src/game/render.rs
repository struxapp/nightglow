//! Software rendering: the meadow is lit pixel by pixel from the player's light, the fireflies and the sparks,
//! then the wisps, fireflies, sparks and the player are drawn on top.

use super::{Game, State};

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

impl Frame {
    /// A buffer of `w × h` (clamped to 160..=480 × 120..=480) with the ground for `seed`.
    pub fn new(w: usize, h: usize, seed: u32) -> Self {
        let (w, h) = clamp_size(w, h);
        let mut ground = Vec::with_capacity(w * h);
        let mut vignette = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32, y as f32);
                let n = smooth_noise(fx / 22.0, fy / 22.0, seed) * 0.6
                    + smooth_noise(fx / 7.0, fy / 7.0, seed ^ 0x55) * 0.3
                    + smooth_noise(fx / 2.5, fy / 2.5, seed ^ 0xaa) * 0.1;
                let moss = smooth_noise(fx / 40.0 + 9.0, fy / 40.0, seed ^ 0x1234);
                let blade = if hash(x as i32, y as i32, seed ^ 0x77) > 0.93 { 0.25 } else { 0.0 };
                let stone = smooth_noise(fx / 11.0 + 50.0, fy / 11.0, seed ^ 0x99) > 0.78;
                ground.push(if stone {
                    [0.42 + n * 0.1, 0.42 + n * 0.1, 0.46 + n * 0.1]
                } else {
                    [0.16 + n * 0.16 + moss * 0.06, 0.34 + n * 0.28 + blade, 0.20 + n * 0.12 - moss * 0.04]
                });
                let dx = (fx / w as f32 - 0.5) * 2.0;
                let dy = (fy / h as f32 - 0.5) * 2.0;
                vignette.push((1.0 - (dx * dx + dy * dy) * 0.28).clamp(0.45, 1.0));
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

    /// Ground × light, greyed by `grey` (0..1), then the vignette.
    fn compose(&mut self, grey: f32) {
        let cells = self.ground.iter().zip(&self.light).zip(&self.vignette);
        for (pixel, ((g, l), v)) in self.pixels.as_chunks_mut::<4>().0.iter_mut().zip(cells) {
            let mut c = [g[0] * l[0] * 1.7, g[1] * l[1] * 1.7, g[2] * l[2] * 1.7];
            if grey > 0.0 {
                let mean = (c[0] + c[1] + c[2]) / 3.0;
                c = c.map(|channel| channel + (mean - channel) * grey);
            }
            pixel[0] = (c[0] * v * 255.0).min(255.0) as u8;
            pixel[1] = (c[1] * v * 255.0).min(255.0) as u8;
            pixel[2] = (c[2] * v * 255.0).min(255.0) as u8;
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

    /// Pulls one pixel toward the wisps' near-black violet.
    fn darken(&mut self, x: i32, y: i32, amount: f32) {
        let Some(o) = self.offset(x, y) else { return };
        let amount = amount.clamp(0.0, 1.0);
        let keep = 1.0 - amount;
        for (k, shadow) in [6.0, 4.0, 12.0].iter().enumerate() {
            self.pixels[o + k] = (self.pixels[o + k] as f32 * keep + shadow * amount) as u8;
        }
    }
}

impl Game {
    /// How far the hit shake moves the scene this frame.
    pub(super) fn camera_offset(&self) -> (f32, f32) {
        if self.shake <= 0.0 {
            return (0.0, 0.0);
        }
        let t = self.clock;
        ((t * 91.0).sin() * self.shake * 6.0, (t * 77.0).cos() * self.shake * 6.0)
    }

    pub(super) fn render(&mut self) {
        let t = self.clock;
        let (ox, oy) = self.camera_offset();
        let light_radius = self.light_radius();
        let frame = &mut self.frame;

        // Light: the ambient night, the fireflies, the player and the sparks.
        frame.clear_light(if self.state == State::Title { [0.09, 0.10, 0.17] } else { [0.05, 0.06, 0.11] });
        for f in &self.fireflies {
            let flicker = 0.75 + 0.25 * (f.phase * 7.0).sin();
            frame.add_light(f.x + ox, f.y + oy, 15.0, [0.75, 1.0, 0.45], 0.75 * flicker);
        }
        if self.state != State::Title {
            let fade = if self.state == State::Over { (1.0 - self.over_time * 0.8).max(0.0) } else { 1.0 };
            let flicker = 0.92 + 0.08 * (t * 13.0).sin() * (t * 7.3).cos();
            let radius = light_radius * fade.max(0.25);
            frame.add_light(self.px + ox, self.py + oy, radius, [1.0, 0.82, 0.55], 1.25 * flicker * fade.max(0.1));
        }
        for p in &self.particles {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            frame.add_light(p.x + ox, p.y + oy, 4.0, p.color, 0.5 * k);
        }
        let grey = if self.state == State::Over { (self.over_time * 0.6).min(0.7) } else { 0.0 };
        frame.compose(grey);

        // Wisps: dark, wavy blobs with two glowing eyes, always visible so they are never unfair.
        for wisp in &self.wisps {
            let (cx, cy) = ((wisp.x + ox) as i32, (wisp.y + oy) as i32);
            for y in -9i32..=9 {
                for x in -9i32..=9 {
                    let angle = (y as f32).atan2(x as f32);
                    let edge = 6.5 + (angle * 3.0 + wisp.phase * 2.0).sin() * 1.3;
                    let d = ((x * x + y * y) as f32).sqrt();
                    if d < edge {
                        frame.darken(cx + x, cy + y, (1.0 - d / edge).min(1.0) * 0.85 + 0.1);
                    }
                }
            }
            let look = (self.px - wisp.x).signum() as i32;
            let eye = [1.0, 0.36, 0.3];
            frame.plot(cx - 2 + look, cy - 1, eye, 0.95);
            frame.plot(cx + 2 + look, cy - 1, eye, 0.95);
        }

        for f in &self.fireflies {
            let flicker = 0.7 + 0.3 * (f.phase * 7.0).sin();
            let (x, y) = ((f.x + ox) as i32, (f.y + oy) as i32);
            frame.plot(x, y, [0.9, 1.0, 0.6], flicker);
            frame.plot(x + 1, y, [0.7, 0.9, 0.4], flicker * 0.5);
        }
        for p in &self.particles {
            let k = (p.life / p.max_life).clamp(0.0, 1.0);
            frame.plot((p.x + ox) as i32, (p.y + oy) as i32, p.color, k);
        }

        // The player: a small bright core that blinks while it can't be hit.
        let visible = self.state == State::Playing || (self.state == State::Over && self.over_time < 0.6);
        let blink = self.invulnerable > 0.0 && (t * 20.0).sin() < 0.0;
        if visible && !blink {
            let (cx, cy) = ((self.px + ox) as i32, (self.py + oy) as i32);
            for y in -2i32..=2 {
                for x in -2i32..=2 {
                    let d = x * x + y * y;
                    if d <= 5 {
                        let core = if d <= 1 { [1.0, 1.0, 0.92] } else { [1.0, 0.85, 0.55] };
                        frame.plot(cx + x, cy + y, core, 1.0);
                    }
                }
            }
        }
    }
}
