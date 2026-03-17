use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use termsaver_plugin::{
    Cell, ConfigField, EventResponse, FieldKind, KeyInput, PluginBuffer, PluginColor,
    PluginInfo, ScreensaverPlugin,
};
use termsaver_plugin::protocol::KeyCode;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct MatrixRainConfig {
    density: f64,
    speed: f64,
    charset: String,
    depth_layers: bool,
    glitch_wave: bool,
    glitch_interval_secs: u64,
    interactive: bool,
}

impl Default for MatrixRainConfig {
    fn default() -> Self {
        Self {
            density: 0.8,
            speed: 1.0,
            charset: "katakana".into(),
            depth_layers: false,
            glitch_wave: false,
            glitch_interval_secs: 25,
            interactive: false,
        }
    }
}

struct Column {
    head_y: f64,
    speed: f64,
    chars: Vec<char>,
    length: usize,
    active: bool,
    layer: u8,
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
                char::from_u32(cp).unwrap_or('\u{30A2}')
            }
            CharSet::Ascii => {
                let chars: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@#$%&";
                chars[rng.random_range(0..chars.len())] as char
            }
            CharSet::Binary => {
                if rng.random_bool(0.5) { '0' } else { '1' }
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

struct MatrixRain {
    columns: Vec<Column>,
    width: u16,
    height: u16,
    density: f64,
    speed: f64,
    charset: CharSet,
    depth_layers: bool,
    glitch_wave: bool,
    glitch_wave_x: Option<f64>,
    glitch_interval_secs: f64,
    glitch_timer: f64,
    glitch_wave_speed: f64,
    interactive: bool,
    speed_mode: SpeedMode,
}

impl MatrixRain {
    fn new(config: MatrixRainConfig) -> Self {
        Self {
            columns: Vec::new(),
            width: 80,
            height: 24,
            density: config.density,
            speed: config.speed,
            charset: CharSet::from_str(&config.charset),
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
            0
        } else if roll < 0.75 {
            1
        } else {
            2
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

impl ScreensaverPlugin for MatrixRain {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "matrix_rain".into(),
            description: "Falling characters in the style of The Matrix".into(),
            key_bindings: vec![
                ("+/-".into(), "Increase/decrease density".into()),
                ("s".into(), "Cycle speed mode".into()),
                ("c".into(), "Cycle charset".into()),
            ],
            features: vec![
                "Multiple character sets: katakana, ascii, binary".into(),
                "Depth layers with parallax".into(),
                "Glitch wave effect".into(),
                "Interactive controls".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "density".into(),
                    label: "Density".into(),
                    kind: FieldKind::F64,
                    default: "0.8".into(),
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
                    key: "charset".into(),
                    label: "Charset".into(),
                    kind: FieldKind::Text,
                    default: "katakana".into(),
                    choices: Some(vec!["katakana".into(), "ascii".into(), "binary".into()]),
                },
                ConfigField {
                    key: "depth_layers".into(),
                    label: "Depth Layers".into(),
                    kind: FieldKind::Bool,
                    default: "false".into(),
                    choices: None,
                },
                ConfigField {
                    key: "glitch_wave".into(),
                    label: "Glitch Wave".into(),
                    kind: FieldKind::Bool,
                    default: "false".into(),
                    choices: None,
                },
                ConfigField {
                    key: "glitch_interval_secs".into(),
                    label: "Glitch Interval (s)".into(),
                    kind: FieldKind::U64,
                    default: "25".into(),
                    choices: None,
                },
                ConfigField {
                    key: "interactive".into(),
                    label: "Interactive".into(),
                    kind: FieldKind::Bool,
                    default: "false".into(),
                    choices: None,
                },
            ],
        }
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

        if self.glitch_wave {
            self.glitch_timer += dt_secs;
            if self.glitch_timer >= self.glitch_interval_secs && self.glitch_wave_x.is_none() {
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
                if rng.random_range(0.0..1.0) < 0.01 {
                    *col = Self::spawn_column(self.height, self.speed, self.charset, self.depth_layers);
                }
                continue;
            }

            col.head_y += col.speed * dt_secs * speed_mult;

            if rng.random_range(0.0..1.0) < 0.05 && !col.chars.is_empty() {
                let idx = rng.random_range(0..col.chars.len());
                col.chars[idx] = self.charset.random_char();
            }

            if let Some(wx) = self.glitch_wave_x {
                let dist = (col_idx as f64 - wx).abs();
                if dist < 3.0 && !col.chars.is_empty() {
                    for ch in &mut col.chars {
                        *ch = self.charset.random_char();
                    }
                }
            }

            if col.head_y > (self.height as f64 + col.length as f64 + 5.0) {
                *col = Self::spawn_column(self.height, self.speed, self.charset, self.depth_layers);
            }
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let bw = buf.width();
        let bh = buf.height();

        let render_order: Vec<usize> = if self.depth_layers {
            let mut indices: Vec<usize> = (0..self.columns.len()).collect();
            indices.sort_by_key(|&i| self.columns[i].layer);
            indices
        } else {
            (0..self.columns.len()).collect()
        };

        for col_idx in render_order {
            let col = &self.columns[col_idx];
            if !col.active || col_idx as u16 >= bw {
                continue;
            }

            let head = col.head_y.round() as i32;
            let max_green = if self.depth_layers {
                Self::layer_max_green(col.layer)
            } else {
                230
            };

            let in_glitch = if let Some(wx) = self.glitch_wave_x {
                (col_idx as f64 - wx).abs() < 3.0
            } else {
                false
            };

            for (i, &ch) in col.chars.iter().enumerate() {
                let row = head - i as i32;
                if row < 0 || row >= bh as i32 {
                    continue;
                }

                let color = if in_glitch {
                    PluginColor::Rgb(255, 255, 255)
                } else if i == 0 {
                    PluginColor::Rgb(220, 255, 220)
                } else {
                    let fade = 1.0 - (i as f64 / col.length as f64);
                    let g_base = if self.depth_layers {
                        (fade * (max_green as f64 - 30.0)) as u8 + 30
                    } else {
                        (fade * 200.0) as u8 + 30
                    };
                    PluginColor::Rgb(0, g_base, 0)
                };

                buf.set(col_idx as u16, row as u16, Cell { ch, fg: color, bg: PluginColor::Reset });
            }
        }
    }

    fn handle_key(&mut self, key: KeyInput) -> EventResponse {
        if !self.interactive {
            return EventResponse::Ignored;
        }

        match key.code {
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
    }
}

fn main() {
    let config = MatrixRainConfig::default();
    termsaver_plugin::run(MatrixRain::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_produces_valid_chars() {
        for _ in 0..100 {
            let ch = CharSet::Katakana.random_char();
            assert!((0x30A0..=0x30FF).contains(&(ch as u32)) || ch == '\u{30A2}');
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
        let mut mr = MatrixRain::new(config);
        mr.init(80, 24);
        assert_eq!(mr.columns.len(), 80);
    }

    #[test]
    fn update_advances_heads() {
        let config = MatrixRainConfig::default();
        let mut mr = MatrixRain::new(config);
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

        assert!(!active_heads.is_empty());
        assert!(!new_heads.is_empty());
    }

    #[test]
    fn depth_layers_creates_columns_with_varying_layers() {
        let mut config = MatrixRainConfig::default();
        config.depth_layers = true;
        config.density = 1.0;
        let mut mr = MatrixRain::new(config);
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
        assert!(has_layer_0, "expected some layer 0 (bg) columns");
        assert!(has_layer_1, "expected some layer 1 (mid) columns");
        assert!(has_layer_2, "expected some layer 2 (fg) columns");
    }

    #[test]
    fn glitch_wave_advances_across_screen() {
        let mut config = MatrixRainConfig::default();
        config.glitch_wave = true;
        config.glitch_interval_secs = 1;
        config.density = 1.0;
        let mut mr = MatrixRain::new(config);
        mr.init(80, 24);

        assert!(mr.glitch_wave_x.is_none());

        mr.update(Duration::from_secs(2));
        assert!(mr.glitch_wave_x.is_some(), "wave should have started after interval elapsed");
        assert!((mr.glitch_wave_x.unwrap() - 0.0).abs() < 0.001, "wave should start at x=0");

        mr.update(Duration::from_millis(100));
        let wave_x_after = mr.glitch_wave_x.unwrap();
        assert!(
            wave_x_after > 0.0,
            "wave should advance from 0: got {}",
            wave_x_after
        );

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
    fn interactive_handle_key_returns_continue_for_valid_keys() {
        let mut config = MatrixRainConfig::default();
        config.interactive = true;
        let mut mr = MatrixRain::new(config);
        mr.init(80, 24);

        let initial_density = mr.density;
        let key = KeyInput { code: KeyCode::Char('+'), modifiers: 0 };
        assert_eq!(mr.handle_key(key), EventResponse::Continue);
        assert!((mr.density - (initial_density + 0.1)).abs() < 0.001);

        let key = KeyInput { code: KeyCode::Char('-'), modifiers: 0 };
        assert_eq!(mr.handle_key(key), EventResponse::Continue);
        assert!((mr.density - initial_density).abs() < 0.001);

        assert_eq!(mr.speed_mode, SpeedMode::Normal);
        let key = KeyInput { code: KeyCode::Char('s'), modifiers: 0 };
        assert_eq!(mr.handle_key(key), EventResponse::Continue);
        assert_eq!(mr.speed_mode, SpeedMode::Fast);

        assert_eq!(mr.charset, CharSet::Katakana);
        let key = KeyInput { code: KeyCode::Char('c'), modifiers: 0 };
        assert_eq!(mr.handle_key(key), EventResponse::Continue);
        assert_eq!(mr.charset, CharSet::Ascii);

        let key = KeyInput { code: KeyCode::Char('z'), modifiers: 0 };
        assert_eq!(mr.handle_key(key), EventResponse::Ignored);
    }

    #[test]
    fn non_interactive_handle_key_returns_ignored() {
        let config = MatrixRainConfig::default(); // interactive = false
        let mut mr = MatrixRain::new(config);
        mr.init(80, 24);

        let key = KeyInput { code: KeyCode::Char('+'), modifiers: 0 };
        assert_eq!(mr.handle_key(key), EventResponse::Ignored);
    }

    #[test]
    fn name_via_info() {
        let mr = MatrixRain::new(MatrixRainConfig::default());
        assert_eq!(mr.info().name, "matrix_rain");
    }

    #[test]
    fn render_does_not_panic() {
        let config = MatrixRainConfig::default();
        let mut mr = MatrixRain::new(config);
        mr.init(80, 24);
        mr.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(80, 24);
        mr.render(&mut buf);
    }
}
