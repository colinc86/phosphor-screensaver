use std::time::Duration;

use crossterm::event::Event;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::PlasmaConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

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

/// Compute the plasma value at a given position and time.
fn plasma_value(x: f64, y: f64, t: f64, scale: f64) -> f64 {
    let v = (x / scale + t).sin()
        + (y / scale + t * 0.7).sin()
        + ((x + y) / scale + t * 1.3).sin()
        + ((x * x + y * y).sqrt() / scale + t * 0.5).sin();
    // v is in [-4, 4], normalize to [0, 1]
    (v + 4.0) / 8.0
}

/// Map a normalized plasma value [0,1] to an RGB color based on the color mode.
fn value_to_color(v: f64, color_mode: &str) -> Color {
    match color_mode {
        "fire" => {
            let (r, g, b) = if v < 0.25 {
                let t = v / 0.25;
                ((t * 180.0) as u8, 0, 0)
            } else if v < 0.5 {
                let t = (v - 0.25) / 0.25;
                (180 + (t * 75.0) as u8, (t * 165.0) as u8, 0)
            } else if v < 0.75 {
                let t = (v - 0.5) / 0.25;
                (255, 165 + (t * 90.0) as u8, (t * 50.0) as u8)
            } else {
                let t = (v - 0.75) / 0.25;
                (255, 255, 50 + (t * 205.0) as u8)
            };
            Color::Rgb(r, g, b)
        }
        "ocean" => {
            let (r, g, b) = if v < 0.25 {
                let t = v / 0.25;
                (0, 0, (t * 180.0) as u8)
            } else if v < 0.5 {
                let t = (v - 0.25) / 0.25;
                (0, (t * 200.0) as u8, 180 + (t * 75.0) as u8)
            } else if v < 0.75 {
                let t = (v - 0.5) / 0.25;
                (0, 200 + (t * 55.0) as u8, 255 - (t * 100.0) as u8)
            } else {
                let t = (v - 0.75) / 0.25;
                ((t * 255.0) as u8, 255, 155 + (t * 100.0) as u8)
            };
            Color::Rgb(r, g, b)
        }
        "mono" => {
            let g = (v * 255.0) as u8;
            Color::Rgb(g, g, g)
        }
        // "rainbow" and default
        _ => {
            let hue = v * 360.0;
            let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
            Color::Rgb(r, g, b)
        }
    }
}

pub struct Plasma {
    width: u16,
    height: u16,
    t: f64,
    speed: f64,
    scale: f64,
    mode: String,
    color_mode: String,
    #[allow(dead_code)]
    theme: Theme,
}

impl Plasma {
    pub fn new(config: &PlasmaConfig, theme: Theme) -> Self {
        Self {
            width: 80,
            height: 24,
            t: 0.0,
            speed: config.speed,
            scale: config.scale,
            mode: config.mode.clone(),
            color_mode: config.color_mode.clone(),
            theme,
        }
    }
}

impl Screensaver for Plasma {
    fn name(&self) -> &str {
        "plasma"
    }

    fn description(&self) -> &str {
        "Classic demoscene plasma effect with shifting colors"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.t = 0.0;
    }

    fn update(&mut self, dt: Duration) {
        self.t += dt.as_secs_f64() * self.speed;
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        if self.mode == "braille" {
            // Each terminal cell covers 2x4 subpixels
            for cy in 0..area.height {
                for cx in 0..area.width {
                    let mut bits: u8 = 0;
                    let mut val_sum: f64 = 0.0;

                    for dx in 0..2usize {
                        for dy in 0..4usize {
                            let sx = cx as f64 * 2.0 + dx as f64;
                            let sy = cy as f64 * 4.0 + dy as f64;
                            let v = plasma_value(sx, sy, self.t, self.scale);
                            val_sum += v;
                            if v > 0.5 {
                                bits |= braille_bit(dx, dy);
                            }
                        }
                    }

                    let avg = val_sum / 8.0;
                    let color = value_to_color(avg, &self.color_mode);
                    let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');

                    frame.buffer_mut().cell_mut((cx, cy)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(Style::default().fg(color));
                    });
                }
            }
        } else {
            // Fill mode: solid color per cell
            for y in 0..area.height {
                for x in 0..area.width {
                    let v = plasma_value(x as f64, y as f64, self.t, self.scale);
                    let color = value_to_color(v, &self.color_mode);

                    frame.buffer_mut().cell_mut((x, y)).map(|cell| {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(color));
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

    fn default_plasma() -> Plasma {
        let config = PlasmaConfig::default();
        let theme = Theme::default_theme();
        Plasma::new(&config, theme)
    }

    #[test]
    fn init_creates_valid_state() {
        let mut p = default_plasma();
        p.init(120, 40);
        assert_eq!(p.width, 120);
        assert_eq!(p.height, 40);
        assert_eq!(p.t, 0.0);
    }

    #[test]
    fn update_advances_time() {
        let mut p = default_plasma();
        p.init(80, 24);
        p.update(Duration::from_millis(100));
        assert!(p.t > 0.0);
    }

    #[test]
    fn name_and_description() {
        let p = default_plasma();
        assert_eq!(p.name(), "plasma");
        assert!(!p.description().is_empty());
    }

    #[test]
    fn handle_event_returns_ignored() {
        let mut p = default_plasma();
        let event = Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(matches!(p.handle_event(&event), EventResponse::Ignored));
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
    fn plasma_value_in_range() {
        for x in 0..20 {
            for y in 0..20 {
                let v = plasma_value(x as f64, y as f64, 1.0, 8.0);
                assert!((0.0..=1.0).contains(&v), "v={v} out of range");
            }
        }
    }
}
