use std::time::Duration;

use crossterm::event::Event;
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::AuroraConfig;
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

struct Ribbon {
    a1: f64,
    f1: f64,
    p1: f64,
    a2: f64,
    f2: f64,
    p2: f64,
    base_y: f64,
    width: f64,
    pulse_freq: f64,
    hue_offset: f64,
}

pub struct Aurora {
    width: u16,
    height: u16,
    t: f64,
    ribbons: Vec<Ribbon>,
    ribbon_count: usize,
    speed: f64,
    intensity: f64,
    palette: String,
    #[allow(dead_code)]
    theme: Theme,
}

impl Aurora {
    pub fn new(config: &AuroraConfig, theme: Theme) -> Self {
        Self {
            width: 80,
            height: 24,
            t: 0.0,
            ribbons: Vec::new(),
            ribbon_count: config.ribbon_count,
            speed: config.speed,
            intensity: config.intensity,
            palette: config.palette.clone(),
            theme,
        }
    }

    fn generate_ribbons(&mut self) {
        let mut rng = rand::rng();
        self.ribbons.clear();
        let h = self.height as f64;

        for i in 0..self.ribbon_count {
            let spread = 0.6 / self.ribbon_count as f64;
            let base_y = 0.05 + spread * i as f64 + rng.random_range(0.0..spread);

            self.ribbons.push(Ribbon {
                a1: rng.random_range(2.0..6.0) * (h / 24.0),
                f1: rng.random_range(0.02..0.06),
                p1: rng.random_range(0.3..0.8),
                a2: rng.random_range(1.0..3.0) * (h / 24.0),
                f2: rng.random_range(0.04..0.10),
                p2: rng.random_range(0.5..1.2),
                base_y,
                width: rng.random_range(3.0..7.0) * (h / 24.0),
                pulse_freq: rng.random_range(0.2..0.6),
                hue_offset: rng.random_range(-10.0..10.0),
            });
        }
    }

    fn ribbon_center_y(&self, ribbon: &Ribbon, x: f64) -> f64 {
        let h = self.height as f64;
        let y_offset = ribbon.a1 * (ribbon.f1 * x + ribbon.p1 * self.t).sin()
            + ribbon.a2 * (ribbon.f2 * x + ribbon.p2 * self.t).sin();
        ribbon.base_y * h + y_offset
    }

    fn color_for_position(&self, y: f64, ribbon: &Ribbon) -> (f64, f64, f64) {
        let y_frac = (y / self.height as f64).clamp(0.0, 1.0);
        match self.palette.as_str() {
            "warm" => {
                let hue = 0.0 + y_frac * 60.0 + ribbon.hue_offset;
                let (r, g, b) = hsv_to_rgb(hue.rem_euclid(360.0), 0.9, 1.0);
                (r as f64, g as f64, b as f64)
            }
            "mono" => {
                let v = 255.0;
                (v, v, v)
            }
            _ => {
                // "classic": green (120) at top -> cyan (180) -> violet (270) at bottom
                let hue = 120.0 + y_frac * 150.0 + ribbon.hue_offset;
                let (r, g, b) = hsv_to_rgb(hue.clamp(120.0, 280.0), 0.85, 1.0);
                (r as f64, g as f64, b as f64)
            }
        }
    }

    fn intensity_char(intensity: f64) -> char {
        if intensity < 0.1 {
            ' '
        } else if intensity < 0.3 {
            '\u{2591}' // ░
        } else if intensity < 0.6 {
            '\u{2592}' // ▒
        } else if intensity < 0.8 {
            '\u{2593}' // ▓
        } else {
            '\u{2588}' // █
        }
    }
}

impl Screensaver for Aurora {
    fn name(&self) -> &str {
        "aurora"
    }

    fn description(&self) -> &str {
        "Northern lights with undulating ribbons of color"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.t = 0.0;
        self.generate_ribbons();
    }

    fn update(&mut self, dt: Duration) {
        self.t += dt.as_secs_f64() * self.speed;
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let w = area.width as usize;
        let h = area.height as usize;

        for cy in 0..h {
            for cx in 0..w {
                let mut r_acc: f64 = 0.0;
                let mut g_acc: f64 = 0.0;
                let mut b_acc: f64 = 0.0;
                let mut max_intensity: f64 = 0.0;

                for ribbon in &self.ribbons {
                    let center_y = self.ribbon_center_y(ribbon, cx as f64);
                    let dist = (cy as f64 - center_y).abs();

                    if dist >= ribbon.width {
                        continue;
                    }

                    let falloff = 1.0 - dist / ribbon.width;
                    let brightness = 0.6 + 0.4 * (self.t * ribbon.pulse_freq).sin();
                    let cell_intensity = falloff * brightness * self.intensity;

                    let (cr, cg, cb) = self.color_for_position(cy as f64, ribbon);
                    r_acc += cr * cell_intensity;
                    g_acc += cg * cell_intensity;
                    b_acc += cb * cell_intensity;
                    max_intensity += cell_intensity;
                }

                if max_intensity < 0.1 {
                    continue;
                }

                let r = (r_acc as u32).min(255) as u8;
                let g = (g_acc as u32).min(255) as u8;
                let b = (b_acc as u32).min(255) as u8;
                let ch = Self::intensity_char(max_intensity.min(1.0));

                if ch != ' ' {
                    frame
                        .buffer_mut()
                        .cell_mut((cx as u16, cy as u16))
                        .map(|cell| {
                            cell.set_char(ch);
                            cell.set_style(Style::default().fg(Color::Rgb(r, g, b)));
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

    fn make_aurora() -> Aurora {
        let config = AuroraConfig::default();
        let theme = Theme::default_theme();
        Aurora::new(&config, theme)
    }

    #[test]
    fn init_creates_correct_ribbon_count() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        assert_eq!(aurora.ribbons.len(), aurora.ribbon_count);
        assert_eq!(aurora.width, 80);
        assert_eq!(aurora.height, 24);
    }

    #[test]
    fn update_advances_time() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        let t_before = aurora.t;
        aurora.update(Duration::from_millis(100));
        assert!(aurora.t > t_before);
    }

    #[test]
    fn ribbons_have_valid_parameters() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        for ribbon in &aurora.ribbons {
            assert!(ribbon.base_y >= 0.0 && ribbon.base_y <= 1.0);
            assert!(ribbon.width > 0.0);
            assert!(ribbon.pulse_freq > 0.0);
            assert!(ribbon.a1 > 0.0);
            assert!(ribbon.f1 > 0.0);
        }
    }

    #[test]
    fn name_and_description() {
        let aurora = make_aurora();
        assert_eq!(aurora.name(), "aurora");
        assert!(!aurora.description().is_empty());
    }

    #[test]
    fn handle_event_returns_ignored() {
        let mut aurora = make_aurora();
        let event = Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(aurora.handle_event(&event), EventResponse::Ignored);
    }

    #[test]
    fn intensity_char_mapping() {
        assert_eq!(Aurora::intensity_char(0.0), ' ');
        assert_eq!(Aurora::intensity_char(0.2), '\u{2591}');
        assert_eq!(Aurora::intensity_char(0.4), '\u{2592}');
        assert_eq!(Aurora::intensity_char(0.7), '\u{2593}');
        assert_eq!(Aurora::intensity_char(0.9), '\u{2588}');
    }

    #[test]
    fn hsv_to_rgb_green() {
        let (r, g, b) = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 255);
        assert_eq!(b, 0);
    }

    #[test]
    fn reinit_resets_state() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        aurora.update(Duration::from_secs(5));
        aurora.init(120, 40);
        assert_eq!(aurora.t, 0.0);
        assert_eq!(aurora.width, 120);
        assert_eq!(aurora.height, 40);
        assert_eq!(aurora.ribbons.len(), aurora.ribbon_count);
    }
}
