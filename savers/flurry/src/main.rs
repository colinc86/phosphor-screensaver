use std::time::Duration;

use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};
use rand::Rng;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct FlurryConfig {
    streams: usize,
    intensity: f64,
    speed: f64,
    color_mode: String,
}

impl Default for FlurryConfig {
    fn default() -> Self {
        Self {
            streams: 5,
            intensity: 1.0,
            speed: 1.0,
            color_mode: "classic".into(),
        }
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = v - c;
    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

fn intensity_char(intensity: f64) -> char {
    if intensity < 0.05 {
        ' '
    } else if intensity < 0.15 {
        '\u{00B7}' // ·
    } else if intensity < 0.3 {
        '\u{2591}' // ░
    } else if intensity < 0.5 {
        '\u{2592}' // ▒
    } else if intensity < 0.75 {
        '\u{2593}' // ▓
    } else {
        '\u{2588}' // █
    }
}

/// Particle alpha/brightness based on age fraction (age/lifetime).
/// Ramps up during first 10%, then fades out.
fn particle_alpha(age_frac: f64) -> f64 {
    if !(0.0..=1.0).contains(&age_frac) {
        0.0
    } else if age_frac < 0.1 {
        age_frac / 0.1
    } else {
        1.0 - (age_frac - 0.1) / 0.9
    }
}

/// Complex Lissajous position for star/spark orbits.
/// Uses incommensurate frequencies so the path never exactly repeats.
/// `mystery` is a per-entity phase offset for unique paths.
fn lissajous_pos(t: f64, mystery: f64, scale_x: f64, scale_y: f64) -> (f64, f64) {
    let angle = t * 0.1;
    let cf = ((7.0 * (angle + mystery)).cos()
        + (3.0 * (angle + mystery)).cos()
        + (13.0 * (angle + mystery)).cos())
        / 6.0
        + 0.75;
    let x = scale_x * cf * (11.0 * (mystery + 3.0 * angle)).cos();
    let y = scale_y * cf * (12.0 * (mystery + 4.0 * angle)).sin();
    (x, y)
}

// --- Data structures ---

/// A smoke particle emitted from the star, attracted toward sparks.
struct SmokeParticle {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    prev_x: f64,
    prev_y: f64,
    age: f64,
    lifetime: f64,
    stream_idx: usize,
    color: (f64, f64, f64),
    width: f64,
}

/// An invisible attractor (spark) that particles are drawn toward.
struct Spark {
    mystery: f64,
    hue: f64,
    hue_drift: f64,
}

impl Spark {
    fn position(&self, t: f64, cx: f64, cy: f64, scale_x: f64, scale_y: f64) -> (f64, f64) {
        let (lx, ly) = lissajous_pos(t, self.mystery, scale_x, scale_y);
        (cx + lx, cy + ly)
    }

    fn color(&self, t: f64, color_mode: &str, sat: f64) -> (f64, f64, f64) {
        let hue = (self.hue + self.hue_drift * t).rem_euclid(360.0);
        match color_mode {
            "monochrome" => (255.0, 255.0, 255.0),
            _ => {
                let (r, g, b) = hsv_to_rgb(hue, sat, 1.0);
                (r as f64, g as f64, b as f64)
            }
        }
    }
}

struct Flurry {
    width: u16,
    height: u16,
    t: f64,
    star_mystery: f64,
    sparks: Vec<Spark>,
    particles: Vec<SmokeParticle>,
    spawn_accum: f64,
    // Persistent color accumulator for trail fade effect
    acc_r: Vec<f64>,
    acc_g: Vec<f64>,
    acc_b: Vec<f64>,
    acc_w: usize,
    acc_h: usize,
    config: FlurryConfig,
}

const MAX_PARTICLES: usize = 800;
const SPAWN_RATE: f64 = 40.0;
const GRAVITY: f64 = 80.0;
const STREAM_BIAS: f64 = 7.0;
const DRAG: f64 = 0.97;
const PARTICLE_LIFETIME: (f64, f64) = (2.0, 5.0);
const FADE_RATE: f64 = 0.88;

impl Flurry {
    fn new(config: FlurryConfig) -> Self {
        Self {
            width: 80,
            height: 24,
            t: 0.0,
            star_mystery: 0.0,
            sparks: Vec::new(),
            particles: Vec::new(),
            spawn_accum: 0.0,
            acc_r: Vec::new(),
            acc_g: Vec::new(),
            acc_b: Vec::new(),
            acc_w: 0,
            acc_h: 0,
            config,
        }
    }

    fn star_pos(&self) -> (f64, f64) {
        let cx = self.width as f64 / 2.0;
        let cy = self.height as f64 / 2.0;
        let scale_x = self.width as f64 * 0.2;
        let scale_y = self.height as f64 * 0.2;
        let (lx, ly) = lissajous_pos(self.t, self.star_mystery, scale_x, scale_y);
        (cx + lx, cy + ly)
    }

    fn spark_scale(&self) -> (f64, f64) {
        (self.width as f64 * 0.3, self.height as f64 * 0.3)
    }

    fn generate_sparks(&mut self) {
        let mut rng = rand::rng();
        self.sparks.clear();
        self.star_mystery = rng.random_range(0.0..std::f64::consts::TAU * 10.0);

        let n = self.config.streams;
        for i in 0..n {
            let hue = match self.config.color_mode.as_str() {
                "rainbow" => (i as f64 / n as f64) * 360.0,
                "monochrome" => 0.0,
                _ => {
                    // classic: warm/cool hues spread across spectrum
                    let choices = [300.0, 270.0, 220.0, 180.0, 330.0, 30.0, 60.0, 150.0];
                    choices[i % choices.len()] + rng.random_range(-15.0..15.0)
                }
            };

            self.sparks.push(Spark {
                mystery: rng.random_range(0.0..std::f64::consts::TAU * 10.0),
                hue,
                hue_drift: rng.random_range(2.0..10.0),
            });
        }
    }

    fn pre_populate(&mut self) {
        let mut rng = rand::rng();
        let cx = self.width as f64 / 2.0;
        let cy = self.height as f64 / 2.0;
        let (sx, sy) = self.spark_scale();
        let n_sparks = self.sparks.len();
        if n_sparks == 0 {
            return;
        }

        let sat = if self.config.color_mode == "classic" {
            0.8
        } else {
            1.0
        };

        // Simulate a few seconds of particle emission
        let sim_steps = 200;
        let sim_dt = 0.02;
        for step in 0..sim_steps {
            let fake_t = step as f64 * sim_dt;
            let (star_x, star_y) = {
                let scale_x = self.width as f64 * 0.2;
                let scale_y = self.height as f64 * 0.2;
                let (lx, ly) = lissajous_pos(fake_t, self.star_mystery, scale_x, scale_y);
                (cx + lx, cy + ly)
            };

            // Spawn some particles
            if step % 3 == 0 && self.particles.len() < MAX_PARTICLES {
                let stream_idx = rng.random_range(0..n_sparks);
                let spark = &self.sparks[stream_idx];
                let (spk_x, spk_y) = spark.position(fake_t, cx, cy, sx, sy);
                let toward_x = (spk_x - star_x) * 0.5;
                let toward_y = (spk_y - star_y) * 0.5;
                let (cr, cg, cb) = spark.color(fake_t, &self.config.color_mode, sat);
                self.particles.push(SmokeParticle {
                    x: star_x,
                    y: star_y,
                    vx: toward_x + rng.random_range(-1.0..1.0),
                    vy: toward_y + rng.random_range(-0.5..0.5),
                    prev_x: star_x,
                    prev_y: star_y,
                    age: 0.0,
                    lifetime: rng.random_range(PARTICLE_LIFETIME.0..PARTICLE_LIFETIME.1),
                    stream_idx,
                    color: (cr, cg, cb),
                    width: 1.0,
                });
            }

            // Simulate physics for all particles
            for p in &mut self.particles {
                p.prev_x = p.x;
                p.prev_y = p.y;

                for (si, spark) in self.sparks.iter().enumerate() {
                    let (spk_x, spk_y) = spark.position(fake_t, cx, cy, sx, sy);
                    let dx = spk_x - p.x;
                    let dy = spk_y - p.y;
                    let dist_sq = dx * dx + dy * dy + 1.0;
                    let bias = if si == p.stream_idx {
                        1.0 + STREAM_BIAS
                    } else {
                        1.0
                    };
                    let force = GRAVITY * bias / dist_sq;
                    let dist = dist_sq.sqrt();
                    p.vx += force * dx / dist * sim_dt;
                    p.vy += force * dy / dist * sim_dt;
                }

                p.vx *= DRAG;
                p.vy *= DRAG;
                p.x += p.vx * sim_dt;
                p.y += p.vy * sim_dt;
                p.age += sim_dt;
                p.width = 1.0 + p.age * 0.3;
            }

            self.particles.retain(|p| p.age < p.lifetime);
        }

        // Shift particle time so they appear to have been running
        self.t = sim_steps as f64 * sim_dt;
    }

    fn resize_accumulator(&mut self, w: usize, h: usize) {
        let size = w * h;
        self.acc_r = vec![0.0; size];
        self.acc_g = vec![0.0; size];
        self.acc_b = vec![0.0; size];
        self.acc_w = w;
        self.acc_h = h;
    }

    /// Draw a line segment into the accumulator with additive blending.
    #[allow(clippy::too_many_arguments)]
    fn draw_trail(
        acc_r: &mut [f64],
        acc_g: &mut [f64],
        acc_b: &mut [f64],
        w: usize,
        h: usize,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        cr: f64,
        cg: f64,
        cb: f64,
        width: f64,
    ) {
        // Bresenham between (x0,y0) and (x1,y1) with width-based splatting
        let mut ix0 = x0.round() as isize;
        let mut iy0 = y0.round() as isize;
        let ix1 = x1.round() as isize;
        let iy1 = y1.round() as isize;

        let dx = (ix1 - ix0).abs();
        let dy = -(iy1 - iy0).abs();
        let sx: isize = if ix0 < ix1 { 1 } else { -1 };
        let sy: isize = if iy0 < iy1 { 1 } else { -1 };
        let mut err = dx + dy;

        let half_w = (width * 0.5).ceil() as isize;

        loop {
            // Splat a small area around the point for width
            for oy in -half_w..=half_w {
                for ox in -half_w..=half_w {
                    let px = ix0 + ox;
                    let py = iy0 + oy;
                    if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        let dist = ((ox * ox + oy * oy) as f64).sqrt();
                        let falloff = (1.0 - dist / (half_w as f64 + 1.0)).max(0.0);
                        let idx = py as usize * w + px as usize;
                        acc_r[idx] += cr * falloff;
                        acc_g[idx] += cg * falloff;
                        acc_b[idx] += cb * falloff;
                    }
                }
            }

            if ix0 == ix1 && iy0 == iy1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                ix0 += sx;
            }
            if e2 <= dx {
                err += dx;
                iy0 += sy;
            }
        }
    }
}

impl ScreensaverPlugin for Flurry {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "flurry".into(),
            description: "Glowing smoke streams inspired by macOS Flurry".into(),
            interactive: false,
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Star/spark/smoke particle hierarchy".into(),
                "Gravitational attraction with stream bias".into(),
                "Motion-trail rendering with additive blending".into(),
                "Persistent glow with gradual fade".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "streams".into(),
                    label: "Streams".into(),
                    kind: FieldKind::USize,
                    default: "5".into(),
                    choices: None,
                },
                ConfigField {
                    key: "intensity".into(),
                    label: "Intensity".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "speed".into(),
                    label: "Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color_mode".into(),
                    label: "Color Mode".into(),
                    kind: FieldKind::Text,
                    default: "classic".into(),
                    choices: Some(vec![
                        "classic".into(),
                        "rainbow".into(),
                        "monochrome".into(),
                    ]),
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.t = 0.0;
        self.particles.clear();
        self.spawn_accum = 0.0;
        self.resize_accumulator(width as usize, height as usize);
        self.generate_sparks();
        self.pre_populate();
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64() * self.config.speed;
        self.t += dt_secs;

        let mut rng = rand::rng();
        let cx = self.width as f64 / 2.0;
        let cy = self.height as f64 / 2.0;
        let (sx, sy) = self.spark_scale();
        let n_sparks = self.sparks.len();
        if n_sparks == 0 {
            return;
        }

        let sat = if self.config.color_mode == "classic" {
            0.8
        } else {
            1.0
        };

        // Spawn new smoke particles from the star
        let (star_x, star_y) = self.star_pos();
        self.spawn_accum += SPAWN_RATE * dt_secs;
        while self.spawn_accum >= 1.0 && self.particles.len() < MAX_PARTICLES {
            self.spawn_accum -= 1.0;
            let stream_idx = rng.random_range(0..n_sparks);
            let spark = &self.sparks[stream_idx];
            let (spk_x, spk_y) = spark.position(self.t, cx, cy, sx, sy);

            // Initial velocity: toward assigned spark + some randomness
            let toward_x = (spk_x - star_x) * 0.5;
            let toward_y = (spk_y - star_y) * 0.5;

            let (cr, cg, cb) = spark.color(self.t, &self.config.color_mode, sat);
            // Add color incoherence
            let jitter = 0.15;
            let cr = (cr * (1.0 + rng.random_range(-jitter..jitter))).clamp(0.0, 255.0);
            let cg = (cg * (1.0 + rng.random_range(-jitter..jitter))).clamp(0.0, 255.0);
            let cb = (cb * (1.0 + rng.random_range(-jitter..jitter))).clamp(0.0, 255.0);

            self.particles.push(SmokeParticle {
                x: star_x + rng.random_range(-0.5..0.5),
                y: star_y + rng.random_range(-0.25..0.25),
                vx: toward_x + rng.random_range(-1.5..1.5),
                vy: toward_y + rng.random_range(-0.75..0.75),
                prev_x: star_x,
                prev_y: star_y,
                age: 0.0,
                lifetime: rng.random_range(PARTICLE_LIFETIME.0..PARTICLE_LIFETIME.1),
                stream_idx,
                color: (cr, cg, cb),
                width: 1.0,
            });
        }

        // Physics update: gravitational attraction toward sparks
        for p in &mut self.particles {
            p.prev_x = p.x;
            p.prev_y = p.y;

            // Attract toward all sparks, with stream bias for assigned spark
            for (si, spark) in self.sparks.iter().enumerate() {
                let (spk_x, spk_y) = spark.position(self.t, cx, cy, sx, sy);
                let dx = spk_x - p.x;
                let dy = spk_y - p.y;
                let dist_sq = dx * dx + dy * dy + 1.0; // +1 prevents singularity
                let bias = if si == p.stream_idx {
                    1.0 + STREAM_BIAS
                } else {
                    1.0
                };
                let force = GRAVITY * bias / dist_sq;
                let dist = dist_sq.sqrt();
                p.vx += force * dx / dist * dt_secs;
                p.vy += force * dy / dist * dt_secs;
            }

            // Drag
            let drag_per_frame = DRAG.powf(dt_secs * 60.0);
            p.vx *= drag_per_frame;
            p.vy *= drag_per_frame;

            // Integrate position
            p.x += p.vx * dt_secs;
            p.y += p.vy * dt_secs;

            // Age and expand
            p.age += dt_secs;
            p.width = 1.0 + p.age * 0.3;
        }

        // Remove dead particles
        self.particles.retain(|p| p.age < p.lifetime);
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let w = buf.width() as usize;
        let h = buf.height() as usize;

        if w == 0 || h == 0 {
            return;
        }

        // Resize accumulator if needed (handles first render after init)
        // We use a local accumulator that starts from the persistent one
        let size = w * h;
        let (mut acc_r, mut acc_g, mut acc_b) = if self.acc_w == w && self.acc_h == h {
            // Fade existing accumulator
            let mut r = self.acc_r.clone();
            let mut g = self.acc_g.clone();
            let mut b = self.acc_b.clone();
            for i in 0..size {
                r[i] *= FADE_RATE;
                g[i] *= FADE_RATE;
                b[i] *= FADE_RATE;
            }
            (r, g, b)
        } else {
            (vec![0.0; size], vec![0.0; size], vec![0.0; size])
        };

        let intensity = self.config.intensity;

        // Draw motion trails for all particles (additive blending)
        for p in &self.particles {
            let alpha = particle_alpha(p.age / p.lifetime) * intensity;
            if alpha < 0.01 {
                continue;
            }

            let cr = p.color.0 * alpha / 255.0;
            let cg = p.color.1 * alpha / 255.0;
            let cb = p.color.2 * alpha / 255.0;

            // Scale color contribution
            let scale = 30.0;
            Self::draw_trail(
                &mut acc_r,
                &mut acc_g,
                &mut acc_b,
                w,
                h,
                p.prev_x,
                p.prev_y,
                p.x,
                p.y,
                cr * scale,
                cg * scale,
                cb * scale,
                p.width,
            );
        }

        // Convert accumulator to characters
        for cy in 0..h {
            for cx in 0..w {
                let idx = cy * w + cx;
                let r = acc_r[idx];
                let g = acc_g[idx];
                let b = acc_b[idx];

                let max_channel = r.max(g).max(b);
                if max_channel < 3.0 {
                    continue;
                }

                let norm = (max_channel / 255.0).min(1.0);
                let ch = intensity_char(norm);
                if ch == ' ' {
                    continue;
                }

                let ri = (r as u32).min(255) as u8;
                let gi = (g as u32).min(255) as u8;
                let bi = (b as u32).min(255) as u8;

                buf.set(
                    cx as u16,
                    cy as u16,
                    Cell {
                        ch,
                        fg: PluginColor::Rgb(ri, gi, bi),
                        bg: PluginColor::Reset,
                    },
                );
            }
        }
    }
}

fn main() {
    let config = FlurryConfig::default();
    phosphor_plugin::run(Flurry::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_flurry() -> Flurry {
        Flurry::new(FlurryConfig::default())
    }

    // --- HSV ---

    #[test]
    fn hsv_red() {
        let (r, g, b) = hsv_to_rgb(0.0, 1.0, 1.0);
        assert_eq!((r, g, b), (255, 0, 0));
    }

    #[test]
    fn hsv_green() {
        let (r, g, b) = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!((r, g, b), (0, 255, 0));
    }

    #[test]
    fn hsv_blue() {
        let (r, g, b) = hsv_to_rgb(240.0, 1.0, 1.0);
        assert_eq!((r, g, b), (0, 0, 255));
    }

    // --- Particle alpha ---

    #[test]
    fn alpha_zero_at_birth() {
        assert!((particle_alpha(0.0)).abs() < 1e-10);
    }

    #[test]
    fn alpha_peak_at_10_percent() {
        assert!((particle_alpha(0.1) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn alpha_zero_at_death() {
        assert!((particle_alpha(1.0)).abs() < 1e-10);
    }

    #[test]
    fn alpha_monotonically_decreasing_after_peak() {
        let mut prev = particle_alpha(0.1);
        for i in 2..=10 {
            let frac = i as f64 / 10.0;
            let a = particle_alpha(frac);
            assert!(
                a <= prev + 1e-10,
                "alpha should decrease: {prev} -> {a} at {frac}"
            );
            prev = a;
        }
    }

    // --- Intensity chars ---

    #[test]
    fn intensity_char_thresholds() {
        assert_eq!(intensity_char(0.0), ' ');
        assert_eq!(intensity_char(0.1), '\u{00B7}');
        assert_eq!(intensity_char(0.2), '\u{2591}');
        assert_eq!(intensity_char(0.4), '\u{2592}');
        assert_eq!(intensity_char(0.6), '\u{2593}');
        assert_eq!(intensity_char(0.9), '\u{2588}');
    }

    // --- Lissajous ---

    #[test]
    fn lissajous_produces_bounded_output() {
        for t in 0..100 {
            let (x, y) = lissajous_pos(t as f64 * 0.1, 1.5, 10.0, 10.0);
            assert!(x.abs() < 30.0, "x={x} out of bounds at t={t}");
            assert!(y.abs() < 30.0, "y={y} out of bounds at t={t}");
        }
    }

    #[test]
    fn lissajous_different_mystery_gives_different_paths() {
        let (x1, y1) = lissajous_pos(1.0, 0.0, 10.0, 10.0);
        let (x2, y2) = lissajous_pos(1.0, 3.0, 10.0, 10.0);
        assert!(
            (x1 - x2).abs() > 0.01 || (y1 - y2).abs() > 0.01,
            "Different mystery should give different positions"
        );
    }

    // --- Init ---

    #[test]
    fn init_correct_stream_count() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        assert_eq!(flurry.sparks.len(), flurry.config.streams);
    }

    #[test]
    fn init_pre_populates_particles() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        assert!(
            !flurry.particles.is_empty(),
            "Should pre-populate particles"
        );
    }

    #[test]
    fn reinit_resets_state() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        flurry.update(Duration::from_secs(5));
        flurry.init(120, 40);
        assert_eq!(flurry.width, 120);
        assert_eq!(flurry.height, 40);
        assert_eq!(flurry.sparks.len(), flurry.config.streams);
    }

    // --- Update ---

    #[test]
    fn update_advances_time() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        let t_before = flurry.t;
        flurry.update(Duration::from_millis(100));
        assert!(flurry.t > t_before);
    }

    #[test]
    fn update_removes_dead_particles() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        for _ in 0..200 {
            flurry.update(Duration::from_millis(50));
        }
        for p in &flurry.particles {
            assert!(p.age < p.lifetime);
        }
    }

    #[test]
    fn particles_move_toward_sparks() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        // Check that particles have non-zero velocity after update
        flurry.update(Duration::from_millis(100));
        let has_velocity = flurry
            .particles
            .iter()
            .any(|p| p.vx.abs() > 0.01 || p.vy.abs() > 0.01);
        assert!(
            has_velocity,
            "Particles should have velocity from gravitational attraction"
        );
    }

    // --- Render ---

    #[test]
    fn render_no_panic() {
        let mut flurry = make_flurry();
        flurry.init(40, 20);
        flurry.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(40, 20);
        flurry.render(&mut buf);
    }

    #[test]
    fn render_small_no_panic() {
        let mut flurry = make_flurry();
        flurry.init(4, 2);
        flurry.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(4, 2);
        flurry.render(&mut buf);
    }

    #[test]
    fn render_produces_output() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        for _ in 0..10 {
            flurry.update(Duration::from_millis(33));
        }
        let mut buf = PluginBuffer::new(80, 24);
        flurry.render(&mut buf);
        let non_empty = (0..80u16)
            .flat_map(|x| (0..24u16).map(move |y| (x, y)))
            .any(|(x, y)| buf.get(x, y).unwrap().fg != PluginColor::Reset);
        assert!(non_empty, "Should produce visible output");
    }

    // --- Info ---

    #[test]
    fn info_name_and_fields() {
        let flurry = make_flurry();
        let info = flurry.info();
        assert_eq!(info.name, "flurry");
        assert_eq!(info.config_fields.len(), 4);
    }

    // --- Particle stability ---

    #[test]
    fn particle_count_roughly_stable() {
        let mut flurry = make_flurry();
        flurry.init(80, 24);
        for _ in 0..100 {
            flurry.update(Duration::from_millis(33));
        }
        let count_a = flurry.particles.len();
        for _ in 0..100 {
            flurry.update(Duration::from_millis(33));
        }
        let count_b = flurry.particles.len();
        let ratio = count_a.max(count_b) as f64 / count_a.min(count_b).max(1) as f64;
        assert!(
            ratio < 3.0,
            "Particle counts should be roughly stable: {count_a} vs {count_b}"
        );
    }
}
