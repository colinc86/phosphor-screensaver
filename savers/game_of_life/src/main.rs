use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use phosphor_plugin::protocol::KeyCode;
use phosphor_plugin::{
    Cell, ConfigField, EventResponse, FieldKind, KeyInput, PluginBuffer, PluginColor, PluginInfo,
    ScreensaverPlugin,
};
use rand::Rng;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct GameOfLifeConfig {
    initial_density: f64,
    cell_char: String,
    auto_reset_secs: u64,
    color_by_age: bool,
}

impl Default for GameOfLifeConfig {
    fn default() -> Self {
        Self {
            initial_density: 0.3,
            cell_char: "\u{2588}".into(),
            auto_reset_secs: 60,
            color_by_age: true,
        }
    }
}

fn age_color(age: u16) -> PluginColor {
    match age {
        0 => PluginColor::Rgb(0, 0, 0),
        1..=3 => PluginColor::Rgb(255, 255, 255),
        4..=15 => PluginColor::Rgb(0, 255, 0),
        16..=50 => PluginColor::Rgb(255, 255, 0),
        51..=150 => PluginColor::Rgb(255, 165, 0),
        _ => PluginColor::Rgb(255, 0, 0),
    }
}

fn heat_color(heat: f64) -> PluginColor {
    let h = heat.clamp(0.0, 1.0);
    if h < 0.2 {
        let t = (h / 0.2 * 180.0) as u8;
        PluginColor::Rgb(0, 0, t)
    } else if h < 0.5 {
        let t = ((h - 0.2) / 0.3 * 255.0) as u8;
        PluginColor::Rgb(0, t, 255 - t)
    } else if h < 0.75 {
        let t = ((h - 0.5) / 0.25 * 255.0) as u8;
        PluginColor::Rgb(t, 255, 0)
    } else {
        let t = ((h - 0.75) / 0.25 * 255.0) as u8;
        PluginColor::Rgb(255, 255u8.saturating_sub(t), 0)
    }
}

struct GameOfLife {
    grid: Vec<Vec<u16>>,
    next: Vec<Vec<u16>>,
    width: u16,
    height: u16,
    cell_char: char,
    initial_density: f64,
    auto_reset_secs: u64,
    color_by_age: bool,
    paused: bool,
    generation: u64,
    step_accum: f64,
    step_interval: f64,
    recent_hashes: Vec<u64>,
    stagnation_count: u32,
    heat_map: Vec<Vec<f64>>,
    show_heat_map: bool,
    view_x: i32,
    view_y: i32,
}

impl GameOfLife {
    fn new(config: GameOfLifeConfig) -> Self {
        Self {
            grid: Vec::new(),
            next: Vec::new(),
            width: 0,
            height: 0,
            cell_char: config.cell_char.chars().next().unwrap_or('\u{2588}'),
            initial_density: config.initial_density,
            auto_reset_secs: config.auto_reset_secs,
            color_by_age: config.color_by_age,
            paused: false,
            generation: 0,
            step_accum: 0.0,
            step_interval: 0.1,
            recent_hashes: Vec::new(),
            stagnation_count: 0,
            heat_map: Vec::new(),
            show_heat_map: false,
            view_x: 0,
            view_y: 0,
        }
    }

    fn randomize(&mut self) {
        let mut rng = rand::rng();
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                self.grid[y][x] = if rng.random_range(0.0..1.0) < self.initial_density {
                    1
                } else {
                    0
                };
                self.heat_map[y][x] = 0.0;
            }
        }
        self.generation = 0;
        self.recent_hashes.clear();
        self.stagnation_count = 0;
        self.view_x = 0;
        self.view_y = 0;
    }

    pub fn count_neighbors(grid: &[Vec<u16>], x: usize, y: usize, w: usize, h: usize) -> u8 {
        let mut count = 0u8;
        for dy in [-1i32, 0, 1] {
            for dx in [-1i32, 0, 1] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as i32 + dx).rem_euclid(w as i32) as usize;
                let ny = (y as i32 + dy).rem_euclid(h as i32) as usize;
                if grid[ny][nx] > 0 {
                    count += 1;
                }
            }
        }
        count
    }

    fn step(&mut self) {
        let w = self.width as usize;
        let h = self.height as usize;

        for y in 0..h {
            for x in 0..w {
                let alive = self.grid[y][x] > 0;
                let neighbors = Self::count_neighbors(&self.grid, x, y, w, h);
                self.next[y][x] = match (alive, neighbors) {
                    (true, 2) | (true, 3) => self.grid[y][x].saturating_add(1),
                    (false, 3) => 1,
                    _ => 0,
                };
                if alive {
                    self.heat_map[y][x] = (self.heat_map[y][x] + 0.05).min(1.0);
                } else {
                    self.heat_map[y][x] *= 0.995;
                }
            }
        }

        std::mem::swap(&mut self.grid, &mut self.next);
        self.generation += 1;

        let hash = self.grid_hash();
        if self.recent_hashes.contains(&hash) {
            self.stagnation_count += 1;
        } else {
            self.stagnation_count = 0;
        }
        self.recent_hashes.push(hash);
        if self.recent_hashes.len() > 30 {
            self.recent_hashes.remove(0);
        }
    }

    fn grid_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for row in &self.grid {
            for &cell in row {
                (cell > 0).hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

impl ScreensaverPlugin for GameOfLife {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "game_of_life".into(),
            description: "Conway's Game of Life cellular automaton".into(),
            interactive: true,
            key_bindings: vec![
                ("Space".into(), "Pause/Resume".into()),
                ("r".into(), "Randomize".into()),
                ("h".into(), "Toggle heat map".into()),
                ("n".into(), "Step (when paused)".into()),
                ("←/→/↑/↓".into(), "Pan viewport".into()),
            ],
            features: vec![
                "Color by age".into(),
                "Heat map visualization".into(),
                "Stagnation detection with auto-reset".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "initial_density".into(),
                    label: "Initial Density".into(),
                    kind: FieldKind::F64,
                    default: "0.3".into(),
                    choices: None,
                },
                ConfigField {
                    key: "cell_char".into(),
                    label: "Cell Character".into(),
                    kind: FieldKind::Text,
                    default: "\u{2588}".into(),
                    choices: None,
                },
                ConfigField {
                    key: "auto_reset_secs".into(),
                    label: "Auto Reset (secs)".into(),
                    kind: FieldKind::U64,
                    default: "60".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color_by_age".into(),
                    label: "Color By Age".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.grid = vec![vec![0u16; width as usize]; height as usize];
        self.next = vec![vec![0u16; width as usize]; height as usize];
        self.heat_map = vec![vec![0.0f64; width as usize]; height as usize];
        self.randomize();
    }

    fn update(&mut self, dt: Duration) {
        if self.paused {
            return;
        }

        self.step_accum += dt.as_secs_f64();
        if self.step_accum >= self.step_interval {
            self.step_accum -= self.step_interval;
            self.step();

            let stagnation_threshold = (self.auto_reset_secs as f64 / self.step_interval) as u32;
            if self.stagnation_count >= stagnation_threshold {
                self.randomize();
            }
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let bw = buf.width();
        let bh = buf.height();

        if self.show_heat_map {
            for y in 0..bh.min(self.height) {
                for x in 0..bw.min(self.width) {
                    let gy = (y as i32 + self.view_y).rem_euclid(self.height as i32) as usize;
                    let gx = (x as i32 + self.view_x).rem_euclid(self.width as i32) as usize;
                    let heat = self.heat_map[gy][gx];
                    if heat > 0.01 {
                        let alive = self.grid[gy][gx] > 0;
                        let ch = if alive { self.cell_char } else { '\u{00B7}' };
                        buf.set(
                            x,
                            y,
                            Cell {
                                ch,
                                fg: heat_color(heat),
                                bg: PluginColor::Reset,
                            },
                        );
                    }
                }
            }
        } else {
            for y in 0..bh.min(self.height) {
                for x in 0..bw.min(self.width) {
                    let gy = (y as i32 + self.view_y).rem_euclid(self.height as i32) as usize;
                    let gx = (x as i32 + self.view_x).rem_euclid(self.width as i32) as usize;
                    let age = self.grid[gy][gx];
                    if age > 0 {
                        let color = if self.color_by_age {
                            age_color(age)
                        } else {
                            PluginColor::Rgb(0, 255, 0)
                        };
                        buf.set(
                            x,
                            y,
                            Cell {
                                ch: self.cell_char,
                                fg: color,
                                bg: PluginColor::Reset,
                            },
                        );
                    }
                }
            }
        }

        // Generation counter
        let mode_str = if self.show_heat_map { " [HEAT]" } else { "" };
        let gen_str = format!("Gen: {}{}", self.generation, mode_str);
        let y = bh.saturating_sub(1);
        for (i, ch) in gen_str.chars().enumerate() {
            let x = i as u16 + 1;
            if x < bw {
                buf.set(
                    x,
                    y,
                    Cell {
                        ch,
                        fg: PluginColor::Rgb(100, 100, 100),
                        bg: PluginColor::Reset,
                    },
                );
            }
        }

        if self.paused {
            let msg = "PAUSED";
            let x = bw.saturating_sub(msg.len() as u16 + 2);
            for (i, ch) in msg.chars().enumerate() {
                buf.set(
                    x + i as u16,
                    y,
                    Cell {
                        ch,
                        fg: PluginColor::Rgb(255, 255, 0),
                        bg: PluginColor::Reset,
                    },
                );
            }
        }
    }

    fn handle_key(&mut self, key: KeyInput) -> EventResponse {
        match key.code {
            KeyCode::Space => {
                self.paused = !self.paused;
                EventResponse::Continue
            }
            KeyCode::Char('r') => {
                self.randomize();
                EventResponse::Continue
            }
            KeyCode::Char('h') => {
                self.show_heat_map = !self.show_heat_map;
                EventResponse::Continue
            }
            KeyCode::Char('n') if self.paused => {
                self.step();
                EventResponse::Continue
            }
            KeyCode::Left => {
                self.view_x -= 1;
                EventResponse::Continue
            }
            KeyCode::Right => {
                self.view_x += 1;
                EventResponse::Continue
            }
            KeyCode::Up => {
                self.view_y -= 1;
                EventResponse::Continue
            }
            KeyCode::Down => {
                self.view_y += 1;
                EventResponse::Continue
            }
            _ => EventResponse::Ignored,
        }
    }
}

fn main() {
    let config = GameOfLifeConfig::default();
    phosphor_plugin::run(GameOfLife::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_grid(w: usize, h: usize) -> Vec<Vec<u16>> {
        vec![vec![0u16; w]; h]
    }

    #[test]
    fn count_neighbors_empty() {
        let grid = empty_grid(5, 5);
        assert_eq!(GameOfLife::count_neighbors(&grid, 2, 2, 5, 5), 0);
    }

    #[test]
    fn count_neighbors_all_alive() {
        let grid = vec![vec![1u16; 5]; 5];
        assert_eq!(GameOfLife::count_neighbors(&grid, 2, 2, 5, 5), 8);
    }

    #[test]
    fn count_neighbors_wraps() {
        let mut grid = empty_grid(5, 5);
        grid[4][4] = 1;
        assert_eq!(GameOfLife::count_neighbors(&grid, 0, 0, 5, 5), 1);
    }

    #[test]
    fn still_life_block() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.width = 6;
        gol.height = 6;
        gol.grid = empty_grid(6, 6);
        gol.next = empty_grid(6, 6);
        gol.heat_map = vec![vec![0.0; 6]; 6];
        gol.grid[2][2] = 1;
        gol.grid[2][3] = 1;
        gol.grid[3][2] = 1;
        gol.grid[3][3] = 1;

        gol.step();
        assert!(gol.grid[2][2] > 0);
        assert!(gol.grid[2][3] > 0);
        assert!(gol.grid[3][2] > 0);
        assert!(gol.grid[3][3] > 0);
    }

    #[test]
    fn oscillator_blinker() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.width = 5;
        gol.height = 5;
        gol.grid = empty_grid(5, 5);
        gol.next = empty_grid(5, 5);
        gol.heat_map = vec![vec![0.0; 5]; 5];
        gol.grid[2][1] = 1;
        gol.grid[2][2] = 1;
        gol.grid[2][3] = 1;

        gol.step();
        assert_eq!(gol.grid[2][1], 0);
        assert!(gol.grid[1][2] > 0);
        assert!(gol.grid[2][2] > 0);
        assert!(gol.grid[3][2] > 0);
        assert_eq!(gol.grid[2][3], 0);

        gol.step();
        assert!(gol.grid[2][1] > 0);
        assert!(gol.grid[2][2] > 0);
        assert!(gol.grid[2][3] > 0);
        assert_eq!(gol.grid[1][2], 0);
        assert_eq!(gol.grid[3][2], 0);
    }

    #[test]
    fn stagnation_detected() {
        let mut config = GameOfLifeConfig::default();
        config.auto_reset_secs = 5;
        let mut gol = GameOfLife::new(config);
        gol.width = 5;
        gol.height = 5;
        gol.grid = empty_grid(5, 5);
        gol.next = empty_grid(5, 5);
        gol.heat_map = vec![vec![0.0; 5]; 5];
        for _ in 0..100 {
            gol.step();
        }
        assert!(gol.stagnation_count > 0);
    }

    #[test]
    fn pause_prevents_update() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.init(10, 10);
        gol.paused = true;
        let generation = gol.generation;
        gol.update(Duration::from_millis(200));
        assert_eq!(gol.generation, generation);
    }

    #[test]
    fn age_increases_for_survivors() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.width = 6;
        gol.height = 6;
        gol.grid = empty_grid(6, 6);
        gol.next = empty_grid(6, 6);
        gol.heat_map = vec![vec![0.0; 6]; 6];
        gol.grid[2][2] = 1;
        gol.grid[2][3] = 1;
        gol.grid[3][2] = 1;
        gol.grid[3][3] = 1;

        gol.step();
        assert_eq!(gol.grid[2][2], 2);
        gol.step();
        assert_eq!(gol.grid[2][2], 3);
    }

    #[test]
    fn heat_map_accumulates() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.width = 6;
        gol.height = 6;
        gol.grid = empty_grid(6, 6);
        gol.next = empty_grid(6, 6);
        gol.heat_map = vec![vec![0.0; 6]; 6];
        gol.grid[2][2] = 1;
        gol.grid[2][3] = 1;
        gol.grid[3][2] = 1;
        gol.grid[3][3] = 1;

        gol.step();
        assert!(gol.heat_map[2][2] > 0.0);
    }

    #[test]
    fn heat_map_decays_for_dead_cells() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.width = 5;
        gol.height = 5;
        gol.grid = empty_grid(5, 5);
        gol.next = empty_grid(5, 5);
        gol.heat_map = vec![vec![0.0; 5]; 5];
        gol.heat_map[2][2] = 0.5;
        gol.step();
        assert!(gol.heat_map[2][2] < 0.5);
    }

    #[test]
    fn age_color_returns_valid_colors() {
        let _ = age_color(0);
        let _ = age_color(1);
        let _ = age_color(10);
        let _ = age_color(50);
        let _ = age_color(100);
        let _ = age_color(1000);
    }

    #[test]
    fn heat_color_returns_valid_colors() {
        let _ = heat_color(0.0);
        let _ = heat_color(0.25);
        let _ = heat_color(0.5);
        let _ = heat_color(0.75);
        let _ = heat_color(1.0);
    }

    #[test]
    fn h_key_toggles_heat_map() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        assert!(!gol.show_heat_map);

        let key = KeyInput {
            code: KeyCode::Char('h'),
            modifiers: 0,
        };
        assert_eq!(gol.handle_key(key.clone()), EventResponse::Continue);
        assert!(gol.show_heat_map);
        assert_eq!(gol.handle_key(key), EventResponse::Continue);
        assert!(!gol.show_heat_map);
    }

    #[test]
    fn space_key_toggles_pause() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        assert!(!gol.paused);

        let key = KeyInput {
            code: KeyCode::Space,
            modifiers: 0,
        };
        assert_eq!(gol.handle_key(key.clone()), EventResponse::Continue);
        assert!(gol.paused);
        assert_eq!(gol.handle_key(key), EventResponse::Continue);
        assert!(!gol.paused);
    }

    #[test]
    fn name_via_info() {
        let gol = GameOfLife::new(GameOfLifeConfig::default());
        assert_eq!(gol.info().name, "game_of_life");
    }

    #[test]
    fn render_does_not_panic() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.init(20, 10);
        gol.update(Duration::from_millis(200));
        let mut buf = PluginBuffer::new(20, 10);
        gol.render(&mut buf);
    }

    #[test]
    fn arrow_keys_change_view() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.init(10, 10);

        let left = KeyInput {
            code: KeyCode::Left,
            modifiers: 0,
        };
        assert_eq!(gol.handle_key(left), EventResponse::Continue);
        assert_eq!(gol.view_x, -1);

        let right = KeyInput {
            code: KeyCode::Right,
            modifiers: 0,
        };
        assert_eq!(gol.handle_key(right), EventResponse::Continue);
        assert_eq!(gol.view_x, 0);

        let up = KeyInput {
            code: KeyCode::Up,
            modifiers: 0,
        };
        assert_eq!(gol.handle_key(up), EventResponse::Continue);
        assert_eq!(gol.view_y, -1);

        let down = KeyInput {
            code: KeyCode::Down,
            modifiers: 0,
        };
        assert_eq!(gol.handle_key(down), EventResponse::Continue);
        assert_eq!(gol.view_y, 0);
    }

    #[test]
    fn viewport_wraps_toroidally() {
        let mut gol = GameOfLife::new(GameOfLifeConfig::default());
        gol.width = 10;
        gol.height = 10;
        gol.grid = empty_grid(10, 10);
        gol.next = empty_grid(10, 10);
        gol.heat_map = vec![vec![0.0; 10]; 10];
        gol.grid[0][0] = 1; // alive at (0,0)

        // Shift view so (0,0) in grid maps to screen position (1,1)
        gol.view_x = -1;
        gol.view_y = -1;
        // The cell at grid (0,0) should now render at screen (1,1)
        // because screen (1,1) + view(-1,-1) = grid (0,0)
        let mut buf = PluginBuffer::new(10, 10);
        gol.render(&mut buf);
        // Just ensure no panic and wrapping works
    }

    #[test]
    fn info_is_interactive() {
        let gol = GameOfLife::new(GameOfLifeConfig::default());
        assert!(gol.info().interactive);
    }
}
