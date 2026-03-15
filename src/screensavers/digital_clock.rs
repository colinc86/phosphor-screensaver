use std::time::Duration;

use crossterm::event::Event;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::DigitalClockConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

/// 5-line tall block font for digits 0-9 and colon.
const DIGITS: [[&str; 5]; 11] = [
    // 0
    [
        "█████",
        "█   █",
        "█   █",
        "█   █",
        "█████",
    ],
    // 1
    [
        "  █  ",
        " ██  ",
        "  █  ",
        "  █  ",
        "█████",
    ],
    // 2
    [
        "█████",
        "    █",
        "█████",
        "█    ",
        "█████",
    ],
    // 3
    [
        "█████",
        "    █",
        "█████",
        "    █",
        "█████",
    ],
    // 4
    [
        "█   █",
        "█   █",
        "█████",
        "    █",
        "    █",
    ],
    // 5
    [
        "█████",
        "█    ",
        "█████",
        "    █",
        "█████",
    ],
    // 6
    [
        "█████",
        "█    ",
        "█████",
        "█   █",
        "█████",
    ],
    // 7
    [
        "█████",
        "    █",
        "   █ ",
        "  █  ",
        "  █  ",
    ],
    // 8
    [
        "█████",
        "█   █",
        "█████",
        "█   █",
        "█████",
    ],
    // 9
    [
        "█████",
        "█   █",
        "█████",
        "    █",
        "█████",
    ],
    // : (colon) - index 10
    [
        "     ",
        "  █  ",
        "     ",
        "  █  ",
        "     ",
    ],
];

/// Width of a single digit in characters.
const DIGIT_WIDTH: u16 = 5;
/// Space between digits.
const DIGIT_GAP: u16 = 2;
/// Height of digit font.
const DIGIT_HEIGHT: u16 = 5;

pub struct DigitalClock {
    width: u16,
    height: u16,
    time_str: String,
    date_str: String,
    show_date: bool,
    color: Color,
    theme: Theme,
    colon_visible: bool,
    blink_accum: f64,
}

impl DigitalClock {
    pub fn new(config: &DigitalClockConfig, theme: Theme) -> Self {
        let color = parse_color(&config.color, &theme);
        Self {
            width: 80,
            height: 24,
            time_str: String::new(),
            date_str: String::new(),
            show_date: config.show_date,
            color,
            theme,
            colon_visible: true,
            blink_accum: 0.0,
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
}

fn parse_color(name: &str, theme: &Theme) -> Color {
    match name {
        "red" => Color::Red,
        "green" => Color::Green,
        "blue" => Color::Blue,
        "yellow" => Color::Yellow,
        "cyan" => Color::Cyan,
        "magenta" => Color::Magenta,
        "white" => Color::White,
        _ => theme.primary,
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

impl Screensaver for DigitalClock {
    fn name(&self) -> &str {
        "digital_clock"
    }

    fn description(&self) -> &str {
        "A large ASCII-art digital clock display"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    fn update(&mut self, dt: Duration) {
        let now = chrono::Local::now();
        self.time_str = now.format("%H:%M").to_string();
        self.date_str = now.format("%A, %B %e, %Y").to_string();

        // Blink the colon
        self.blink_accum += dt.as_secs_f64();
        if self.blink_accum >= 0.5 {
            self.blink_accum -= 0.5;
            self.colon_visible = !self.colon_visible;
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buf = frame.buffer_mut();

        let total_width = Self::calculate_width(&self.time_str);
        let start_x = area.width.saturating_sub(total_width) / 2;
        let start_y = area.height.saturating_sub(DIGIT_HEIGHT) / 2;

        let mut cursor_x = start_x;

        for ch in self.time_str.chars() {
            let Some(idx) = char_to_digit_index(ch) else {
                cursor_x += DIGIT_WIDTH + DIGIT_GAP;
                continue;
            };

            // Skip colon rendering when blinking off
            let is_colon = idx == 10;
            let visible = !is_colon || self.colon_visible;

            let glyph = &DIGITS[idx];
            for (row, line) in glyph.iter().enumerate() {
                let y = start_y + row as u16;
                if y >= area.height {
                    continue;
                }
                for (col, glyph_ch) in line.chars().enumerate() {
                    let x = cursor_x + col as u16;
                    if x >= area.width {
                        continue;
                    }
                    if glyph_ch != ' ' && visible {
                        buf.cell_mut((x, y)).map(|cell| {
                            cell.set_char(glyph_ch);
                            cell.set_style(Style::default().fg(self.color));
                        });
                    }
                }
            }

            cursor_x += DIGIT_WIDTH + DIGIT_GAP;
        }

        // Draw date below the clock
        if self.show_date {
            let date_y = start_y + DIGIT_HEIGHT + 2;
            if date_y < area.height {
                let date_x = area.width.saturating_sub(self.date_str.len() as u16) / 2;
                for (i, ch) in self.date_str.chars().enumerate() {
                    let x = date_x + i as u16;
                    if x < area.width {
                        buf.cell_mut((x, date_y)).map(|cell| {
                            cell.set_char(ch);
                            cell.set_style(Style::default().fg(self.theme.dim));
                        });
                    }
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
    fn colon_blinks() {
        let config = DigitalClockConfig::default();
        let theme = Theme::default_theme();
        let mut clock = DigitalClock::new(&config, theme);
        clock.init(80, 24);

        assert!(clock.colon_visible);
        clock.update(Duration::from_millis(500));
        assert!(!clock.colon_visible);
        clock.update(Duration::from_millis(500));
        assert!(clock.colon_visible);
    }
}
