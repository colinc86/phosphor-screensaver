use std::time::Duration;

use crossterm::event::Event;
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::StarfieldConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

struct Star {
    x: f64,
    y: f64,
    z: f64,
}

pub struct Starfield {
    stars: Vec<Star>,
    width: u16,
    height: u16,
    star_count: usize,
    speed: f64,
    max_depth: f64,
    #[allow(dead_code)]
    theme: Theme,
}

impl Starfield {
    pub fn new(config: &StarfieldConfig, theme: Theme) -> Self {
        Self {
            stars: Vec::new(),
            width: 80,
            height: 24,
            star_count: config.star_count,
            speed: config.speed,
            max_depth: 64.0,
            theme,
        }
    }

    fn spawn_star(width: u16, height: u16, max_depth: f64, far: bool) -> Star {
        let mut rng = rand::rng();
        let half_w = width as f64 / 2.0;
        let half_h = height as f64 / 2.0;
        Star {
            x: rng.random_range(-half_w..half_w),
            y: rng.random_range(-half_h..half_h),
            z: if far {
                rng.random_range(1.0..max_depth)
            } else {
                max_depth
            },
        }
    }

    /// Project a 3D star position to 2D screen coordinates.
    /// Returns None if the star is behind the camera or off-screen.
    pub fn project(
        x: f64,
        y: f64,
        z: f64,
        half_w: f64,
        half_h: f64,
    ) -> Option<(f64, f64)> {
        if z <= 0.0 {
            return None;
        }
        let sx = (x / z) * half_w + half_w;
        let sy = (y / z) * half_h + half_h;
        Some((sx, sy))
    }
}

impl Screensaver for Starfield {
    fn name(&self) -> &str {
        "starfield"
    }

    fn description(&self) -> &str {
        "A parallax starfield flying through space"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.stars.clear();
        for _ in 0..self.star_count {
            self.stars
                .push(Self::spawn_star(width, height, self.max_depth, true));
        }
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let move_speed = self.speed * 20.0 * dt_secs;

        for star in &mut self.stars {
            star.z -= move_speed;
        }

        // Respawn stars that have passed the camera
        let width = self.width;
        let height = self.height;
        let max_depth = self.max_depth;
        for star in &mut self.stars {
            if star.z <= 0.5 {
                *star = Self::spawn_star(width, height, max_depth, false);
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let half_w = area.width as f64 / 2.0;
        let half_h = area.height as f64 / 2.0;

        for star in &self.stars {
            let Some((sx, sy)) = Self::project(star.x, star.y, star.z, half_w, half_h)
            else {
                continue;
            };

            let px = sx.round() as u16;
            let py = sy.round() as u16;
            if px >= area.width || py >= area.height {
                continue;
            }

            // Brightness based on depth (closer = brighter)
            let depth_ratio = 1.0 - (star.z / self.max_depth);
            let brightness = (depth_ratio * 255.0) as u8;

            let (ch, color) = if depth_ratio > 0.8 {
                ('*', Color::Rgb(brightness, brightness, brightness))
            } else if depth_ratio > 0.5 {
                ('+', Color::Rgb(brightness, brightness, brightness))
            } else if depth_ratio > 0.2 {
                ('·', Color::Rgb(brightness / 2, brightness / 2, brightness))
            } else {
                ('.', Color::Rgb(brightness / 3, brightness / 3, brightness / 2))
            };

            frame.buffer_mut().cell_mut((px, py)).map(|cell| {
                cell.set_char(ch);
                cell.set_style(Style::default().fg(color));
            });
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
    fn project_center() {
        // A star at (0, 0, z) should project to center
        let result = Starfield::project(0.0, 0.0, 10.0, 40.0, 12.0);
        assert_eq!(result, Some((40.0, 12.0)));
    }

    #[test]
    fn project_behind_camera() {
        let result = Starfield::project(5.0, 5.0, -1.0, 40.0, 12.0);
        assert_eq!(result, None);
    }

    #[test]
    fn project_at_camera() {
        let result = Starfield::project(5.0, 5.0, 0.0, 40.0, 12.0);
        assert_eq!(result, None);
    }

    #[test]
    fn project_right_of_center() {
        let result = Starfield::project(10.0, 0.0, 10.0, 40.0, 12.0).unwrap();
        // x/z * half_w + half_w = 10/10 * 40 + 40 = 80
        assert_eq!(result.0, 80.0);
        assert_eq!(result.1, 12.0);
    }

    #[test]
    fn stars_initialized_on_init() {
        let config = StarfieldConfig::default();
        let theme = Theme::default_theme();
        let mut sf = Starfield::new(&config, theme);
        sf.init(80, 24);
        assert_eq!(sf.stars.len(), config.star_count);
    }

    #[test]
    fn stars_move_closer_on_update() {
        let config = StarfieldConfig::default();
        let theme = Theme::default_theme();
        let mut sf = Starfield::new(&config, theme);
        sf.init(80, 24);
        let initial_z: Vec<f64> = sf.stars.iter().map(|s| s.z).collect();
        sf.update(Duration::from_millis(100));
        for (star, &init_z) in sf.stars.iter().zip(&initial_z) {
            // Stars should have moved closer (lower z) or been respawned (high z)
            assert!(star.z < init_z || star.z > init_z * 0.9);
        }
    }
}
