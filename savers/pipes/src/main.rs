use std::time::Duration;

use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};
use rand::Rng;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct PipesConfig {
    palette: String,
    turn_probability: f64,
    junctions: bool,
    flow_enabled: bool,
    flow_speed: f64,
}

impl Default for PipesConfig {
    fn default() -> Self {
        Self {
            palette: "rainbow".into(),
            turn_probability: 0.3,
            junctions: true,
            flow_enabled: true,
            flow_speed: 1.0,
        }
    }
}

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
        (Up, Up) | (Down, Down) => '\u{2502}',       // │
        (Left, Left) | (Right, Right) => '\u{2500}', // ─
        // Corners
        (Up, Right) | (Left, Down) => '\u{250C}', // ┌
        (Up, Left) | (Right, Down) => '\u{2510}', // ┐
        (Down, Right) | (Left, Up) => '\u{2514}', // └
        (Down, Left) | (Right, Up) => '\u{2518}', // ┘
        // Fallback
        _ => '\u{253C}', // ┼
    }
}

/// Get the junction character when pipes cross.
fn junction_char(existing_horizontal: bool, new_horizontal: bool) -> char {
    match (existing_horizontal, new_horizontal) {
        (true, false) | (false, true) => '\u{253C}', // ┼ perpendicular crossing
        (true, true) => '\u{2550}',                  // ═ horizontal overlap
        (false, false) => '\u{2551}',                // ║ vertical overlap
    }
}

/// Track which direction a pipe segment runs through a cell.
#[derive(Clone, Copy)]
struct CellInfo {
    ch: char,
    color: PluginColor,
    horizontal: bool,
}

struct ActivePipe {
    x: i16,
    y: i16,
    direction: Direction,
    color: PluginColor,
    time_accum: f64,
    /// History of positions this pipe has visited, for flow animation.
    path: Vec<(u16, u16)>,
}

struct Flow {
    /// Index into the pipe's path
    path: Vec<(u16, u16)>,
    position: f64,
    speed: f64,
    color: PluginColor,
}

struct Pipes {
    /// Grid tracking what's been drawn at each cell
    grid: Vec<Vec<Option<CellInfo>>>,
    active_pipes: Vec<ActivePipe>,
    completed_paths: Vec<Vec<(u16, u16)>>,
    flows: Vec<Flow>,
    width: u16,
    height: u16,
    fill_count: usize,
    max_fill: usize,
    step_interval: f64,
    palette: Vec<PluginColor>,
    flow_spawn_accum: f64,
    config: PipesConfig,
}

impl Pipes {
    fn new(config: PipesConfig) -> Self {
        let palette = Self::make_palette(&config.palette);
        Self {
            grid: Vec::new(),
            active_pipes: Vec::new(),
            completed_paths: Vec::new(),
            flows: Vec::new(),
            width: 80,
            height: 24,
            fill_count: 0,
            max_fill: 0,
            step_interval: 0.03,
            palette,
            flow_spawn_accum: 0.0,
            config,
        }
    }

    fn make_palette(name: &str) -> Vec<PluginColor> {
        match name {
            "mono" => vec![PluginColor::Rgb(0, 255, 255)],
            "pastel" => vec![
                PluginColor::Rgb(173, 216, 230),
                PluginColor::Rgb(152, 251, 152),
                PluginColor::Rgb(255, 182, 193),
                PluginColor::Rgb(230, 230, 250),
                PluginColor::Rgb(255, 218, 185),
            ],
            // rainbow (default)
            _ => vec![
                PluginColor::Rgb(255, 0, 0),
                PluginColor::Rgb(0, 255, 0),
                PluginColor::Rgb(255, 255, 0),
                PluginColor::Rgb(0, 0, 255),
                PluginColor::Rgb(255, 0, 255),
                PluginColor::Rgb(0, 255, 255),
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
        let color = PluginColor::Rgb(
            rng.random_range(200..=255),
            rng.random_range(200..=255),
            rng.random_range(200..=255),
        );
        self.flows.push(Flow {
            path,
            position: 0.0,
            speed: self.config.flow_speed * (8.0 + rng.random_range(0.0..4.0)),
            color,
        });
    }
}

/// Make a color brighter for flow glow effect.
fn brighten(color: PluginColor) -> PluginColor {
    match color {
        PluginColor::Rgb(r, g, b) => PluginColor::Rgb(
            r.saturating_add(60),
            g.saturating_add(60),
            b.saturating_add(60),
        ),
        _ => color,
    }
}

impl ScreensaverPlugin for Pipes {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "pipes".into(),
            description: "Random pipe drawing with colorful box-drawing characters".into(),
            interactive: false,
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Box-drawing pipe characters".into(),
                "Junction crossings".into(),
                "Flow animation on completed pipes".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "palette".into(),
                    label: "Palette".into(),
                    kind: FieldKind::Text,
                    default: "rainbow".into(),
                    choices: Some(vec!["rainbow".into(), "theme".into(), "mono".into()]),
                },
                ConfigField {
                    key: "turn_probability".into(),
                    label: "Turn Probability".into(),
                    kind: FieldKind::F64,
                    default: "0.3".into(),
                    choices: None,
                },
                ConfigField {
                    key: "junctions".into(),
                    label: "Junctions".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
                ConfigField {
                    key: "flow_enabled".into(),
                    label: "Flow Enabled".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
                ConfigField {
                    key: "flow_speed".into(),
                    label: "Flow Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
            ],
        }
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

        let turn_prob = self.config.turn_probability;
        let width = self.width;
        let height = self.height;
        let junctions = self.config.junctions;

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
                        (jch, PluginColor::Rgb(255, 255, 255)) // accent color for junctions
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
        if self.config.flow_enabled && self.fill_ratio() > 0.3 {
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

    fn render(&self, buf: &mut PluginBuffer) {
        // Draw pipe segments from grid
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                if let Some(info) = &self.grid[y][x] {
                    buf.set(
                        x as u16,
                        y as u16,
                        Cell {
                            ch: info.ch,
                            fg: info.color,
                            bg: PluginColor::Reset,
                        },
                    );
                }
            }
        }

        // Draw flow markers on top
        for flow in &self.flows {
            let idx = flow.position as usize;
            if idx < flow.path.len() {
                let (fx, fy) = flow.path[idx];
                if fx < self.width && fy < self.height {
                    buf.set(
                        fx,
                        fy,
                        Cell {
                            ch: '\u{25CF}',
                            fg: flow.color,
                            bg: PluginColor::Reset,
                        },
                    );
                }
            }
            // Glow: briefly brighten cells the flow just passed
            if idx > 0 && idx - 1 < flow.path.len() {
                let (gx, gy) = flow.path[idx - 1];
                if gx < self.width
                    && gy < self.height
                    && let Some(info) = &self.grid[gy as usize][gx as usize]
                {
                    let bright_color = brighten(info.color);
                    buf.set(
                        gx,
                        gy,
                        Cell {
                            ch: info.ch,
                            fg: bright_color,
                            bg: PluginColor::Reset,
                        },
                    );
                }
            }
        }
    }
}

fn main() {
    let config = PipesConfig::default();
    phosphor_plugin::run(Pipes::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_char_straight() {
        assert_eq!(pipe_char(Direction::Up, Direction::Up), '\u{2502}');
        assert_eq!(pipe_char(Direction::Down, Direction::Down), '\u{2502}');
        assert_eq!(pipe_char(Direction::Left, Direction::Left), '\u{2500}');
        assert_eq!(pipe_char(Direction::Right, Direction::Right), '\u{2500}');
    }

    #[test]
    fn pipe_char_corners() {
        assert_eq!(pipe_char(Direction::Up, Direction::Right), '\u{250C}');
        assert_eq!(pipe_char(Direction::Up, Direction::Left), '\u{2510}');
        assert_eq!(pipe_char(Direction::Down, Direction::Right), '\u{2514}');
        assert_eq!(pipe_char(Direction::Down, Direction::Left), '\u{2518}');
    }

    #[test]
    fn pipe_char_reverse_corners() {
        assert_eq!(pipe_char(Direction::Left, Direction::Down), '\u{250C}');
        assert_eq!(pipe_char(Direction::Right, Direction::Down), '\u{2510}');
        assert_eq!(pipe_char(Direction::Left, Direction::Up), '\u{2514}');
        assert_eq!(pipe_char(Direction::Right, Direction::Up), '\u{2518}');
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
        let mut pipes = Pipes::new(PipesConfig::default());
        pipes.init(80, 24);
        assert_eq!(pipes.active_pipes.len(), 3);
        assert_eq!(pipes.fill_count, 0);
    }

    #[test]
    fn update_produces_segments() {
        let mut pipes = Pipes::new(PipesConfig::default());
        pipes.init(80, 24);
        for _ in 0..100 {
            pipes.update(Duration::from_millis(33));
        }
        assert!(pipes.fill_count > 0);
    }

    #[test]
    fn clears_at_max_fill() {
        let mut pipes = Pipes::new(PipesConfig::default());
        pipes.init(80, 24);
        pipes.fill_count = pipes.max_fill;
        pipes.update(Duration::from_millis(33));
        assert_eq!(pipes.fill_count, 0);
    }

    #[test]
    fn junction_char_perpendicular() {
        assert_eq!(junction_char(true, false), '\u{253C}');
        assert_eq!(junction_char(false, true), '\u{253C}');
    }

    #[test]
    fn grid_tracks_cells() {
        let mut pipes = Pipes::new(PipesConfig::default());
        pipes.init(80, 24);
        for _ in 0..100 {
            pipes.update(Duration::from_millis(33));
        }
        // At least some grid cells should be filled
        let filled = pipes.grid.iter().flatten().filter(|c| c.is_some()).count();
        assert!(filled > 0);
        assert_eq!(filled, pipes.fill_count);
    }

    #[test]
    fn brighten_works() {
        match brighten(PluginColor::Rgb(100, 100, 100)) {
            PluginColor::Rgb(r, g, b) => {
                assert_eq!(r, 160);
                assert_eq!(g, 160);
                assert_eq!(b, 160);
            }
            _ => panic!("Expected Rgb"),
        }
    }

    #[test]
    fn fill_ratio_calculation() {
        let mut pipes = Pipes::new(PipesConfig::default());
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

    #[test]
    fn name_via_info() {
        let pipes = Pipes::new(PipesConfig::default());
        assert_eq!(pipes.info().name, "pipes");
    }

    #[test]
    fn render_does_not_panic() {
        let mut pipes = Pipes::new(PipesConfig::default());
        pipes.init(40, 20);
        for _ in 0..50 {
            pipes.update(Duration::from_millis(33));
        }
        let mut buf = PluginBuffer::new(40, 20);
        pipes.render(&mut buf);
    }
}
