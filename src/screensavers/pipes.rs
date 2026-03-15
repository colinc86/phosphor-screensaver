use std::time::Duration;

use crossterm::event::Event;
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::PipesConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn dx(&self) -> i16 {
        match self {
            Direction::Left => -1,
            Direction::Right => 1,
            _ => 0,
        }
    }

    fn dy(&self) -> i16 {
        match self {
            Direction::Up => -1,
            Direction::Down => 1,
            _ => 0,
        }
    }
}

/// Get the box-drawing character for a direction change.
fn pipe_char(from: Direction, to: Direction) -> char {
    use Direction::*;
    match (from, to) {
        // Straight
        (Up, Up) | (Down, Down) => '│',
        (Left, Left) | (Right, Right) => '─',
        // Corners
        (Up, Right) | (Left, Down) => '┌',
        (Up, Left) | (Right, Down) => '┐',
        (Down, Right) | (Left, Up) => '└',
        (Down, Left) | (Right, Up) => '┘',
        // Fallback
        _ => '┼',
    }
}

struct PipeSegment {
    x: u16,
    y: u16,
    ch: char,
    color: Color,
}

struct ActivePipe {
    x: i16,
    y: i16,
    direction: Direction,
    color: Color,
    time_accum: f64,
}

pub struct Pipes {
    segments: Vec<PipeSegment>,
    active_pipes: Vec<ActivePipe>,
    width: u16,
    height: u16,
    turn_probability: f64,
    fill_count: usize,
    max_fill: usize,
    step_interval: f64,
    palette: Vec<Color>,
    #[allow(dead_code)]
    theme: Theme,
}

impl Pipes {
    pub fn new(config: &PipesConfig, theme: Theme) -> Self {
        let palette = Self::make_palette(&config.palette, &theme);
        Self {
            segments: Vec::new(),
            active_pipes: Vec::new(),
            width: 80,
            height: 24,
            turn_probability: config.turn_probability,
            fill_count: 0,
            max_fill: 0,
            step_interval: 0.03,
            palette,
            theme,
        }
    }

    fn make_palette(name: &str, theme: &Theme) -> Vec<Color> {
        match name {
            "mono" => vec![theme.primary],
            "pastel" => vec![
                Color::Rgb(173, 216, 230),
                Color::Rgb(152, 251, 152),
                Color::Rgb(255, 182, 193),
                Color::Rgb(230, 230, 250),
                Color::Rgb(255, 218, 185),
            ],
            // rainbow (default)
            _ => vec![
                Color::Red,
                Color::Green,
                Color::Yellow,
                Color::Blue,
                Color::Magenta,
                Color::Cyan,
            ],
        }
    }

    fn spawn_pipe(&mut self) {
        let mut rng = rand::rng();
        let color = self.palette[rng.random_range(0..self.palette.len())];

        // Start from a random edge
        let (x, y, dir) = match rng.random_range(0..4u8) {
            0 => (rng.random_range(0..self.width as i16), 0i16, Direction::Down),
            1 => (
                rng.random_range(0..self.width as i16),
                self.height as i16 - 1,
                Direction::Up,
            ),
            2 => (0i16, rng.random_range(0..self.height as i16), Direction::Right),
            _ => (
                self.width as i16 - 1,
                rng.random_range(0..self.height as i16),
                Direction::Left,
            ),
        };

        self.active_pipes.push(ActivePipe {
            x,
            y,
            direction: dir,
            color,
            time_accum: 0.0,
        });
    }
}

impl Screensaver for Pipes {
    fn name(&self) -> &str {
        "pipes"
    }

    fn description(&self) -> &str {
        "Random pipe drawing with colorful box-drawing characters"
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.segments.clear();
        self.active_pipes.clear();
        self.fill_count = 0;
        self.max_fill = (width as usize * height as usize * 70) / 100;

        // Start with a few pipes
        for _ in 0..3 {
            self.spawn_pipe();
        }
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let mut rng = rand::rng();

        // Check if we need to clear
        if self.fill_count >= self.max_fill {
            self.segments.clear();
            self.fill_count = 0;
            self.active_pipes.clear();
            for _ in 0..3 {
                self.spawn_pipe();
            }
            return;
        }

        // Occasionally spawn new pipes
        if self.active_pipes.len() < 5 && rng.random_range(0.0..1.0) < 0.02 {
            self.spawn_pipe();
        }

        let turn_prob = self.turn_probability;
        let width = self.width;
        let height = self.height;

        // Step each active pipe
        let mut new_segments = Vec::new();
        self.active_pipes.retain_mut(|pipe| {
            pipe.time_accum += dt_secs;
            if pipe.time_accum < self.step_interval {
                return true;
            }
            pipe.time_accum -= self.step_interval;

            let old_dir = pipe.direction;

            // Maybe turn
            if rng.random_range(0.0..1.0) < turn_prob {
                pipe.direction = match rng.random_range(0..2u8) {
                    0 => match old_dir {
                        Direction::Up | Direction::Down => Direction::Left,
                        Direction::Left | Direction::Right => Direction::Up,
                    },
                    _ => match old_dir {
                        Direction::Up | Direction::Down => Direction::Right,
                        Direction::Left | Direction::Right => Direction::Down,
                    },
                };
            }

            let ch = pipe_char(old_dir, pipe.direction);

            // Record segment
            if pipe.x >= 0
                && pipe.x < width as i16
                && pipe.y >= 0
                && pipe.y < height as i16
            {
                new_segments.push(PipeSegment {
                    x: pipe.x as u16,
                    y: pipe.y as u16,
                    ch,
                    color: pipe.color,
                });
            }

            // Move
            pipe.x += pipe.direction.dx();
            pipe.y += pipe.direction.dy();

            // Kill if off-screen
            pipe.x >= 0
                && pipe.x < width as i16
                && pipe.y >= 0
                && pipe.y < height as i16
        });

        self.fill_count += new_segments.len();
        self.segments.extend(new_segments);
    }

    fn render(&self, frame: &mut Frame) {
        let buf = frame.buffer_mut();
        for seg in &self.segments {
            buf.cell_mut((seg.x, seg.y)).map(|cell| {
                cell.set_char(seg.ch);
                cell.set_style(Style::default().fg(seg.color));
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
    fn pipe_char_straight() {
        assert_eq!(pipe_char(Direction::Up, Direction::Up), '│');
        assert_eq!(pipe_char(Direction::Down, Direction::Down), '│');
        assert_eq!(pipe_char(Direction::Left, Direction::Left), '─');
        assert_eq!(pipe_char(Direction::Right, Direction::Right), '─');
    }

    #[test]
    fn pipe_char_corners() {
        assert_eq!(pipe_char(Direction::Up, Direction::Right), '┌');
        assert_eq!(pipe_char(Direction::Up, Direction::Left), '┐');
        assert_eq!(pipe_char(Direction::Down, Direction::Right), '└');
        assert_eq!(pipe_char(Direction::Down, Direction::Left), '┘');
    }

    #[test]
    fn pipe_char_reverse_corners() {
        // These are the "from other direction" equivalents
        assert_eq!(pipe_char(Direction::Left, Direction::Down), '┌');
        assert_eq!(pipe_char(Direction::Right, Direction::Down), '┐');
        assert_eq!(pipe_char(Direction::Left, Direction::Up), '└');
        assert_eq!(pipe_char(Direction::Right, Direction::Up), '┘');
    }

    #[test]
    fn direction_deltas() {
        assert_eq!((Direction::Up.dx(), Direction::Up.dy()), (0, -1));
        assert_eq!((Direction::Down.dx(), Direction::Down.dy()), (0, 1));
        assert_eq!((Direction::Left.dx(), Direction::Left.dy()), (-1, 0));
        assert_eq!((Direction::Right.dx(), Direction::Right.dy()), (1, 0));
    }

    #[test]
    fn init_spawns_pipes() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        assert_eq!(pipes.active_pipes.len(), 3);
        assert!(pipes.segments.is_empty());
    }

    #[test]
    fn update_produces_segments() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        // Run enough updates to produce segments
        for _ in 0..100 {
            pipes.update(Duration::from_millis(33));
        }
        assert!(!pipes.segments.is_empty());
    }

    #[test]
    fn clears_at_max_fill() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        // Force near max fill
        pipes.fill_count = pipes.max_fill;
        pipes.update(Duration::from_millis(33));
        assert_eq!(pipes.fill_count, 0);
        assert!(pipes.segments.is_empty());
    }
}
