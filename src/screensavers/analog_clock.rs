use std::f64::consts::PI;
use std::time::Duration;

use crossterm::event::Event;
use ratatui::style::{Color, Modifier, Style};
use ratatui::Frame;

use crate::config::AnalogClockConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

pub struct AnalogClock {
    width: u16,
    height: u16,
    hour: u32,
    minute: u32,
    second: u32,
    date_str: String,
    show_seconds: bool,
    show_date: bool,
    color: Color,
    timezone: Option<chrono_tz::Tz>,
    theme: Theme,
}

impl AnalogClock {
    pub fn new(config: &AnalogClockConfig, theme: Theme) -> Self {
        let color = parse_color(&config.color, &theme);
        let timezone = if config.timezone.is_empty() {
            None
        } else {
            config.timezone.parse::<chrono_tz::Tz>().ok()
        };
        Self {
            width: 80,
            height: 24,
            hour: 0,
            minute: 0,
            second: 0,
            date_str: String::new(),
            show_seconds: config.show_seconds,
            show_date: config.show_date,
            color,
            timezone,
            theme,
        }
    }

    /// Calculate the angle in radians for a clock hand position.
    /// 12 o'clock = -PI/2 (pointing up), goes clockwise.
    pub fn hand_angle(value: u32, max: u32) -> f64 {
        let fraction = value as f64 / max as f64;
        fraction * 2.0 * PI - PI / 2.0
    }

    /// Compute endpoint of a hand given angle and length.
    /// Returns (dx, dy) offset from center, accounting for 2:1 character aspect ratio.
    pub fn hand_endpoint(angle: f64, length: f64) -> (f64, f64) {
        let dx = angle.cos() * length * 2.0; // 2x for character aspect ratio
        let dy = angle.sin() * length;
        (dx, dy)
    }

    /// Plot points along a line from (0,0) to (dx,dy).
    fn line_points(dx: f64, dy: f64, steps: usize) -> Vec<(i32, i32)> {
        let mut points = Vec::with_capacity(steps);
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let x = (dx * t).round() as i32;
            let y = (dy * t).round() as i32;
            if points.last() != Some(&(x, y)) {
                points.push((x, y));
            }
        }
        points
    }

    /// Choose a line character based on angle.
    fn hand_char(angle: f64) -> char {
        // Normalize to 0..2PI
        let a = ((angle % (2.0 * PI)) + 2.0 * PI) % (2.0 * PI);
        // 8 segments: → ↘ ↓ ↙ ← ↖ ↑ ↗
        let segment = ((a + PI / 8.0) / (PI / 4.0)) as usize % 8;
        match segment {
            0 => '─', // right
            1 => '╲', // down-right
            2 => '│', // down
            3 => '╱', // down-left
            4 => '─', // left
            5 => '╲', // up-left
            6 => '│', // up
            7 => '╱', // up-right
            _ => '·',
        }
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

impl Screensaver for AnalogClock {
    fn name(&self) -> &str {
        "analog_clock"
    }

    fn description(&self) -> &str {
        "An analog clock face with hour, minute, and second hands"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    fn update(&mut self, _dt: Duration) {
        use chrono::{Local, Timelike};

        let now = if let Some(tz) = self.timezone {
            let utc = chrono::Utc::now();
            let local = utc.with_timezone(&tz);
            self.date_str = local.format("%A, %B %e, %Y").to_string();
            (local.hour(), local.minute(), local.second())
        } else {
            let local = Local::now();
            self.date_str = local.format("%A, %B %e, %Y").to_string();
            (local.hour(), local.minute(), local.second())
        };
        self.hour = now.0 % 12;
        self.minute = now.1;
        self.second = now.2;
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buf = frame.buffer_mut();

        let cx = area.width as i32 / 2;
        let cy = area.height as i32 / 2;
        // Radius accounting for aspect ratio: width needs 2x chars per unit
        let radius = ((area.width as i32 / 4).min(area.height as i32 / 2) - 2).max(3);
        let radius_f = radius as f64;

        // Draw clock face outline — circle of dots
        let circle_steps = (radius_f * 16.0) as usize;
        for i in 0..circle_steps {
            let angle = (i as f64 / circle_steps as f64) * 2.0 * PI;
            let x = cx + (angle.cos() * radius_f * 2.0).round() as i32;
            let y = cy + (angle.sin() * radius_f).round() as i32;
            if x >= 0 && x < area.width as i32 && y >= 0 && y < area.height as i32 {
                buf.cell_mut((x as u16, y as u16)).map(|cell| {
                    cell.set_char('·');
                    cell.set_style(Style::default().fg(self.theme.dim));
                });
            }
        }

        // Draw hour markers (1-12)
        for h in 1..=12 {
            let angle = Self::hand_angle(h, 12);
            let (dx, dy) = Self::hand_endpoint(angle, radius_f * 0.85);
            let x = cx + dx.round() as i32;
            let y = cy + dy.round() as i32;
            if x >= 0 && x < area.width as i32 && y >= 0 && y < area.height as i32 {
                // Use number for major marks, dot for others
                let ch = match h {
                    12 => '⒓',
                    3 => '3',
                    6 => '6',
                    9 => '9',
                    _ => '•',
                };
                buf.cell_mut((x as u16, y as u16)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(self.color));
                });
            }
        }

        // Draw center dot
        if cx >= 0 && cx < area.width as i32 && cy >= 0 && cy < area.height as i32 {
            buf.cell_mut((cx as u16, cy as u16)).map(|cell| {
                cell.set_char('◉');
                cell.set_style(Style::default().fg(self.color));
            });
        }

        // Draw hour hand (short, bold)
        let hour_with_minutes = self.hour as f64 + self.minute as f64 / 60.0;
        let hour_angle = (hour_with_minutes / 12.0) * 2.0 * PI - PI / 2.0;
        let (hdx, hdy) = Self::hand_endpoint(hour_angle, radius_f * 0.5);
        let hour_char = Self::hand_char(hour_angle);
        for (px, py) in Self::line_points(hdx, hdy, 20) {
            let x = cx + px;
            let y = cy + py;
            if x >= 0 && x < area.width as i32 && y >= 0 && y < area.height as i32 {
                buf.cell_mut((x as u16, y as u16)).map(|cell| {
                    cell.set_char(hour_char);
                    cell.set_style(
                        Style::default()
                            .fg(self.color)
                            .add_modifier(Modifier::BOLD),
                    );
                });
            }
        }

        // Draw minute hand (medium length)
        let min_angle = Self::hand_angle(self.minute, 60);
        let (mdx, mdy) = Self::hand_endpoint(min_angle, radius_f * 0.75);
        let min_char = Self::hand_char(min_angle);
        for (px, py) in Self::line_points(mdx, mdy, 30) {
            let x = cx + px;
            let y = cy + py;
            if x >= 0 && x < area.width as i32 && y >= 0 && y < area.height as i32 {
                buf.cell_mut((x as u16, y as u16)).map(|cell| {
                    cell.set_char(min_char);
                    cell.set_style(Style::default().fg(self.color));
                });
            }
        }

        // Draw second hand (full length, dim)
        if self.show_seconds {
            let sec_angle = Self::hand_angle(self.second, 60);
            let (sdx, sdy) = Self::hand_endpoint(sec_angle, radius_f * 0.85);
            for (px, py) in Self::line_points(sdx, sdy, 40) {
                let x = cx + px;
                let y = cy + py;
                if x >= 0 && x < area.width as i32 && y >= 0 && y < area.height as i32 {
                    buf.cell_mut((x as u16, y as u16)).map(|cell| {
                        cell.set_char('·');
                        cell.set_style(Style::default().fg(Color::Red));
                    });
                }
            }
        }

        // Digital time below center
        let time_str = if self.show_seconds {
            format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
        } else {
            format!("{:02}:{:02}", self.hour, self.minute)
        };
        let time_x = cx - (time_str.len() as i32 / 2);
        let time_y = cy + radius as i32 + 2;
        if time_y >= 0 && time_y < area.height as i32 {
            for (i, ch) in time_str.chars().enumerate() {
                let x = time_x + i as i32;
                if x >= 0 && x < area.width as i32 {
                    buf.cell_mut((x as u16, time_y as u16)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(Style::default().fg(self.theme.dim));
                    });
                }
            }
        }

        // Date below time
        if self.show_date {
            let date_x = cx - (self.date_str.len() as i32 / 2);
            let date_y = time_y + 1;
            if date_y >= 0 && date_y < area.height as i32 {
                for (i, ch) in self.date_str.chars().enumerate() {
                    let x = date_x + i as i32;
                    if x >= 0 && x < area.width as i32 {
                        buf.cell_mut((x as u16, date_y as u16)).map(|cell| {
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
    fn hand_angle_12_oclock() {
        let angle = AnalogClock::hand_angle(0, 60);
        // 0/60 * 2PI - PI/2 = -PI/2 (pointing up)
        assert!((angle - (-PI / 2.0)).abs() < 1e-10);
    }

    #[test]
    fn hand_angle_3_oclock() {
        let angle = AnalogClock::hand_angle(15, 60);
        // 15/60 * 2PI - PI/2 = PI/2 - PI/2 = 0 (pointing right)
        assert!(angle.abs() < 1e-10);
    }

    #[test]
    fn hand_angle_6_oclock() {
        let angle = AnalogClock::hand_angle(30, 60);
        // 30/60 * 2PI - PI/2 = PI - PI/2 = PI/2 (pointing down)
        assert!((angle - PI / 2.0).abs() < 1e-10);
    }

    #[test]
    fn hand_angle_9_oclock() {
        let angle = AnalogClock::hand_angle(45, 60);
        // 45/60 * 2PI - PI/2 = 3PI/2 - PI/2 = PI (pointing left)
        assert!((angle - PI).abs() < 1e-10);
    }

    #[test]
    fn hand_endpoint_right() {
        let (dx, dy) = AnalogClock::hand_endpoint(0.0, 10.0);
        // cos(0) = 1, sin(0) = 0, dx = 1 * 10 * 2 = 20
        assert!((dx - 20.0).abs() < 1e-10);
        assert!(dy.abs() < 1e-10);
    }

    #[test]
    fn hand_endpoint_down() {
        let (dx, dy) = AnalogClock::hand_endpoint(PI / 2.0, 10.0);
        // cos(PI/2) ≈ 0, sin(PI/2) = 1, dy = 10
        assert!(dx.abs() < 1e-6);
        assert!((dy - 10.0).abs() < 1e-10);
    }

    #[test]
    fn line_points_generates_unique() {
        let points = AnalogClock::line_points(10.0, 5.0, 20);
        assert!(!points.is_empty());
        // Check no consecutive duplicates
        for w in points.windows(2) {
            assert_ne!(w[0], w[1]);
        }
    }

    #[test]
    fn hand_char_horizontal() {
        assert_eq!(AnalogClock::hand_char(0.0), '─');
        assert_eq!(AnalogClock::hand_char(PI), '─');
    }

    #[test]
    fn hand_char_vertical() {
        assert_eq!(AnalogClock::hand_char(PI / 2.0), '│');
        assert_eq!(AnalogClock::hand_char(-PI / 2.0), '│');
    }

    #[test]
    fn timezone_parsing() {
        let config = AnalogClockConfig {
            timezone: "America/New_York".into(),
            ..Default::default()
        };
        let theme = Theme::default_theme();
        let clock = AnalogClock::new(&config, theme);
        assert!(clock.timezone.is_some());
    }

    #[test]
    fn empty_timezone_is_local() {
        let config = AnalogClockConfig::default();
        let theme = Theme::default_theme();
        let clock = AnalogClock::new(&config, theme);
        assert!(clock.timezone.is_none());
    }

    #[test]
    fn invalid_timezone_falls_back() {
        let config = AnalogClockConfig {
            timezone: "Not/A/Timezone".into(),
            ..Default::default()
        };
        let theme = Theme::default_theme();
        let clock = AnalogClock::new(&config, theme);
        assert!(clock.timezone.is_none());
    }
}
