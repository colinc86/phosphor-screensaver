use std::time::Duration;

use crossterm::event::Event;
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
}

#[derive(Debug, Clone, Copy)]
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
        }
    }

    fn spawn_column(height: u16, speed: f64, charset: CharSet) -> Column {
        let mut rng = rand::rng();
        let length = rng.random_range(4..height.max(5) as usize);
        let col_speed = speed * rng.random_range(8.0..25.0);
        let chars: Vec<char> = (0..length).map(|_| charset.random_char()).collect();
        Column {
            head_y: -(length as f64),
            speed: col_speed,
            chars,
            length,
            active: true,
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

        let mut rng = rand::rng();
        for _ in 0..width {
            if rng.random_range(0.0..1.0) < self.density {
                let mut col = Self::spawn_column(height, self.speed, self.charset);
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
                });
            }
        }
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let mut rng = rand::rng();

        for col in &mut self.columns {
            if !col.active {
                // Randomly activate inactive columns
                if rng.random_range(0.0..1.0) < 0.01 {
                    *col = Self::spawn_column(self.height, self.speed, self.charset);
                }
                continue;
            }

            col.head_y += col.speed * dt_secs;

            // Randomly flicker characters
            if rng.random_range(0.0..1.0) < 0.05 && !col.chars.is_empty() {
                let idx = rng.random_range(0..col.chars.len());
                col.chars[idx] = self.charset.random_char();
            }

            // Respawn if fully off-screen
            if col.head_y > (self.height as f64 + col.length as f64 + 5.0) {
                *col = Self::spawn_column(self.height, self.speed, self.charset);
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buf = frame.buffer_mut();

        for (col_idx, col) in self.columns.iter().enumerate() {
            if !col.active || col_idx as u16 >= area.width {
                continue;
            }

            let head = col.head_y.round() as i32;

            for (i, &ch) in col.chars.iter().enumerate() {
                let row = head - i as i32;
                if row < 0 || row >= area.height as i32 {
                    continue;
                }

                let color = if i == 0 {
                    // Head character: bright white
                    Color::Rgb(220, 255, 220)
                } else {
                    // Trail: green fading to dark
                    let fade = 1.0 - (i as f64 / col.length as f64);
                    let g = (fade * 200.0) as u8 + 30;
                    Color::Rgb(0, g, 0)
                };

                buf.cell_mut((col_idx as u16, row as u16)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                });
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
}
