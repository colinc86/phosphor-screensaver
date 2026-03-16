use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::Event;
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::LissajousConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

/// Map a (dx, dy) position within a 2x4 braille cell to its bit index.
/// dx is 0 or 1 (column within cell), dy is 0..3 (row within cell).
/// Returns the bit to OR into the braille pattern byte.
fn braille_bit(dx: usize, dy: usize) -> u8 {
    // Standard braille dot numbering in a 2-wide x 4-tall grid:
    //   (0,0)=bit0  (1,0)=bit3
    //   (0,1)=bit1  (1,1)=bit4
    //   (0,2)=bit2  (1,2)=bit5
    //   (0,3)=bit6  (1,3)=bit7
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

/// Convert an HSV color (h in 0..360, s and v in 0..1) to an RGB tuple.
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

/// Harmonograph parameters for the Lissajous curve.
struct HarmonographParams {
    a: f64,
    b: f64,
    delta: f64,
    d1: f64,
    d2: f64,
    amplitude_x: f64,
    amplitude_y: f64,
}

pub struct Lissajous {
    width: u16,
    height: u16,
    /// Dot-space width (terminal width * 2)
    dot_width: usize,
    /// Dot-space height (terminal height * 4)
    dot_height: usize,
    /// Trail of (x, y) coordinates in dot-space
    trail: VecDeque<(f64, f64)>,
    /// Maximum trail length
    trail_length: usize,
    /// Current parametric time
    t: f64,
    /// Speed multiplier
    speed: f64,
    /// Damping coefficient
    damping: f64,
    /// Color mode: "rainbow", "theme", or "mono"
    color_mode: String,
    /// How many seconds between parameter regeneration
    morph_interval_secs: f64,
    /// Timer tracking time since last morph
    morph_timer: f64,
    /// Current harmonograph parameters
    params: HarmonographParams,
    /// Theme for "theme" color mode
    theme: Theme,
}

impl Lissajous {
    pub fn new(config: &LissajousConfig, theme: Theme) -> Self {
        let params = Self::random_params(0.002);
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
            theme,
        }
    }

    /// Generate random harmonograph parameters.
    fn random_params(damping: f64) -> HarmonographParams {
        let mut rng = rand::rng();
        // Use small integer ratios for interesting patterns, with slight drift
        let base_a: f64 = rng.random_range(1..=5) as f64;
        let base_b: f64 = rng.random_range(1..=5) as f64;
        // Add a small irrational offset so the pattern slowly morphs
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

    /// Evaluate the harmonograph at time t, returning (x, y) in normalized [-1, 1] space.
    fn evaluate(&self, t: f64) -> (f64, f64) {
        let p = &self.params;
        let x = p.amplitude_x * (p.a * t + p.delta).sin() * (-p.d1 * t).exp();
        let y = p.amplitude_y * (p.b * t).sin() * (-p.d2 * t).exp();
        (x, y)
    }

    /// Convert normalized [-1, 1] coordinates to dot-space coordinates.
    fn to_dot_space(&self, x: f64, y: f64) -> (f64, f64) {
        let cx = self.dot_width as f64 / 2.0;
        let cy = self.dot_height as f64 / 2.0;
        // Scale to fit with some margin
        let scale_x = cx * 0.9;
        let scale_y = cy * 0.9;
        (cx + x * scale_x, cy + y * scale_y)
    }

    /// Check whether the curve amplitude has decayed enough to warrant rebirth.
    fn is_decayed(&self) -> bool {
        let p = &self.params;
        let decay_x = (-p.d1 * self.t).exp();
        let decay_y = (-p.d2 * self.t).exp();
        decay_x < 0.01 && decay_y < 0.01
    }

    /// Reset the harmonograph with new random parameters.
    fn rebirth(&mut self) {
        self.params = Self::random_params(self.damping);
        self.t = 0.0;
        self.trail.clear();
        self.morph_timer = 0.0;
    }

    /// Compute the color for a trail point based on its position in the trail.
    fn trail_color(&self, index: usize, total: usize) -> Color {
        if total == 0 {
            return Color::White;
        }
        // freshness: 0.0 = oldest (index 0, front of deque), 1.0 = newest (back)
        let freshness = index as f64 / total as f64;
        // Brightness dims with age (older = dimmer)
        let brightness = (freshness * 255.0).clamp(30.0, 255.0) as f64 / 255.0;

        match self.color_mode.as_str() {
            "rainbow" => {
                // Cycle hue along the trail
                let hue = (index as f64 / total as f64) * 360.0;
                let (r, g, b) = hsv_to_rgb(hue % 360.0, 1.0, brightness);
                Color::Rgb(r, g, b)
            }
            "theme" => {
                // Use theme primary color, dimmed by age
                match self.theme.primary {
                    Color::Rgb(r, g, b) => Color::Rgb(
                        (r as f64 * brightness) as u8,
                        (g as f64 * brightness) as u8,
                        (b as f64 * brightness) as u8,
                    ),
                    _ => {
                        // Fallback for named colors: use cyan-ish
                        let v = (brightness * 255.0) as u8;
                        Color::Rgb(0, v, v)
                    }
                }
            }
            _ => {
                // "mono" or any other value: white, dimmed by age
                let v = (brightness * 255.0) as u8;
                Color::Rgb(v, v, v)
            }
        }
    }
}

impl Screensaver for Lissajous {
    fn name(&self) -> &str {
        "lissajous"
    }

    fn description(&self) -> &str {
        "Lissajous harmonograph with Braille subpixel rendering"
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

        // Advance morph timer
        self.morph_timer += dt_secs;

        // Check if it's time to morph or if the pattern has decayed
        if self.morph_timer >= self.morph_interval_secs || self.is_decayed() {
            self.rebirth();
        }

        // Advance the parametric time and add new trail points.
        // Add multiple points per frame for a smooth curve.
        let steps = 8;
        let step_dt = dt_secs * self.speed / steps as f64;
        for _ in 0..steps {
            self.t += step_dt * 4.0;
            let (nx, ny) = self.evaluate(self.t);
            let (dx, dy) = self.to_dot_space(nx, ny);
            self.trail.push_back((dx, dy));
        }

        // Trim trail to maximum length
        while self.trail.len() > self.trail_length {
            self.trail.pop_front();
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let cell_cols = area.width as usize;
        let cell_rows = area.height as usize;

        if cell_cols == 0 || cell_rows == 0 {
            return;
        }

        // Build a grid of braille bit patterns, one per terminal cell.
        // Also track which trail index contributed to each cell for coloring.
        let grid_size = cell_cols * cell_rows;
        let mut braille_grid: Vec<u8> = vec![0; grid_size];
        // Store the "best" (newest) trail index that touches each cell, for coloring.
        let mut color_index: Vec<usize> = vec![0; grid_size];

        let trail_len = self.trail.len();

        for (i, &(dx, dy)) in self.trail.iter().enumerate() {
            // Compute which terminal cell this dot falls in
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

            // Position within the braille cell
            let sub_x = dot_x % 2;
            let sub_y = dot_y % 4;

            let grid_idx = cell_y * cell_cols + cell_x;
            braille_grid[grid_idx] |= braille_bit(sub_x, sub_y);
            // Always update to the newest point that touches this cell
            color_index[grid_idx] = i;
        }

        // Render the braille grid to the terminal buffer
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
                if px < area.width && py < area.height {
                    frame.buffer_mut().cell_mut((px, py)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(Style::default().fg(color));
                    });
                }
            }
        }
    }

    fn handle_event(&mut self, _event: &Event) -> EventResponse {
        EventResponse::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_lissajous() -> Lissajous {
        let config = LissajousConfig::default();
        let theme = Theme::default_theme();
        Lissajous::new(&config, theme)
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
        // Simulate some updates
        for _ in 0..10 {
            liss.update(Duration::from_millis(33));
        }
        assert!(!liss.trail.is_empty());

        // Resize should reset state
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

        // After rebirth, at least one parameter should differ (probabilistically).
        // With random generation, it's astronomically unlikely all three match.
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
        config.morph_interval_secs = 0.1; // Very short interval for testing
        let theme = Theme::default_theme();
        let mut liss = Lissajous::new(&config, theme);
        liss.init(80, 24);

        // Update until morph triggers
        for _ in 0..10 {
            liss.update(Duration::from_millis(20));
        }
        // After 200ms total, with 0.1s morph interval, should have morphed
        // t should have been reset at least once (and restarted from 0)
        // The morph_timer should be less than morph_interval_secs
        assert!(liss.morph_timer < liss.morph_interval_secs + 0.05);
    }

    #[test]
    fn trail_length_is_bounded() {
        let mut config = LissajousConfig::default();
        config.trail_length = 100; // Small for testing
        let theme = Theme::default_theme();
        let mut liss = Lissajous::new(&config, theme);
        liss.init(80, 24);

        // Run many updates to fill trail beyond capacity
        for _ in 0..200 {
            liss.update(Duration::from_millis(33));
        }
        assert!(liss.trail.len() <= config.trail_length);
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
        // At t=0, x = A*sin(delta)*e^0 and y = B*sin(0)*e^0 = 0
        let (x, y) = liss.evaluate(0.0);
        assert!(x.abs() <= 1.0, "x should be in [-1,1], got {x}");
        assert!(y.abs() <= 1e-10, "y should be ~0 at t=0, got {y}");
    }

    #[test]
    fn evaluate_decays_over_time() {
        let liss = make_lissajous();
        // At a very large t, the exponential decay should make values tiny
        let (x, y) = liss.evaluate(10000.0);
        assert!(x.abs() < 0.01, "x should have decayed, got {x}");
        assert!(y.abs() < 0.01, "y should have decayed, got {y}");
    }

    #[test]
    fn decay_detection_works() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        assert!(!liss.is_decayed(), "Should not be decayed at t=0");

        // Fast-forward time to make it decay
        liss.t = 100000.0;
        assert!(liss.is_decayed(), "Should be decayed at very large t");
    }

    #[test]
    fn to_dot_space_center() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        let (dx, dy) = liss.to_dot_space(0.0, 0.0);
        // Center should be at (dot_width/2, dot_height/2)
        assert!((dx - 80.0).abs() < 0.01);
        assert!((dy - 48.0).abs() < 0.01);
    }

    #[test]
    fn to_dot_space_edges() {
        let mut liss = make_lissajous();
        liss.init(80, 24);
        let (dx, _) = liss.to_dot_space(1.0, 0.0);
        // Should be near the right edge (within margin)
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
        // Rainbow
        let color = liss.trail_color(50, 100);
        matches!(color, Color::Rgb(_, _, _));

        // Theme mode
        let mut config = LissajousConfig::default();
        config.color_mode = "theme".into();
        let theme_liss = Lissajous::new(&config, Theme::neon());
        let color = theme_liss.trail_color(50, 100);
        matches!(color, Color::Rgb(_, _, _));

        // Mono mode
        config.color_mode = "mono".into();
        let mono_liss = Lissajous::new(&config, Theme::default_theme());
        let color = mono_liss.trail_color(50, 100);
        matches!(color, Color::Rgb(_, _, _));
    }

    #[test]
    fn color_newest_is_brightest() {
        // Use mono mode where brightness is directly proportional to age
        let mut config = LissajousConfig::default();
        config.color_mode = "mono".into();
        let liss = Lissajous::new(&config, Theme::default_theme());
        // Index 99 out of 100 is the newest point (age_ratio closest to 1.0)
        let new_color = liss.trail_color(99, 100);
        let old_color = liss.trail_color(0, 100);
        // Extract RGB values
        if let (Color::Rgb(nr, ng, nb), Color::Rgb(or, og, ob)) = (new_color, old_color) {
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
        assert_eq!(liss.name(), "lissajous");
        assert!(!liss.description().is_empty());
    }

    #[test]
    fn handle_event_returns_ignored() {
        let mut liss = make_lissajous();
        let event = Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(liss.handle_event(&event), EventResponse::Ignored);
    }

    #[test]
    fn random_params_produces_valid_ranges() {
        for _ in 0..50 {
            let p = Lissajous::random_params(0.002);
            // a and b should be near small integers (1..=5) with small drift
            assert!(p.a > 0.5 && p.a < 5.5, "a out of range: {}", p.a);
            assert!(p.b > 0.5 && p.b < 5.5, "b out of range: {}", p.b);
            // delta should be in [0, TAU)
            assert!(p.delta >= 0.0 && p.delta < std::f64::consts::TAU);
            // damping values should be positive
            assert!(p.d1 > 0.0);
            assert!(p.d2 > 0.0);
            // amplitudes should be 0.9
            assert!((p.amplitude_x - 0.9).abs() < 1e-10);
            assert!((p.amplitude_y - 0.9).abs() < 1e-10);
        }
    }
}
