use std::collections::VecDeque;
use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use termsaver_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct LissajousConfig {
    speed: f64,
    trail_length: usize,
    damping: f64,
    color_mode: String,
    morph_interval_secs: f64,
}

impl Default for LissajousConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            trail_length: 3000,
            damping: 0.002,
            color_mode: "rainbow".into(),
            morph_interval_secs: 8.0,
        }
    }
}

fn braille_bit(dx: usize, dy: usize) -> u8 {
    match (dx, dy) {
        (0, 0) => 1 << 0,
        (0, 1) => 1 << 1,
        (0, 2) => 1 << 2,
        (1, 0) => 1 << 3,
        (1, 1) => 1 << 4,
        (1, 2) => 1 << 5,
        (0, 3) => 1 << 6,
        (1, 3) => 1 << 7,
        _ => 0,
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

struct HarmonographParams {
    a: f64,
    b: f64,
    delta: f64,
    d1: f64,
    d2: f64,
    amplitude_x: f64,
    amplitude_y: f64,
}

struct Lissajous {
    width: u16,
    height: u16,
    dot_width: usize,
    dot_height: usize,
    trail: VecDeque<(f64, f64)>,
    trail_length: usize,
    t: f64,
    speed: f64,
    damping: f64,
    color_mode: String,
    morph_interval_secs: f64,
    morph_timer: f64,
    params: HarmonographParams,
}

impl Lissajous {
    fn new(config: LissajousConfig) -> Self {
        let params = Self::random_params(config.damping);
        Self {
            width: 80,
            height: 24,
            dot_width: 160,
            dot_height: 96,
            trail: VecDeque::new(),
            trail_length: config.trail_length,
            t: 0.0,
            speed: config.speed,
            damping: config.damping,
            color_mode: config.color_mode.clone(),
            morph_interval_secs: config.morph_interval_secs,
            morph_timer: 0.0,
            params,
        }
    }

    fn random_params(damping: f64) -> HarmonographParams {
        let mut rng = rand::rng();
        let base_a: f64 = rng.random_range(1..=5) as f64;
        let base_b: f64 = rng.random_range(1..=5) as f64;
        let drift = rng.random_range(-0.02..0.02);
        HarmonographParams {
            a: base_a + drift,
            b: base_b + rng.random_range(-0.02..0.02),
            delta: rng.random_range(0.0..std::f64::consts::TAU),
            d1: damping * rng.random_range(0.5..2.0),
            d2: damping * rng.random_range(0.5..2.0),
            amplitude_x: 0.9,
            amplitude_y: 0.9,
        }
    }

    fn evaluate(&self, t: f64) -> (f64, f64) {
        let p = &self.params;
        let x = p.amplitude_x * (p.a * t + p.delta).sin() * (-p.d1 * t).exp();
        let y = p.amplitude_y * (p.b * t).sin() * (-p.d2 * t).exp();
        (x, y)
    }

    fn to_dot_space(&self, x: f64, y: f64) -> (f64, f64) {
        let cx = self.dot_width as f64 / 2.0;
        let cy = self.dot_height as f64 / 2.0;
        let scale_x = cx * 0.9;
        let scale_y = cy * 0.9;
        (cx + x * scale_x, cy + y * scale_y)
    }

    fn is_decayed(&self) -> bool {
        let p = &self.params;
        let decay_x = (-p.d1 * self.t).exp();
        let decay_y = (-p.d2 * self.t).exp();
        decay_x < 0.01 && decay_y < 0.01
    }

    fn rebirth(&mut self) {
        self.params = Self::random_params(self.damping);
        self.t = 0.0;
        self.trail.clear();
        self.morph_timer = 0.0;
    }

    fn trail_color(&self, index: usize, total: usize) -> PluginColor {
        if total == 0 {
            return PluginColor::Rgb(255, 255, 255);
        }
        let freshness = index as f64 / total as f64;
        let brightness = (freshness * 255.0).clamp(30.0, 255.0) as f64 / 255.0;

        match self.color_mode.as_str() {
            "rainbow" => {
                let hue = (index as f64 / total as f64) * 360.0;
                let (r, g, b) = hsv_to_rgb(hue % 360.0, 1.0, brightness);
                PluginColor::Rgb(r, g, b)
            }
            _ => {
                // "mono" or any other value
                let v = (brightness * 255.0) as u8;
                PluginColor::Rgb(v, v, v)
            }
        }
    }
}

impl ScreensaverPlugin for Lissajous {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "lissajous".into(),
            description: "Lissajous harmonograph with Braille subpixel rendering".into(),
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Braille subpixel rendering".into(),
                "Harmonograph with damping".into(),
                "Auto-morph with new parameters".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "speed".into(),
                    label: "Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "trail_length".into(),
                    label: "Trail Length".into(),
                    kind: FieldKind::USize,
                    default: "3000".into(),
                    choices: None,
                },
                ConfigField {
                    key: "damping".into(),
                    label: "Damping".into(),
                    kind: FieldKind::F64,
                    default: "0.002".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color_mode".into(),
                    label: "Color Mode".into(),
                    kind: FieldKind::Text,
                    default: "rainbow".into(),
                    choices: Some(vec!["rainbow".into(), "theme".into()]),
                },
                ConfigField {
                    key: "morph_interval_secs".into(),
                    label: "Morph Interval (secs)".into(),
                    kind: FieldKind::F64,
                    default: "8.0".into(),
                    choices: None,
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.dot_width = width as usize * 2;
        self.dot_height = height as usize * 4;
        self.trail.clear();
        self.t = 0.0;
        self.morph_timer = 0.0;
        self.params = Self::random_params(self.damping);
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();

        self.morph_timer += dt_secs;

        if self.morph_timer >= self.morph_interval_secs || self.is_decayed() {
            self.rebirth();
        }

        let steps = 8;
        let step_dt = dt_secs * self.speed / steps as f64;
        for _ in 0..steps {
            self.t += step_dt * 4.0;
            let (nx, ny) = self.evaluate(self.t);
            let (dx, dy) = self.to_dot_space(nx, ny);
            self.trail.push_back((dx, dy));
        }

        while self.trail.len() > self.trail_length {
            self.trail.pop_front();
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let cell_cols = buf.width() as usize;
        let cell_rows = buf.height() as usize;

        if cell_cols == 0 || cell_rows == 0 {
            return;
        }

        let grid_size = cell_cols * cell_rows;
        let mut braille_grid: Vec<u8> = vec![0; grid_size];
        let mut color_index: Vec<usize> = vec![0; grid_size];

        let trail_len = self.trail.len();

        for (i, &(dx, dy)) in self.trail.iter().enumerate() {
            let dot_x = dx.round() as isize;
            let dot_y = dy.round() as isize;

            if dot_x < 0 || dot_y < 0 {
                continue;
            }

            let dot_x = dot_x as usize;
            let dot_y = dot_y as usize;

            let cell_x = dot_x / 2;
            let cell_y = dot_y / 4;

            if cell_x >= cell_cols || cell_y >= cell_rows {
                continue;
            }

            let sub_x = dot_x % 2;
            let sub_y = dot_y % 4;

            let grid_idx = cell_y * cell_cols + cell_x;
            braille_grid[grid_idx] |= braille_bit(sub_x, sub_y);
            color_index[grid_idx] = i;
        }

        for cy in 0..cell_rows {
            for cx in 0..cell_cols {
                let grid_idx = cy * cell_cols + cx;
                let pattern = braille_grid[grid_idx];
                if pattern == 0 {
                    continue;
                }

                let ch = char::from_u32(0x2800 + pattern as u32).unwrap_or(' ');
                let color = self.trail_color(color_index[grid_idx], trail_len);

                let px = cx as u16;
                let py = cy as u16;
                if px < buf.width() && py < buf.height() {
                    buf.set(px, py, Cell { ch, fg: color, bg: PluginColor::Reset });
                }
            }
        }
    }
}

fn main() {
    let config = LissajousConfig::default();
    termsaver_plugin::run(Lissajous::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_lissajous() -> Lissajous {
        let config = LissajousConfig::default();
        Lissajous::new(config)
    }

    #[test]
    fn braille_dot_mapping_top_left() {
        assert_eq!(braille_bit(0, 0), 0b0000_0001);
    }

    #[test]
    fn braille_dot_mapping_mid_left() {
        assert_eq!(braille_bit(0, 1), 0b0000_0010);
    }

    #[test]
    fn braille_dot_mapping_bottom_left_row2() {
        assert_eq!(braille_bit(0, 2), 0b0000_0100);
    }

    #[test]
    fn braille_dot_mapping_top_right() {
        assert_eq!(braille_bit(1, 0), 0b0000_1000);
    }

    #[test]
    fn braille_dot_mapping_mid_right() {
        assert_eq!(braille_bit(1, 1), 0b0001_0000);
    }

    #[test]
    fn braille_dot_mapping_bottom_right_row2() {
        assert_eq!(braille_bit(1, 2), 0b0010_0000);
    }

    #[test]
    fn braille_dot_mapping_bottom_left_row3() {
        assert_eq!(braille_bit(0, 3), 0b0100_0000);
    }

    #[test]
    fn braille_dot_mapping_bottom_right_row3() {
        assert_eq!(braille_bit(1, 3), 0b1000_0000);
    }

    #[test]
    fn braille_dot_mapping_out_of_range() {
        assert_eq!(braille_bit(2, 0), 0);
        assert_eq!(braille_bit(0, 4), 0);
        assert_eq!(braille_bit(3, 5), 0);
    }

    #[test]
    fn braille_full_cell_all_dots() {
        let mut pattern: u8 = 0;
        for dx in 0..2 {
            for dy in 0..4 {
                pattern |= braille_bit(dx, dy);
            }
        }
        assert_eq!(pattern, 0xFF);
        let ch = char::from_u32(0x2800 + pattern as u32).unwrap();
        assert_eq!(ch, '\u{28FF}');
    }

    #[test]
    fn braille_empty_cell() {
        let ch = char::from_u32(0x2800).unwrap();
        assert_eq!(ch, '\u{2800}');
    }

    #[test]
    fn init_creates_valid_state() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        assert_eq!(liss.width, 80);
        assert_eq!(liss.height, 24);
        assert_eq!(liss.dot_width, 160);
        assert_eq!(liss.dot_height, 96);
        assert!(liss.trail.is_empty());
        assert_eq!(liss.t, 0.0);
        assert_eq!(liss.morph_timer, 0.0);
    }

    #[test]
    fn init_reinitializes_on_resize() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        for _ in 0..10 {
            liss.update(Duration::from_millis(33));
        }
        assert!(!liss.trail.is_empty());

        liss.init(120, 40);
        assert_eq!(liss.width, 120);
        assert_eq!(liss.height, 40);
        assert_eq!(liss.dot_width, 240);
        assert_eq!(liss.dot_height, 160);
        assert!(liss.trail.is_empty());
        assert_eq!(liss.t, 0.0);
    }

    #[test]
    fn parameter_regeneration_on_morph() {
        let mut liss = make_lissajous();
        liss.init(80, 24);

        let old_a = liss.params.a;
        let old_b = liss.params.b;
        let old_delta = liss.params.delta;

        liss.rebirth();

        let params_changed =
            (liss.params.a - old_a).abs() > 1e-10
            || (liss.params.b - old_b).abs() > 1e-10
            || (liss.params.delta - old_delta).abs() > 1e-10;
        assert!(params_changed, "Parameters should change after rebirth");
        assert_eq!(liss.t, 0.0);
        assert!(liss.trail.is_empty());
    }

    #[test]
    fn morph_timer_triggers_rebirth() {
        let mut config = LissajousConfig::default();
        config.morph_interval_secs = 0.1;
        let mut liss = Lissajous::new(config);
        liss.init(80, 24);

        for _ in 0..10 {
            liss.update(Duration::from_millis(20));
        }
        assert!(liss.morph_timer < liss.morph_interval_secs + 0.05);
    }

    #[test]
    fn trail_length_is_bounded() {
        let mut config = LissajousConfig::default();
        config.trail_length = 100;
        let mut liss = Lissajous::new(config);
        liss.init(80, 24);

        for _ in 0..200 {
            liss.update(Duration::from_millis(33));
        }
        assert!(liss.trail.len() <= 100);
    }

    #[test]
    fn trail_accumulates_points() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        assert!(liss.trail.is_empty());

        liss.update(Duration::from_millis(33));
        assert!(!liss.trail.is_empty());
    }

    #[test]
    fn evaluate_returns_bounded_values_at_start() {
        let liss = make_lissajous();
        let (x, y) = liss.evaluate(0.0);
        assert!(x.abs() <= 1.0, "x should be in [-1,1], got {x}");
        assert!(y.abs() <= 1e-10, "y should be ~0 at t=0, got {y}");
    }

    #[test]
    fn evaluate_decays_over_time() {
        let liss = make_lissajous();
        let (x, y) = liss.evaluate(10000.0);
        assert!(x.abs() < 0.01, "x should have decayed, got {x}");
        assert!(y.abs() < 0.01, "y should have decayed, got {y}");
    }

    #[test]
    fn decay_detection_works() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        assert!(!liss.is_decayed(), "Should not be decayed at t=0");

        liss.t = 100000.0;
        assert!(liss.is_decayed(), "Should be decayed at very large t");
    }

    #[test]
    fn to_dot_space_center() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        let (dx, dy) = liss.to_dot_space(0.0, 0.0);
        assert!((dx - 80.0).abs() < 0.01);
        assert!((dy - 48.0).abs() < 0.01);
    }

    #[test]
    fn to_dot_space_edges() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        let (dx, _) = liss.to_dot_space(1.0, 0.0);
        assert!(dx > 100.0, "Right edge point should be past center");
        assert!(dx < 160.0, "Right edge point should be within canvas");
    }

    #[test]
    fn hsv_red() {
        let (r, g, b) = hsv_to_rgb(0.0, 1.0, 1.0);
        assert_eq!(r, 255);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn hsv_green() {
        let (r, g, b) = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 255);
        assert_eq!(b, 0);
    }

    #[test]
    fn hsv_blue() {
        let (r, g, b) = hsv_to_rgb(240.0, 1.0, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 255);
    }

    #[test]
    fn hsv_white() {
        let (r, g, b) = hsv_to_rgb(0.0, 0.0, 1.0);
        assert_eq!(r, 255);
        assert_eq!(g, 255);
        assert_eq!(b, 255);
    }

    #[test]
    fn hsv_black() {
        let (r, g, b) = hsv_to_rgb(0.0, 0.0, 0.0);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn color_modes_produce_valid_colors() {
        let liss = make_lissajous();
        let color = liss.trail_color(50, 100);
        assert!(matches!(color, PluginColor::Rgb(_, _, _)));

        let mut config = LissajousConfig::default();
        config.color_mode = "mono".into();
        let mono_liss = Lissajous::new(config);
        let color = mono_liss.trail_color(50, 100);
        assert!(matches!(color, PluginColor::Rgb(_, _, _)));
    }

    #[test]
    fn color_newest_is_brightest() {
        let mut config = LissajousConfig::default();
        config.color_mode = "mono".into();
        let liss = Lissajous::new(config);
        let new_color = liss.trail_color(99, 100);
        let old_color = liss.trail_color(0, 100);
        if let (PluginColor::Rgb(nr, ng, nb), PluginColor::Rgb(or, og, ob)) = (new_color, old_color) {
            let new_brightness = nr as u16 + ng as u16 + nb as u16;
            let old_brightness = or as u16 + og as u16 + ob as u16;
            assert!(
                new_brightness >= old_brightness,
                "Newest point ({new_brightness}) should be at least as bright as oldest ({old_brightness})"
            );
        }
    }

    #[test]
    fn name_and_description() {
        let liss = make_lissajous();
        assert_eq!(liss.info().name, "lissajous");
        assert!(!liss.info().description.is_empty());
    }

    #[test]
    fn random_params_produces_valid_ranges() {
        for _ in 0..50 {
            let p = Lissajous::random_params(0.002);
            assert!(p.a > 0.5 && p.a < 5.5, "a out of range: {}", p.a);
            assert!(p.b > 0.5 && p.b < 5.5, "b out of range: {}", p.b);
            assert!(p.delta >= 0.0 && p.delta < std::f64::consts::TAU);
            assert!(p.d1 > 0.0);
            assert!(p.d2 > 0.0);
            assert!((p.amplitude_x - 0.9).abs() < 1e-10);
            assert!((p.amplitude_y - 0.9).abs() < 1e-10);
        }
    }

    #[test]
    fn render_does_not_panic() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        for _ in 0..10 {
            liss.update(Duration::from_millis(33));
        }
        let mut buf = PluginBuffer::new(80, 24);
        liss.render(&mut buf);
    }
}
