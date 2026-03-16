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

    fn is_horizontal(&self) -> bool {
        matches!(self, Direction::Left | Direction::Right)
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

/// Get the junction character when pipes cross.
fn junction_char(existing_horizontal: bool, new_horizontal: bool) -> char {
    match (existing_horizontal, new_horizontal) {
        (true, false) | (false, true) => '┼', // perpendicular crossing
        (true, true) => '═',                   // horizontal overlap
        (false, false) => '║',                 // vertical overlap
    }
}

/// Track which direction a pipe segment runs through a cell.
#[derive(Clone, Copy)]
struct CellInfo {
    ch: char,
    color: Color,
    horizontal: bool, // true if the pipe runs horizontally through this cell
}

struct ActivePipe {
    x: i16,
    y: i16,
    direction: Direction,
    color: Color,
    time_accum: f64,
    /// History of positions this pipe has visited, for flow animation.
    path: Vec<(u16, u16)>,
}

struct Flow {
    /// Index into the pipe's path
    path: Vec<(u16, u16)>,
    position: f64,
    speed: f64,
    color: Color,
}

pub struct Pipes {
    /// Grid tracking what's been drawn at each cell
    grid: Vec<Vec<Option<CellInfo>>>,
    active_pipes: Vec<ActivePipe>,
    completed_paths: Vec<Vec<(u16, u16)>>,
    flows: Vec<Flow>,
    width: u16,
    height: u16,
    turn_probability: f64,
    fill_count: usize,
    max_fill: usize,
    step_interval: f64,
    palette: Vec<Color>,
    junctions_enabled: bool,
    flow_enabled: bool,
    flow_speed: f64,
    flow_spawn_accum: f64,
    #[allow(dead_code)]
    theme: Theme,
}

impl Pipes {
    pub fn new(config: &PipesConfig, theme: Theme) -> Self {
        let palette = Self::make_palette(&config.palette, &theme);
        Self {
            grid: Vec::new(),
            active_pipes: Vec::new(),
            completed_paths: Vec::new(),
            flows: Vec::new(),
            width: 80,
            height: 24,
            turn_probability: config.turn_probability,
            fill_count: 0,
            max_fill: 0,
            step_interval: 0.03,
            palette,
            junctions_enabled: config.junctions,
            flow_enabled: config.flow_enabled,
            flow_speed: config.flow_speed,
            flow_spawn_accum: 0.0,
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

        let (x, y, dir) = match rng.random_range(0..4u8) {
            0 => (
                rng.random_range(0..self.width as i16),
                0i16,
                Direction::Down,
            ),
            1 => (
                rng.random_range(0..self.width as i16),
                self.height as i16 - 1,
                Direction::Up,
            ),
            2 => (
                0i16,
                rng.random_range(0..self.height as i16),
                Direction::Right,
            ),
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
            path: Vec::new(),
        });
    }

    fn fill_ratio(&self) -> f64 {
        if self.max_fill == 0 {
            return 0.0;
        }
        self.fill_count as f64 / self.max_fill as f64
    }

    fn spawn_flow(&mut self) {
        let mut rng = rand::rng();
        if self.completed_paths.is_empty() {
            return;
        }
        let path_idx = rng.random_range(0..self.completed_paths.len());
        let path = self.completed_paths[path_idx].clone();
        if path.len() < 3 {
            return;
        }
        let color = Color::Rgb(
            rng.random_range(200..=255),
            rng.random_range(200..=255),
            rng.random_range(200..=255),
        );
        self.flows.push(Flow {
            path,
            position: 0.0,
            speed: self.flow_speed * (8.0 + rng.random_range(0.0..4.0)),
            color,
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
        self.grid = vec![vec![None; width as usize]; height as usize];
        self.active_pipes.clear();
        self.completed_paths.clear();
        self.flows.clear();
        self.fill_count = 0;
        self.max_fill = (width as usize * height as usize * 70) / 100;
        self.flow_spawn_accum = 0.0;

        for _ in 0..3 {
            self.spawn_pipe();
        }
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let mut rng = rand::rng();

        // Check if we need to clear
        if self.fill_count >= self.max_fill {
            self.grid = vec![vec![None; self.width as usize]; self.height as usize];
            self.fill_count = 0;
            self.active_pipes.clear();
            self.completed_paths.clear();
            self.flows.clear();
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
        let junctions = self.junctions_enabled;

        // Step each active pipe
        let mut new_cells = Vec::new();
        let mut completed = Vec::new();
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
            if pipe.x >= 0 && pipe.x < width as i16 && pipe.y >= 0 && pipe.y < height as i16 {
                let ux = pipe.x as usize;
                let uy = pipe.y as usize;
                let is_horiz = pipe.direction.is_horizontal();

                let (final_ch, final_color) = if junctions {
                    if let Some(existing) = &self.grid[uy][ux] {
                        // Junction: pipe crossing
                        let jch = junction_char(existing.horizontal, is_horiz);
                        (jch, Color::White) // accent color for junctions
                    } else {
                        (ch, pipe.color)
                    }
                } else {
                    (ch, pipe.color)
                };

                new_cells.push((ux, uy, final_ch, final_color, is_horiz));
                pipe.path.push((pipe.x as u16, pipe.y as u16));
            }

            // Move
            pipe.x += pipe.direction.dx();
            pipe.y += pipe.direction.dy();

            // Kill if off-screen
            let alive =
                pipe.x >= 0 && pipe.x < width as i16 && pipe.y >= 0 && pipe.y < height as i16;

            if !alive && pipe.path.len() >= 3 {
                completed.push(pipe.path.clone());
            }

            alive
        });

        for (ux, uy, ch, color, horizontal) in new_cells {
            if self.grid[uy][ux].is_none() {
                self.fill_count += 1;
            }
            self.grid[uy][ux] = Some(CellInfo {
                ch,
                color,
                horizontal,
            });
        }

        self.completed_paths.extend(completed);

        // Flow animation
        if self.flow_enabled && self.fill_ratio() > 0.3 {
            self.flow_spawn_accum += dt_secs;
            if self.flow_spawn_accum >= 0.5 && self.flows.len() < 10 {
                self.flow_spawn_accum = 0.0;
                self.spawn_flow();
            }

            // Update flows
            self.flows.retain_mut(|flow| {
                flow.position += flow.speed * dt_secs;
                (flow.position as usize) < flow.path.len()
            });
        }
    }

    fn render(&self, frame: &mut Frame) {
        let buf = frame.buffer_mut();

        // Draw pipe segments from grid
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                if let Some(info) = &self.grid[y][x] {
                    buf.cell_mut((x as u16, y as u16)).map(|cell| {
                        cell.set_char(info.ch);
                        cell.set_style(Style::default().fg(info.color));
                    });
                }
            }
        }

        // Draw flow markers on top
        for flow in &self.flows {
            let idx = flow.position as usize;
            if idx < flow.path.len() {
                let (fx, fy) = flow.path[idx];
                if fx < self.width && fy < self.height {
                    buf.cell_mut((fx, fy)).map(|cell| {
                        cell.set_char('●');
                        cell.set_style(Style::default().fg(flow.color));
                    });
                }
            }
            // Glow: briefly brighten cells the flow just passed
            if idx > 0 && idx - 1 < flow.path.len() {
                let (gx, gy) = flow.path[idx - 1];
                if gx < self.width && gy < self.height {
                    if let Some(info) = &self.grid[gy as usize][gx as usize] {
                        let bright_color = brighten(info.color);
                        buf.cell_mut((gx, gy)).map(|cell| {
                            cell.set_char(info.ch);
                            cell.set_style(Style::default().fg(bright_color));
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

/// Make a color brighter for flow glow effect.
fn brighten(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(60),
            g.saturating_add(60),
            b.saturating_add(60),
        ),
        Color::Red => Color::Rgb(255, 100, 100),
        Color::Green => Color::Rgb(100, 255, 100),
        Color::Yellow => Color::Rgb(255, 255, 100),
        Color::Blue => Color::Rgb(100, 100, 255),
        Color::Magenta => Color::Rgb(255, 100, 255),
        Color::Cyan => Color::Rgb(100, 255, 255),
        other => other,
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
        assert_eq!(pipes.fill_count, 0);
    }

    #[test]
    fn update_produces_segments() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        for _ in 0..100 {
            pipes.update(Duration::from_millis(33));
        }
        assert!(pipes.fill_count > 0);
    }

    #[test]
    fn clears_at_max_fill() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        pipes.fill_count = pipes.max_fill;
        pipes.update(Duration::from_millis(33));
        assert_eq!(pipes.fill_count, 0);
    }

    #[test]
    fn junction_char_perpendicular() {
        assert_eq!(junction_char(true, false), '┼');
        assert_eq!(junction_char(false, true), '┼');
    }

    #[test]
    fn grid_tracks_cells() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        for _ in 0..100 {
            pipes.update(Duration::from_millis(33));
        }
        // At least some grid cells should be filled
        let filled = pipes
            .grid
            .iter()
            .flatten()
            .filter(|c| c.is_some())
            .count();
        assert!(filled > 0);
        assert_eq!(filled, pipes.fill_count);
    }

    #[test]
    fn brighten_works() {
        match brighten(Color::Rgb(100, 100, 100)) {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 160);
                assert_eq!(g, 160);
                assert_eq!(b, 160);
            }
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn fill_ratio_calculation() {
        let config = PipesConfig::default();
        let theme = Theme::default_theme();
        let mut pipes = Pipes::new(&config, theme);
        pipes.init(80, 24);
        assert_eq!(pipes.fill_ratio(), 0.0);
        pipes.fill_count = pipes.max_fill / 2;
        assert!((pipes.fill_ratio() - 0.5).abs() < 0.01);
    }

    #[test]
    fn direction_is_horizontal() {
        assert!(Direction::Left.is_horizontal());
        assert!(Direction::Right.is_horizontal());
        assert!(!Direction::Up.is_horizontal());
        assert!(!Direction::Down.is_horizontal());
    }
}
