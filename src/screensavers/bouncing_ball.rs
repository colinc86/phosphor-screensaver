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

struct Ball {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    color: Color,
    trail: VecDeque<TrailPoint>,
}

struct Spark {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    lifetime: f64,
    max_lifetime: f64,
}

const BALL_PALETTE: [Color; 6] = [
    Color::Cyan,
    Color::Magenta,
    Color::Yellow,
    Color::Green,
    Color::Red,
    Color::Blue,
];

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

pub struct BouncingBall {
    balls: Vec<Ball>,
    sparks: Vec<Spark>,
    width: u16,
    height: u16,
    trail_length: usize,
    ball_char: String,
    ball_count: usize,
    gravity: String,
    trail_style: String,
    collisions: bool,
    theme: Theme,
    time: f64,
}

impl BouncingBall {
    pub fn new(config: &BouncingBallConfig, theme: Theme) -> Self {
        Self {
            balls: Vec::new(),
            sparks: Vec::new(),
            width: 80,
            height: 24,
            trail_length: config.trail_length,
            ball_char: config.ball_char.clone(),
            ball_count: config.ball_count.max(1),
            gravity: config.gravity.clone(),
            trail_style: config.trail_style.clone(),
            collisions: config.collisions,
            theme,
            time: 0.0,
        }
    }

    /// Compute reflected velocity component with a small random perturbation.
    fn reflect(v: f64) -> f64 {
        let mut rng = rand::rng();
        let perturbation = rng.random_range(-2.0..2.0);
        let reflected = -v + perturbation;
        // Ensure minimum speed
        if reflected.abs() < 5.0 {
            if reflected >= 0.0 {
                5.0
            } else {
                -5.0
            }
        } else {
            reflected
        }
    }

    fn create_balls(&mut self) {
        self.balls.clear();
        let count = self.ball_count.max(1);
        let w = self.width as f64;
        let h = self.height as f64;

        if count == 1 {
            // Single ball: center, same velocity as original
            self.balls.push(Ball {
                x: w / 2.0,
                y: h / 2.0,
                vx: 30.0,
                vy: 15.0,
                color: self.theme.primary,
                trail: VecDeque::new(),
            });
        } else {
            // Multiple balls: evenly spaced, different angles
            let base_speed = 30.0;
            for i in 0..count {
                let frac = (i as f64 + 1.0) / (count as f64 + 1.0);
                let angle =
                    (i as f64 / count as f64) * std::f64::consts::TAU + std::f64::consts::FRAC_PI_4;
                self.balls.push(Ball {
                    x: w * frac,
                    y: h * frac,
                    vx: base_speed * angle.cos(),
                    vy: base_speed * angle.sin(),
                    color: BALL_PALETTE[i % BALL_PALETTE.len()],
                    trail: VecDeque::new(),
                });
            }
        }
    }

    fn handle_collisions(&mut self) {
        if !self.collisions || self.balls.len() < 2 {
            return;
        }

        let n = self.balls.len();
        // Collect collision pairs first to avoid borrow issues
        let mut collision_pairs: Vec<(usize, usize, f64, f64)> = Vec::new();

        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.balls[j].x - self.balls[i].x;
                let dy = self.balls[j].y - self.balls[i].y;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist < 2.0 && dist > 0.0 {
                    collision_pairs.push((i, j, dx, dy));
                }
            }
        }

        for (i, j, dx, dy) in collision_pairs {
            let dist = (dx * dx + dy * dy).sqrt();
            // Collision normal
            let nx = dx / dist;
            let ny = dy / dist;

            // Relative velocity along normal
            let dvx = self.balls[i].vx - self.balls[j].vx;
            let dvy = self.balls[i].vy - self.balls[j].vy;
            let dvn = dvx * nx + dvy * ny;

            // Only collide if balls are approaching
            if dvn > 0.0 {
                // Exchange velocity components along the collision normal
                self.balls[i].vx -= dvn * nx;
                self.balls[i].vy -= dvn * ny;
                self.balls[j].vx += dvn * nx;
                self.balls[j].vy += dvn * ny;

                // Separate balls to avoid sticking
                let overlap = 2.0 - dist;
                self.balls[i].x -= overlap * 0.5 * nx;
                self.balls[i].y -= overlap * 0.5 * ny;
                self.balls[j].x += overlap * 0.5 * nx;
                self.balls[j].y += overlap * 0.5 * ny;

                // Spawn sparks at collision midpoint
                let mid_x = (self.balls[i].x + self.balls[j].x) / 2.0;
                let mid_y = (self.balls[i].y + self.balls[j].y) / 2.0;
                let mut rng = rand::rng();
                let spark_count = rng.random_range(3..=5);
                for _ in 0..spark_count {
                    let angle = rng.random_range(0.0..std::f64::consts::TAU);
                    let speed = rng.random_range(10.0..30.0);
                    self.sparks.push(Spark {
                        x: mid_x,
                        y: mid_y,
                        vx: speed * angle.cos(),
                        vy: speed * angle.sin(),
                        lifetime: 0.3,
                        max_lifetime: 0.3,
                    });
                }
            }
        }
    }

    fn update_sparks(&mut self, dt_secs: f64) {
        for spark in &mut self.sparks {
            spark.x += spark.vx * dt_secs;
            spark.y += spark.vy * dt_secs;
            spark.lifetime -= dt_secs;
        }
        self.sparks.retain(|s| s.lifetime > 0.0);
    }

    fn render_trail_point(
        &self,
        frame: &mut Frame,
        point: &TrailPoint,
        ball_color: Color,
        area_width: u16,
        area_height: u16,
    ) {
        if point.x >= area_width || point.y >= area_height {
            return;
        }

        let intensity = 1.0 - (point.age as f64 / self.trail_length as f64);
        let intensity = intensity.clamp(0.0, 1.0);

        let (ch, color) = match self.trail_style.as_str() {
            "fire" => {
                let ch = if intensity > 0.66 {
                    '\u{2593}' // ▓
                } else if intensity > 0.33 {
                    '\u{2592}' // ▒
                } else {
                    '\u{2591}' // ░
                };
                let r = (255.0 * intensity) as u8;
                let g = (140.0 * intensity * intensity) as u8;
                let b = 0u8;
                (ch, Color::Rgb(r, g, b))
            }
            "rainbow" => {
                let hue = (point.age as f64 / self.trail_length as f64) * 360.0;
                let (r, g, b) = hsv_to_rgb(hue % 360.0, 1.0, intensity);
                ('\u{00B7}', Color::Rgb(r, g, b)) // ·
            }
            _ => {
                // "dots" - original behavior
                let gray = (intensity * 200.0) as u8;
                let color = Color::Rgb(
                    (gray as f64 * 0.3) as u8,
                    (gray as f64 * 0.8) as u8,
                    gray,
                );
                let _ = ball_color; // suppress unused warning for dots mode
                ('\u{00B7}', color) // ·
            }
        };

        let style = Style::default().fg(color);
        frame
            .buffer_mut()
            .cell_mut((point.x, point.y))
            .map(|cell| {
                cell.set_char(ch);
                cell.set_style(style);
            });
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
        self.time = 0.0;
        self.sparks.clear();
        self.create_balls();
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        self.time += dt_secs;

        // Record current positions in trails and age trail points
        for ball in &mut self.balls {
            let px = ball.x.round() as u16;
            let py = ball.y.round() as u16;
            ball.trail.push_back(TrailPoint { x: px, y: py, age: 0 });

            for point in &mut ball.trail {
                point.age += 1;
            }
            while ball.trail.len() > self.trail_length {
                ball.trail.pop_front();
            }
        }

        // Apply gravity and move each ball
        // We need to split the borrow: read self fields, mutate balls
        let gravity = self.gravity.clone();
        let width = self.width;
        let height = self.height;
        let time = self.time;

        for ball in &mut self.balls {
            // Apply gravity
            match gravity.as_str() {
                "down" => {
                    ball.vy += 50.0 * dt_secs;
                }
                "center" => {
                    let cx = width as f64 / 2.0;
                    let cy = height as f64 / 2.0;
                    let ax = (cx - ball.x) * 2.0;
                    let ay = (cy - ball.y) * 2.0;
                    ball.vx += ax * dt_secs;
                    ball.vy += ay * dt_secs;
                }
                "wave" => {
                    let ax = 30.0 * time.cos();
                    let ay = 30.0 * time.sin();
                    ball.vx += ax * dt_secs;
                    ball.vy += ay * dt_secs;
                }
                _ => {} // "none"
            }

            // Move
            ball.x += ball.vx * dt_secs;
            ball.y += ball.vy * dt_secs;

            // Bounce off walls
            if ball.x < 0.0 {
                ball.x = 0.0;
                ball.vx = Self::reflect(ball.vx).abs();
            } else if ball.x >= width as f64 - 1.0 {
                ball.x = width as f64 - 1.0;
                ball.vx = -(Self::reflect(ball.vx).abs());
            }

            if ball.y < 0.0 {
                ball.y = 0.0;
                ball.vy = Self::reflect(ball.vy).abs();
            } else if ball.y >= height as f64 - 1.0 {
                ball.y = height as f64 - 1.0;
                ball.vy = -(Self::reflect(ball.vy).abs());
            }
        }

        // Handle collisions between balls
        self.handle_collisions();

        // Update sparks
        self.update_sparks(dt_secs);
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // Draw trails for all balls
        for ball in &self.balls {
            for point in &ball.trail {
                self.render_trail_point(frame, point, ball.color, area.width, area.height);
            }
        }

        // Draw sparks
        for spark in &self.sparks {
            let sx = spark.x.round() as u16;
            let sy = spark.y.round() as u16;
            if sx < area.width && sy < area.height {
                let intensity = (spark.lifetime / spark.max_lifetime).clamp(0.0, 1.0);
                let r = (255.0 * intensity) as u8;
                let g = (200.0 * intensity) as u8;
                let b = (50.0 * intensity) as u8;
                let style = Style::default().fg(Color::Rgb(r, g, b));
                frame
                    .buffer_mut()
                    .cell_mut((sx, sy))
                    .map(|cell| {
                        cell.set_char('*');
                        cell.set_style(style);
                    });
            }
        }

        // Draw balls
        let ch = self.ball_char.chars().next().unwrap_or('\u{25CF}'); // ●
        for ball in &self.balls {
            let bx = ball.x.round() as u16;
            let by = ball.y.round() as u16;
            if bx < area.width && by < area.height {
                let style = Style::default().fg(ball.color);
                frame.buffer_mut().cell_mut((bx, by)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(style);
                });
            }
        }
    }

    fn handle_event(&mut self, _event: &Event) -> EventResponse {
        EventResponse::Ignored
    }
}

// Accessors for tests - provide backward-compatible access to the primary ball
#[cfg(test)]
impl BouncingBall {
    fn primary_ball(&self) -> &Ball {
        &self.balls[0]
    }

    fn primary_ball_mut(&mut self) -> &mut Ball {
        &mut self.balls[0]
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

    fn make_ball_with_config(config: BouncingBallConfig) -> BouncingBall {
        let theme = Theme::default_theme();
        let mut ball = BouncingBall::new(&config, theme);
        ball.init(80, 24);
        ball
    }

    #[test]
    fn init_centers_ball() {
        let ball = make_ball();
        assert_eq!(ball.primary_ball().x, 40.0);
        assert_eq!(ball.primary_ball().y, 12.0);
    }

    #[test]
    fn update_moves_ball() {
        let mut ball = make_ball();
        let old_x = ball.primary_ball().x;
        let old_y = ball.primary_ball().y;
        ball.update(Duration::from_millis(100));
        assert_ne!(ball.primary_ball().x, old_x);
        assert_ne!(ball.primary_ball().y, old_y);
    }

    #[test]
    fn ball_bounces_off_right_wall() {
        let mut ball = make_ball();
        ball.primary_ball_mut().x = 78.5;
        ball.primary_ball_mut().vx = 30.0;
        ball.update(Duration::from_millis(100));
        // After bounce, vx should be negative
        assert!(ball.primary_ball().vx < 0.0);
    }

    #[test]
    fn ball_bounces_off_left_wall() {
        let mut ball = make_ball();
        ball.primary_ball_mut().x = 0.5;
        ball.primary_ball_mut().vx = -30.0;
        ball.update(Duration::from_millis(100));
        assert!(ball.primary_ball().vx > 0.0);
    }

    #[test]
    fn ball_bounces_off_top_wall() {
        let mut ball = make_ball();
        ball.primary_ball_mut().y = 0.5;
        ball.primary_ball_mut().vy = -30.0;
        ball.update(Duration::from_millis(100));
        assert!(ball.primary_ball().vy > 0.0);
    }

    #[test]
    fn ball_bounces_off_bottom_wall() {
        let mut ball = make_ball();
        ball.primary_ball_mut().y = 22.5;
        ball.primary_ball_mut().vy = 30.0;
        ball.update(Duration::from_millis(100));
        assert!(ball.primary_ball().vy < 0.0);
    }

    #[test]
    fn trail_accumulates_and_limits() {
        let mut ball = make_ball();
        for _ in 0..50 {
            ball.update(Duration::from_millis(33));
        }
        assert!(ball.primary_ball().trail.len() <= ball.trail_length);
    }

    #[test]
    fn reflect_maintains_minimum_speed() {
        for _ in 0..100 {
            let result = BouncingBall::reflect(0.1);
            assert!(result.abs() >= 5.0);
        }
    }

    // --- New tests ---

    #[test]
    fn multi_ball_creates_correct_count() {
        let mut config = BouncingBallConfig::default();
        config.ball_count = 4;
        let saver = make_ball_with_config(config);
        assert_eq!(saver.balls.len(), 4);
        // Each ball should have a different starting position
        for i in 0..saver.balls.len() {
            for j in (i + 1)..saver.balls.len() {
                assert!(
                    saver.balls[i].x != saver.balls[j].x
                        || saver.balls[i].y != saver.balls[j].y,
                    "Balls {i} and {j} should have different positions"
                );
            }
        }
    }

    #[test]
    fn gravity_down_increases_vy() {
        let mut config = BouncingBallConfig::default();
        config.gravity = "down".into();
        let mut saver = make_ball_with_config(config);
        let initial_vy = saver.primary_ball().vy;
        // Use a small dt so the ball doesn't hit a wall
        saver.update(Duration::from_millis(16));
        assert!(
            saver.primary_ball().vy > initial_vy,
            "Gravity down should increase vy: was {initial_vy}, now {}",
            saver.primary_ball().vy
        );
    }

    #[test]
    fn elastic_collision_exchanges_velocities() {
        let mut config = BouncingBallConfig::default();
        config.ball_count = 2;
        config.collisions = true;
        let mut saver = make_ball_with_config(config);

        // Place two balls close together, moving toward each other along x-axis
        saver.balls[0].x = 40.0;
        saver.balls[0].y = 12.0;
        saver.balls[0].vx = 20.0;
        saver.balls[0].vy = 0.0;

        saver.balls[1].x = 41.5;
        saver.balls[1].y = 12.0;
        saver.balls[1].vx = -20.0;
        saver.balls[1].vy = 0.0;

        let v0_before = saver.balls[0].vx;
        let v1_before = saver.balls[1].vx;

        saver.update(Duration::from_millis(16));

        // After collision, velocities along x should have swapped direction
        assert!(
            saver.balls[0].vx < v0_before,
            "Ball 0 vx should decrease after collision: was {v0_before}, now {}",
            saver.balls[0].vx
        );
        assert!(
            saver.balls[1].vx > v1_before,
            "Ball 1 vx should increase after collision: was {v1_before}, now {}",
            saver.balls[1].vx
        );

        // Sparks should have been spawned
        assert!(
            !saver.sparks.is_empty(),
            "Sparks should be spawned on collision"
        );
    }

    #[test]
    fn trail_style_fire_uses_block_chars() {
        let mut config = BouncingBallConfig::default();
        config.trail_style = "fire".into();
        let mut saver = make_ball_with_config(config);

        // Generate some trail points
        for _ in 0..10 {
            saver.update(Duration::from_millis(33));
        }

        // Verify trail exists
        assert!(
            !saver.primary_ball().trail.is_empty(),
            "Trail should have points"
        );

        // Verify render_trail_point produces fire characters by checking the style
        // We test indirectly: fire trail uses block characters ░▒▓
        // The render function is tested via the trail_style field
        assert_eq!(saver.trail_style, "fire");

        // Test the rendering logic directly via a TestBackend
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                saver.render(frame);
            })
            .unwrap();

        // Check that at least one cell in the buffer contains a block char
        let buffer = terminal.backend().buffer();
        let block_chars = ['\u{2591}', '\u{2592}', '\u{2593}'];
        let has_block = (0..buffer.area.width).any(|x| {
            (0..buffer.area.height).any(|y| {
                let cell = &buffer[(x, y)];
                block_chars.contains(&cell.symbol().chars().next().unwrap_or(' '))
            })
        });
        assert!(has_block, "Fire trail should render block characters");
    }
}
