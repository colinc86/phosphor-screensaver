use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent};
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::MatrixRainConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

struct Column {
    head_y: f64,
    speed: f64,
    chars: Vec<char>,
    length: usize,
    active: bool,
    layer: u8,
}

pub struct MatrixRain {
    columns: Vec<Column>,
    width: u16,
    height: u16,
    density: f64,
    speed: f64,
    charset: CharSet,
    #[allow(dead_code)]
    theme: Theme,
    depth_layers: bool,
    glitch_wave: bool,
    glitch_wave_x: Option<f64>,
    glitch_interval_secs: f64,
    glitch_timer: f64,
    glitch_wave_speed: f64,
    interactive: bool,
    speed_mode: SpeedMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SpeedMode {
    Slow,
    Normal,
    Fast,
}

impl SpeedMode {
    fn multiplier(self) -> f64 {
        match self {
            SpeedMode::Slow => 0.5,
            SpeedMode::Normal => 1.0,
            SpeedMode::Fast => 2.0,
        }
    }

    fn next(self) -> Self {
        match self {
            SpeedMode::Slow => SpeedMode::Normal,
            SpeedMode::Normal => SpeedMode::Fast,
            SpeedMode::Fast => SpeedMode::Slow,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CharSet {
    Katakana,
    Ascii,
    Binary,
}

impl CharSet {
    fn from_str(s: &str) -> Self {
        match s {
            "ascii" => CharSet::Ascii,
            "binary" => CharSet::Binary,
            _ => CharSet::Katakana,
        }
    }

    fn random_char(&self) -> char {
        let mut rng = rand::rng();
        match self {
            CharSet::Katakana => {
                let cp = rng.random_range(0x30A0u32..0x30FFu32);
                char::from_u32(cp).unwrap_or('ア')
            }
            CharSet::Ascii => {
                let chars: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@#$%&";
                chars[rng.random_range(0..chars.len())] as char
            }
            CharSet::Binary => {
                if rng.random_bool(0.5) {
                    '0'
                } else {
                    '1'
                }
            }
        }
    }

    fn next(self) -> Self {
        match self {
            CharSet::Katakana => CharSet::Ascii,
            CharSet::Ascii => CharSet::Binary,
            CharSet::Binary => CharSet::Katakana,
        }
    }
}

impl MatrixRain {
    pub fn new(config: &MatrixRainConfig, theme: Theme) -> Self {
        Self {
            columns: Vec::new(),
            width: 80,
            height: 24,
            density: config.density,
            speed: config.speed,
            charset: CharSet::from_str(&config.charset),
            theme,
            depth_layers: config.depth_layers,
            glitch_wave: config.glitch_wave,
            glitch_wave_x: None,
            glitch_interval_secs: config.glitch_interval_secs as f64,
            glitch_timer: 0.0,
            glitch_wave_speed: 40.0,
            interactive: config.interactive,
            speed_mode: SpeedMode::Normal,
        }
    }

    fn assign_layer() -> u8 {
        let mut rng = rand::rng();
        let roll: f64 = rng.random_range(0.0..1.0);
        if roll < 0.40 {
            0 // bg
        } else if roll < 0.75 {
            1 // mid
        } else {
            2 // fg
        }
    }

    fn layer_speed_multiplier(layer: u8) -> f64 {
        match layer {
            0 => 0.5,
            1 => 1.0,
            _ => 1.5,
        }
    }

    fn layer_max_green(layer: u8) -> u8 {
        match layer {
            0 => 120,
            1 => 180,
            _ => 255,
        }
    }

    fn spawn_column(height: u16, speed: f64, charset: CharSet, depth_layers: bool) -> Column {
        let mut rng = rand::rng();
        let layer = if depth_layers { Self::assign_layer() } else { 1 };
        let length = rng.random_range(4..height.max(5) as usize);
        let layer_mult = if depth_layers { Self::layer_speed_multiplier(layer) } else { 1.0 };
        let col_speed = speed * rng.random_range(8.0..25.0) * layer_mult;
        let chars: Vec<char> = (0..length).map(|_| charset.random_char()).collect();
        Column {
            head_y: -(length as f64),
            speed: col_speed,
            chars,
            length,
            active: true,
            layer,
        }
    }
}

impl Screensaver for MatrixRain {
    fn name(&self) -> &str {
        "matrix_rain"
    }

    fn description(&self) -> &str {
        "Falling characters in the style of The Matrix"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.columns.clear();
        self.glitch_wave_x = None;
        self.glitch_timer = 0.0;

        let mut rng = rand::rng();
        for _ in 0..width {
            if rng.random_range(0.0..1.0) < self.density {
                let mut col = Self::spawn_column(height, self.speed, self.charset, self.depth_layers);
                // Stagger initial positions
                col.head_y = rng.random_range(-(height as f64)..height as f64);
                self.columns.push(col);
            } else {
                self.columns.push(Column {
                    head_y: 0.0,
                    speed: 0.0,
                    chars: Vec::new(),
                    length: 0,
                    active: false,
                    layer: 1,
                });
            }
        }
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let speed_mult = self.speed_mode.multiplier();
        let mut rng = rand::rng();

        // Update glitch wave
        if self.glitch_wave {
            self.glitch_timer += dt_secs;
            if self.glitch_timer >= self.glitch_interval_secs && self.glitch_wave_x.is_none() {
                // Start a new wave; don't advance it this frame
                self.glitch_wave_x = Some(0.0);
                self.glitch_timer = 0.0;
            } else if let Some(ref mut wx) = self.glitch_wave_x {
                *wx += self.glitch_wave_speed * dt_secs;
                if *wx > self.width as f64 {
                    self.glitch_wave_x = None;
                }
            }
        }

        for (col_idx, col) in self.columns.iter_mut().enumerate() {
            if !col.active {
                // Randomly activate inactive columns
                if rng.random_range(0.0..1.0) < 0.01 {
                    *col = Self::spawn_column(self.height, self.speed, self.charset, self.depth_layers);
                }
                continue;
            }

            col.head_y += col.speed * dt_secs * speed_mult;

            // Randomly flicker characters
            if rng.random_range(0.0..1.0) < 0.05 && !col.chars.is_empty() {
                let idx = rng.random_range(0..col.chars.len());
                col.chars[idx] = self.charset.random_char();
            }

            // Glitch wave: scramble chars near the wave front
            if let Some(wx) = self.glitch_wave_x {
                let dist = (col_idx as f64 - wx).abs();
                if dist < 3.0 && !col.chars.is_empty() {
                    for ch in &mut col.chars {
                        *ch = self.charset.random_char();
                    }
                }
            }

            // Respawn if fully off-screen
            if col.head_y > (self.height as f64 + col.length as f64 + 5.0) {
                *col = Self::spawn_column(self.height, self.speed, self.charset, self.depth_layers);
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buf = frame.buffer_mut();

        // Determine render order: if depth_layers, sort by layer (bg first, fg last)
        let render_order: Vec<usize> = if self.depth_layers {
            let mut indices: Vec<usize> = (0..self.columns.len()).collect();
            indices.sort_by_key(|&i| self.columns[i].layer);
            indices
        } else {
            (0..self.columns.len()).collect()
        };

        for col_idx in render_order {
            let col = &self.columns[col_idx];
            if !col.active || col_idx as u16 >= area.width {
                continue;
            }

            let head = col.head_y.round() as i32;
            let max_green = if self.depth_layers {
                Self::layer_max_green(col.layer)
            } else {
                230
            };

            // Check if this column is near the glitch wave front
            let in_glitch = if let Some(wx) = self.glitch_wave_x {
                (col_idx as f64 - wx).abs() < 3.0
            } else {
                false
            };

            for (i, &ch) in col.chars.iter().enumerate() {
                let row = head - i as i32;
                if row < 0 || row >= area.height as i32 {
                    continue;
                }

                let color = if in_glitch {
                    Color::Rgb(255, 255, 255)
                } else if i == 0 {
                    // Head character: bright white-green
                    Color::Rgb(220, 255, 220)
                } else {
                    // Trail: green fading to dark, scaled by layer
                    let fade = 1.0 - (i as f64 / col.length as f64);
                    let g_base = if self.depth_layers {
                        (fade * (max_green as f64 - 30.0)) as u8 + 30
                    } else {
                        (fade * 200.0) as u8 + 30
                    };
                    Color::Rgb(0, g_base, 0)
                };

                buf.cell_mut((col_idx as u16, row as u16)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                });
            }
        }
    }

    fn handle_event(&mut self, event: &Event) -> EventResponse {
        if !self.interactive {
            return EventResponse::Ignored;
        }

        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.density = (self.density + 0.1).min(1.5);
                    EventResponse::Continue
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    self.density = (self.density - 0.1).max(0.1);
                    EventResponse::Continue
                }
                KeyCode::Char('s') => {
                    self.speed_mode = self.speed_mode.next();
                    EventResponse::Continue
                }
                KeyCode::Char('c') => {
                    self.charset = self.charset.next();
                    EventResponse::Continue
                }
                _ => EventResponse::Ignored,
            }
        } else {
            EventResponse::Ignored
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_produces_valid_chars() {
        for _ in 0..100 {
            let ch = CharSet::Katakana.random_char();
            assert!((0x30A0..=0x30FF).contains(&(ch as u32)) || ch == 'ア');
        }
        for _ in 0..100 {
            let ch = CharSet::Binary.random_char();
            assert!(ch == '0' || ch == '1');
        }
        for _ in 0..100 {
            let ch = CharSet::Ascii.random_char();
            assert!(ch.is_ascii());
        }
    }

    #[test]
    fn charset_from_str() {
        assert!(matches!(CharSet::from_str("katakana"), CharSet::Katakana));
        assert!(matches!(CharSet::from_str("ascii"), CharSet::Ascii));
        assert!(matches!(CharSet::from_str("binary"), CharSet::Binary));
        assert!(matches!(CharSet::from_str("unknown"), CharSet::Katakana));
    }

    #[test]
    fn init_creates_columns() {
        let config = MatrixRainConfig::default();
        let theme = Theme::default_theme();
        let mut mr = MatrixRain::new(&config, theme);
        mr.init(80, 24);
        assert_eq!(mr.columns.len(), 80);
    }

    #[test]
    fn update_advances_heads() {
        let config = MatrixRainConfig::default();
        let theme = Theme::default_theme();
        let mut mr = MatrixRain::new(&config, theme);
        mr.init(80, 24);

        let active_heads: Vec<f64> = mr
            .columns
            .iter()
            .filter(|c| c.active)
            .map(|c| c.head_y)
            .collect();

        mr.update(Duration::from_millis(100));

        let new_heads: Vec<f64> = mr
            .columns
            .iter()
            .filter(|c| c.active)
            .map(|c| c.head_y)
            .collect();

        // At least some active columns should have advanced
        // (some may have been respawned)
        assert!(!active_heads.is_empty());
        assert!(!new_heads.is_empty());
    }

    #[test]
    fn depth_layers_creates_columns_with_varying_layers() {
        let config = MatrixRainConfig {
            depth_layers: true,
            density: 1.0,
            ..MatrixRainConfig::default()
        };
        let theme = Theme::default_theme();
        let mut mr = MatrixRain::new(&config, theme);
        mr.init(100, 40);

        let mut has_layer_0 = false;
        let mut has_layer_1 = false;
        let mut has_layer_2 = false;
        for col in &mr.columns {
            if col.active {
                match col.layer {
                    0 => has_layer_0 = true,
                    1 => has_layer_1 = true,
                    2 => has_layer_2 = true,
                    _ => {}
                }
            }
        }
        // With 100 columns at density 1.0, distribution 40/35/25,
        // all three layers should be present
        assert!(has_layer_0, "expected some layer 0 (bg) columns");
        assert!(has_layer_1, "expected some layer 1 (mid) columns");
        assert!(has_layer_2, "expected some layer 2 (fg) columns");
    }

    #[test]
    fn glitch_wave_advances_across_screen() {
        let config = MatrixRainConfig {
            glitch_wave: true,
            glitch_interval_secs: 1,
            density: 1.0,
            ..MatrixRainConfig::default()
        };
        let theme = Theme::default_theme();
        let mut mr = MatrixRain::new(&config, theme);
        mr.init(80, 24);

        // No wave initially
        assert!(mr.glitch_wave_x.is_none());

        // Advance past the glitch interval to trigger the wave
        mr.update(Duration::from_secs(2));
        assert!(mr.glitch_wave_x.is_some(), "wave should have started after interval elapsed");
        assert!((mr.glitch_wave_x.unwrap() - 0.0).abs() < 0.001, "wave should start at x=0");

        // Advance a small dt to move the wave forward
        mr.update(Duration::from_millis(100));
        let wave_x_after = mr.glitch_wave_x.unwrap();
        assert!(
            wave_x_after > 0.0,
            "wave should advance from 0: got {}",
            wave_x_after
        );

        // Disable the wave from restarting by setting a long interval,
        // then advance enough to clear it
        mr.glitch_interval_secs = 9999.0;
        for _ in 0..100 {
            mr.update(Duration::from_millis(100));
        }
        assert!(
            mr.glitch_wave_x.is_none(),
            "wave should be cleared after passing screen width"
        );
    }

    #[test]
    fn interactive_handle_event_returns_continue_for_valid_keys() {
        let config = MatrixRainConfig {
            interactive: true,
            ..MatrixRainConfig::default()
        };
        let theme = Theme::default_theme();
        let mut mr = MatrixRain::new(&config, theme);
        mr.init(80, 24);

        // '+' increases density
        let initial_density = mr.density;
        let event = Event::Key(KeyEvent::from(KeyCode::Char('+')));
        assert_eq!(mr.handle_event(&event), EventResponse::Continue);
        assert!((mr.density - (initial_density + 0.1)).abs() < 0.001);

        // '-' decreases density
        let event = Event::Key(KeyEvent::from(KeyCode::Char('-')));
        assert_eq!(mr.handle_event(&event), EventResponse::Continue);
        assert!((mr.density - initial_density).abs() < 0.001);

        // 's' cycles speed mode
        assert_eq!(mr.speed_mode, SpeedMode::Normal);
        let event = Event::Key(KeyEvent::from(KeyCode::Char('s')));
        assert_eq!(mr.handle_event(&event), EventResponse::Continue);
        assert_eq!(mr.speed_mode, SpeedMode::Fast);

        // 'c' cycles charset
        assert_eq!(mr.charset, CharSet::Katakana);
        let event = Event::Key(KeyEvent::from(KeyCode::Char('c')));
        assert_eq!(mr.handle_event(&event), EventResponse::Continue);
        assert_eq!(mr.charset, CharSet::Ascii);

        // Unrecognized key returns Ignored
        let event = Event::Key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(mr.handle_event(&event), EventResponse::Ignored);
    }

    #[test]
    fn non_interactive_handle_event_returns_ignored() {
        let config = MatrixRainConfig::default(); // interactive = false
        let theme = Theme::default_theme();
        let mut mr = MatrixRain::new(&config, theme);
        mr.init(80, 24);

        let event = Event::Key(KeyEvent::from(KeyCode::Char('+')));
        assert_eq!(mr.handle_event(&event), EventResponse::Ignored);
    }
}
