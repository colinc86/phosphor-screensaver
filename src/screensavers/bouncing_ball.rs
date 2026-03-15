use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::Event;
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::BouncingBallConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

struct TrailPoint {
    x: u16,
    y: u16,
    age: usize,
}

pub struct BouncingBall {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    width: u16,
    height: u16,
    trail: VecDeque<TrailPoint>,
    trail_length: usize,
    ball_char: String,
    theme: Theme,
}

impl BouncingBall {
    pub fn new(config: &BouncingBallConfig, theme: Theme) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 30.0,
            vy: 15.0,
            width: 80,
            height: 24,
            trail: VecDeque::new(),
            trail_length: config.trail_length,
            ball_char: config.ball_char.clone(),
            theme,
        }
    }

    /// Compute reflected velocity component with a small random perturbation.
    fn reflect(v: f64) -> f64 {
        let mut rng = rand::rng();
        let perturbation = rng.random_range(-2.0..2.0);
        let reflected = -v + perturbation;
        // Ensure minimum speed
        if reflected.abs() < 5.0 {
            if reflected >= 0.0 { 5.0 } else { -5.0 }
        } else {
            reflected
        }
    }
}

impl Screensaver for BouncingBall {
    fn name(&self) -> &str {
        "bouncing_ball"
    }

    fn description(&self) -> &str {
        "A ball bouncing around the screen with a colorful trail"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.x = width as f64 / 2.0;
        self.y = height as f64 / 2.0;
        self.trail.clear();
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();

        // Record current position in trail
        let px = self.x.round() as u16;
        let py = self.y.round() as u16;
        self.trail.push_back(TrailPoint { x: px, y: py, age: 0 });

        // Age all trail points and remove old ones
        for point in &mut self.trail {
            point.age += 1;
        }
        while self.trail.len() > self.trail_length {
            self.trail.pop_front();
        }

        // Move
        self.x += self.vx * dt_secs;
        self.y += self.vy * dt_secs;

        // Bounce off walls
        if self.x < 0.0 {
            self.x = 0.0;
            self.vx = Self::reflect(self.vx).abs();
        } else if self.x >= self.width as f64 - 1.0 {
            self.x = self.width as f64 - 1.0;
            self.vx = -(Self::reflect(self.vx).abs());
        }

        if self.y < 0.0 {
            self.y = 0.0;
            self.vy = Self::reflect(self.vy).abs();
        } else if self.y >= self.height as f64 - 1.0 {
            self.y = self.height as f64 - 1.0;
            self.vy = -(Self::reflect(self.vy).abs());
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // Draw trail with fading intensity
        for point in &self.trail {
            if point.x < area.width && point.y < area.height {
                let intensity = 1.0 - (point.age as f64 / self.trail_length as f64);
                let gray = (intensity * 200.0) as u8;
                let color = Color::Rgb(
                    (gray as f64 * 0.3) as u8,
                    (gray as f64 * 0.8) as u8,
                    gray,
                );
                let style = Style::default().fg(color);
                frame
                    .buffer_mut()
                    .cell_mut((point.x, point.y))
                    .map(|cell| {
                        cell.set_char('·');
                        cell.set_style(style);
                    });
            }
        }

        // Draw ball
        let bx = self.x.round() as u16;
        let by = self.y.round() as u16;
        if bx < area.width && by < area.height {
            let style = Style::default().fg(self.theme.primary);
            let ch = self.ball_char.chars().next().unwrap_or('●');
            frame.buffer_mut().cell_mut((bx, by)).map(|cell| {
                cell.set_char(ch);
                cell.set_style(style);
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

    fn make_ball() -> BouncingBall {
        let config = BouncingBallConfig::default();
        let theme = Theme::default_theme();
        let mut ball = BouncingBall::new(&config, theme);
        ball.init(80, 24);
        ball
    }

    #[test]
    fn init_centers_ball() {
        let ball = make_ball();
        assert_eq!(ball.x, 40.0);
        assert_eq!(ball.y, 12.0);
    }

    #[test]
    fn update_moves_ball() {
        let mut ball = make_ball();
        let old_x = ball.x;
        let old_y = ball.y;
        ball.update(Duration::from_millis(100));
        assert_ne!(ball.x, old_x);
        assert_ne!(ball.y, old_y);
    }

    #[test]
    fn ball_bounces_off_right_wall() {
        let mut ball = make_ball();
        ball.x = 78.5;
        ball.vx = 30.0;
        ball.update(Duration::from_millis(100));
        // After bounce, vx should be negative
        assert!(ball.vx < 0.0);
    }

    #[test]
    fn ball_bounces_off_left_wall() {
        let mut ball = make_ball();
        ball.x = 0.5;
        ball.vx = -30.0;
        ball.update(Duration::from_millis(100));
        assert!(ball.vx > 0.0);
    }

    #[test]
    fn ball_bounces_off_top_wall() {
        let mut ball = make_ball();
        ball.y = 0.5;
        ball.vy = -30.0;
        ball.update(Duration::from_millis(100));
        assert!(ball.vy > 0.0);
    }

    #[test]
    fn ball_bounces_off_bottom_wall() {
        let mut ball = make_ball();
        ball.y = 22.5;
        ball.vy = 30.0;
        ball.update(Duration::from_millis(100));
        assert!(ball.vy < 0.0);
    }

    #[test]
    fn trail_accumulates_and_limits() {
        let mut ball = make_ball();
        for _ in 0..50 {
            ball.update(Duration::from_millis(33));
        }
        assert!(ball.trail.len() <= ball.trail_length);
    }

    #[test]
    fn reflect_maintains_minimum_speed() {
        for _ in 0..100 {
            let result = BouncingBall::reflect(0.1);
            assert!(result.abs() >= 5.0);
        }
    }
}
