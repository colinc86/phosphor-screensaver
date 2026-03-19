use std::time::Duration;

use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct DigitalClockConfig {
    style: String,
    show_date: bool,
    color: String,
    color_cycle: bool,
    color_cycle_speed: f64,
    mode: String,
    countdown_minutes: u64,
}

impl Default for DigitalClockConfig {
    fn default() -> Self {
        Self {
            style: "block".into(),
            show_date: true,
            color: "green".into(),
            color_cycle: false,
            color_cycle_speed: 1.0,
            mode: "clock".into(),
            countdown_minutes: 25,
        }
    }
}

/// 5-line tall block font for digits 0-9 and colon.
const DIGITS: [[&str; 5]; 11] = [
    // 0
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 1
    [
        "  \u{2588}  ",
        " \u{2588}\u{2588}  ",
        "  \u{2588}  ",
        "  \u{2588}  ",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 2
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}    ",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 3
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 4
    [
        "\u{2588}   \u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "    \u{2588}",
    ],
    // 5
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}    ",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 6
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}    ",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 7
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "   \u{2588} ",
        "  \u{2588}  ",
        "  \u{2588}  ",
    ],
    // 8
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // 9
    [
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "\u{2588}   \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        "    \u{2588}",
        "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    ],
    // : (colon) - index 10
    ["     ", "  \u{2588}  ", "     ", "  \u{2588}  ", "     "],
];

/// 5-line tall 7-segment style font for digits 0-9 and colon.
const DIGITS_SEGMENT: [[&str; 5]; 11] = [
    // 0
    [
        " \u{2584}\u{2584}\u{2584} ",
        "\u{2502}   \u{2502}",
        "     ",
        "\u{2502}   \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // 1
    ["     ", "    \u{2502}", "     ", "    \u{2502}", "     "],
    // 2
    [
        " \u{2584}\u{2584}\u{2584} ",
        "    \u{2502}",
        " \u{2584}\u{2584}\u{2584} ",
        "\u{2502}    ",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // 3
    [
        " \u{2584}\u{2584}\u{2584} ",
        "    \u{2502}",
        " \u{2584}\u{2584}\u{2584} ",
        "    \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // 4
    [
        "     ",
        "\u{2502}   \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
        "    \u{2502}",
        "     ",
    ],
    // 5
    [
        " \u{2584}\u{2584}\u{2584} ",
        "\u{2502}    ",
        " \u{2580}\u{2580}\u{2580} ",
        "    \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // 6
    [
        " \u{2584}\u{2584}\u{2584} ",
        "\u{2502}    ",
        " \u{2580}\u{2580}\u{2580} ",
        "\u{2502}   \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // 7
    [
        " \u{2584}\u{2584}\u{2584} ",
        "    \u{2502}",
        "     ",
        "    \u{2502}",
        "     ",
    ],
    // 8
    [
        " \u{2584}\u{2584}\u{2584} ",
        "\u{2502}   \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
        "\u{2502}   \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // 9
    [
        " \u{2584}\u{2584}\u{2584} ",
        "\u{2502}   \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
        "    \u{2502}",
        " \u{2580}\u{2580}\u{2580} ",
    ],
    // : (colon) - index 10
    ["     ", "  \u{25CF}  ", "     ", "  \u{25CF}  ", "     "],
];

/// Width of a single digit in characters.
const DIGIT_WIDTH: u16 = 5;
/// Space between digits.
const DIGIT_GAP: u16 = 2;
/// Height of digit font.
const DIGIT_HEIGHT: u16 = 5;

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

fn parse_color(name: &str) -> PluginColor {
    match name {
        "red" => PluginColor::Rgb(255, 0, 0),
        "green" => PluginColor::Rgb(0, 255, 0),
        "blue" => PluginColor::Rgb(0, 0, 255),
        "yellow" => PluginColor::Rgb(255, 255, 0),
        "cyan" => PluginColor::Rgb(0, 255, 255),
        "magenta" => PluginColor::Rgb(255, 0, 255),
        "white" => PluginColor::Rgb(255, 255, 255),
        _ => PluginColor::Rgb(0, 255, 0), // default green
    }
}

/// Map a character to its digit font index.
fn char_to_digit_index(c: char) -> Option<usize> {
    match c {
        '0'..='9' => Some((c as u8 - b'0') as usize),
        ':' => Some(10),
        _ => None,
    }
}

struct DigitalClock {
    width: u16,
    height: u16,
    time_str: String,
    date_str: String,
    colon_visible: bool,
    blink_accum: f64,
    hue: f64,
    // Timer modes
    stopwatch_elapsed: f64,
    countdown_remaining: f64,
    countdown_flash_accum: f64,
    countdown_flash_on: bool,
    color: PluginColor,
    config: DigitalClockConfig,
}

impl DigitalClock {
    fn new(config: DigitalClockConfig) -> Self {
        let color = parse_color(&config.color);
        let countdown_remaining = config.countdown_minutes as f64 * 60.0;
        Self {
            width: 80,
            height: 24,
            time_str: String::new(),
            date_str: String::new(),
            colon_visible: true,
            blink_accum: 0.0,
            hue: 0.0,
            stopwatch_elapsed: 0.0,
            countdown_remaining,
            countdown_flash_accum: 0.0,
            countdown_flash_on: false,
            color,
            config,
        }
    }

    /// Calculate total width needed for a time string like "12:34".
    pub fn calculate_width(time_str: &str) -> u16 {
        let char_count = time_str.len() as u16;
        if char_count == 0 {
            return 0;
        }
        char_count * DIGIT_WIDTH + (char_count - 1) * DIGIT_GAP
    }

    /// Get the font array based on the current style.
    fn font(&self) -> &[[&str; 5]; 11] {
        match self.config.style.as_str() {
            "segment" => &DIGITS_SEGMENT,
            _ => &DIGITS,
        }
    }

    /// Format seconds into HH:MM:SS string.
    fn format_duration(total_secs: f64) -> String {
        let total_secs = total_secs.max(0.0) as u64;
        let h = total_secs / 3600;
        let m = (total_secs % 3600) / 60;
        let s = total_secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }

    /// Get the effective display color, accounting for color cycling and countdown flash.
    fn effective_color(&self) -> PluginColor {
        // Countdown finished: flash between configured color and red
        if self.config.mode == "countdown"
            && self.countdown_remaining <= 0.0
            && self.countdown_flash_on
        {
            return PluginColor::Rgb(255, 0, 0);
        }

        if self.config.color_cycle {
            let (r, g, b) = hsv_to_rgb(self.hue, 1.0, 1.0);
            PluginColor::Rgb(r, g, b)
        } else {
            self.color
        }
    }
}

impl ScreensaverPlugin for DigitalClock {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "digital_clock".into(),
            description: "A large ASCII-art digital clock display".into(),
            interactive: false,
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Block and segment font styles".into(),
                "Color cycling".into(),
                "Clock, stopwatch, and countdown modes".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "style".into(),
                    label: "Style".into(),
                    kind: FieldKind::Text,
                    default: "block".into(),
                    choices: Some(vec!["block".into(), "thin".into(), "segment".into()]),
                },
                ConfigField {
                    key: "show_date".into(),
                    label: "Show Date".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color".into(),
                    label: "Color".into(),
                    kind: FieldKind::Text,
                    default: "green".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color_cycle".into(),
                    label: "Color Cycle".into(),
                    kind: FieldKind::Bool,
                    default: "false".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color_cycle_speed".into(),
                    label: "Color Cycle Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "mode".into(),
                    label: "Mode".into(),
                    kind: FieldKind::Text,
                    default: "clock".into(),
                    choices: Some(vec!["clock".into(), "stopwatch".into(), "countdown".into()]),
                },
                ConfigField {
                    key: "countdown_minutes".into(),
                    label: "Countdown Minutes".into(),
                    kind: FieldKind::U64,
                    default: "25".into(),
                    choices: None,
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();

        // Update time string based on mode
        match self.config.mode.as_str() {
            "stopwatch" => {
                self.stopwatch_elapsed += dt_secs;
                self.time_str = Self::format_duration(self.stopwatch_elapsed);
                self.date_str = String::new();
            }
            "countdown" => {
                if self.countdown_remaining > 0.0 {
                    self.countdown_remaining = (self.countdown_remaining - dt_secs).max(0.0);
                }
                self.time_str = Self::format_duration(self.countdown_remaining);
                self.date_str = String::new();

                // Flash when countdown reaches zero
                if self.countdown_remaining <= 0.0 {
                    self.countdown_flash_accum += dt_secs;
                    if self.countdown_flash_accum >= 0.5 {
                        self.countdown_flash_accum -= 0.5;
                        self.countdown_flash_on = !self.countdown_flash_on;
                    }
                }
            }
            _ => {
                // "clock" mode (default)
                let now = chrono::Local::now();
                self.time_str = now.format("%H:%M").to_string();
                self.date_str = now.format("%A, %B %e, %Y").to_string();
            }
        }

        // Blink the colon
        self.blink_accum += dt_secs;
        if self.blink_accum >= 0.5 {
            self.blink_accum -= 0.5;
            self.colon_visible = !self.colon_visible;
        }

        // Advance color cycling hue
        if self.config.color_cycle {
            self.hue += dt_secs * self.config.color_cycle_speed * 30.0;
            if self.hue >= 360.0 {
                self.hue -= 360.0;
            }
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let w = buf.width();
        let h = buf.height();

        let total_width = Self::calculate_width(&self.time_str);
        let start_x = w.saturating_sub(total_width) / 2;
        let start_y = h.saturating_sub(DIGIT_HEIGHT) / 2;

        let mut cursor_x = start_x;

        let digit_color = self.effective_color();
        let font = self.font();

        for ch in self.time_str.chars() {
            let Some(idx) = char_to_digit_index(ch) else {
                cursor_x += DIGIT_WIDTH + DIGIT_GAP;
                continue;
            };

            // Skip colon rendering when blinking off
            let is_colon = idx == 10;
            let visible = !is_colon || self.colon_visible;

            let glyph = &font[idx];
            for (row, line) in glyph.iter().enumerate() {
                let y = start_y + row as u16;
                if y >= h {
                    continue;
                }
                for (col, glyph_ch) in line.chars().enumerate() {
                    let x = cursor_x + col as u16;
                    if x >= w {
                        continue;
                    }
                    if glyph_ch != ' ' && visible {
                        buf.set(
                            x,
                            y,
                            Cell {
                                ch: glyph_ch,
                                fg: digit_color,
                                bg: PluginColor::Reset,
                            },
                        );
                    }
                }
            }

            cursor_x += DIGIT_WIDTH + DIGIT_GAP;
        }

        // Draw date below the clock (only in clock mode)
        if self.config.show_date && self.config.mode == "clock" {
            let date_y = start_y + DIGIT_HEIGHT + 2;
            if date_y < h {
                let dim_color = PluginColor::Rgb(100, 100, 100);
                let date_x = w.saturating_sub(self.date_str.len() as u16) / 2;
                for (i, ch) in self.date_str.chars().enumerate() {
                    let x = date_x + i as u16;
                    if x < w {
                        buf.set(
                            x,
                            date_y,
                            Cell {
                                ch,
                                fg: dim_color,
                                bg: PluginColor::Reset,
                            },
                        );
                    }
                }
            }
        }
    }
}

fn main() {
    let config = DigitalClockConfig::default();
    phosphor_plugin::run(DigitalClock::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_to_digit_index_digits() {
        assert_eq!(char_to_digit_index('0'), Some(0));
        assert_eq!(char_to_digit_index('5'), Some(5));
        assert_eq!(char_to_digit_index('9'), Some(9));
        assert_eq!(char_to_digit_index(':'), Some(10));
        assert_eq!(char_to_digit_index('a'), None);
    }

    #[test]
    fn calculate_width_time() {
        // "12:34" = 5 chars, each 5 wide, 4 gaps of 2 = 25 + 8 = 33
        assert_eq!(DigitalClock::calculate_width("12:34"), 33);
    }

    #[test]
    fn calculate_width_empty() {
        assert_eq!(DigitalClock::calculate_width(""), 0);
    }

    #[test]
    fn digit_font_dimensions() {
        for (i, digit) in DIGITS.iter().enumerate() {
            for (row, line) in digit.iter().enumerate() {
                assert_eq!(
                    line.chars().count(),
                    5,
                    "Digit {i} row {row} has wrong width"
                );
            }
        }
    }

    #[test]
    fn segment_font_dimensions() {
        for (i, digit) in DIGITS_SEGMENT.iter().enumerate() {
            for (row, line) in digit.iter().enumerate() {
                assert_eq!(
                    line.chars().count(),
                    5,
                    "Segment digit {i} row {row} has wrong width"
                );
            }
        }
    }

    #[test]
    fn colon_blinks() {
        let mut clock = DigitalClock::new(DigitalClockConfig::default());
        clock.init(80, 24);

        assert!(clock.colon_visible);
        clock.update(Duration::from_millis(500));
        assert!(!clock.colon_visible);
        clock.update(Duration::from_millis(500));
        assert!(clock.colon_visible);
    }

    #[test]
    fn color_cycle_hue_advances() {
        let config = DigitalClockConfig {
            color_cycle: true,
            color_cycle_speed: 1.0,
            ..Default::default()
        };
        let mut clock = DigitalClock::new(config);
        clock.init(80, 24);

        assert_eq!(clock.hue, 0.0);
        clock.update(Duration::from_secs(1));
        // hue should advance by 1.0 * 1.0 * 30.0 = 30.0
        assert!((clock.hue - 30.0).abs() < 0.01);
    }

    #[test]
    fn stopwatch_mode_format() {
        let config = DigitalClockConfig {
            mode: "stopwatch".into(),
            ..Default::default()
        };
        let mut clock = DigitalClock::new(config);
        clock.init(80, 24);

        // Initially 0
        clock.update(Duration::from_millis(0));
        assert_eq!(clock.time_str, "00:00:00");

        // After 65 seconds
        clock.update(Duration::from_secs(65));
        assert_eq!(clock.time_str, "00:01:05");
    }

    #[test]
    fn countdown_mode_format() {
        let config = DigitalClockConfig {
            mode: "countdown".into(),
            countdown_minutes: 1,
            ..Default::default()
        };
        let mut clock = DigitalClock::new(config);
        clock.init(80, 24);

        // Initial: 1 minute = 60 seconds
        clock.update(Duration::from_millis(0));
        assert_eq!(clock.time_str, "00:01:00");

        // After 30 seconds
        clock.update(Duration::from_secs(30));
        assert_eq!(clock.time_str, "00:00:30");

        // After another 40 seconds (past zero)
        clock.update(Duration::from_secs(40));
        assert_eq!(clock.time_str, "00:00:00");
    }

    #[test]
    fn name_via_info() {
        let clock = DigitalClock::new(DigitalClockConfig::default());
        assert_eq!(clock.info().name, "digital_clock");
    }

    #[test]
    fn render_does_not_panic() {
        let mut clock = DigitalClock::new(DigitalClockConfig::default());
        clock.init(80, 24);
        clock.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(80, 24);
        clock.render(&mut buf);
    }
}
