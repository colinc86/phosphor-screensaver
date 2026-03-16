use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent};
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::config::GameOfLifeConfig;
use crate::screensaver::{EventResponse, Screensaver};
use crate::theme::Theme;

/// Color a cell by its age (generations alive).
fn age_color(age: u16) -> Color {
    match age {
        0 => Color::Black,
        1..=3 => Color::White,
        4..=15 => Color::Green,
        16..=50 => Color::Yellow,
        51..=150 => Color::Rgb(255, 165, 0), // orange
        _ => Color::Red,
    }
}

/// Color for heat map based on accumulated heat (0.0 - 1.0).
fn heat_color(heat: f64) -> Color {
    let h = heat.clamp(0.0, 1.0);
    if h < 0.2 {
        let t = (h / 0.2 * 180.0) as u8;
        Color::Rgb(0, 0, t) // dark blue to blue
    } else if h < 0.5 {
        let t = ((h - 0.2) / 0.3 * 255.0) as u8;
        Color::Rgb(0, t, 255 - t) // blue to green
    } else if h < 0.75 {
        let t = ((h - 0.5) / 0.25 * 255.0) as u8;
        Color::Rgb(t, 255, 0) // green to yellow
    } else {
        let t = ((h - 0.75) / 0.25 * 255.0) as u8;
        Color::Rgb(255, 255u8.saturating_sub(t), 0) // yellow to red
    }
}

pub struct GameOfLife {
    /// Cell age: 0 = dead, 1+ = alive for N generations
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
    // Stagnation detection
    recent_hashes: Vec<u64>,
    stagnation_count: u32,
    // Heat map
    heat_map: Vec<Vec<f64>>,
    show_heat_map: bool,
    theme: Theme,
}

impl GameOfLife {
    pub fn new(config: &GameOfLifeConfig, theme: Theme) -> Self {
        Self {
            grid: Vec::new(),
            next: Vec::new(),
            width: 0,
            height: 0,
            cell_char: config.cell_char.chars().next().unwrap_or('█'),
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
            theme,
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
    }

    /// Count live neighbors for cell at (x, y). A cell is alive if its value > 0.
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
                // Update heat map
                if alive {
                    self.heat_map[y][x] = (self.heat_map[y][x] + 0.05).min(1.0);
                } else {
                    self.heat_map[y][x] *= 0.995; // slow decay
                }
            }
        }

        std::mem::swap(&mut self.grid, &mut self.next);
        self.generation += 1;

        // Stagnation detection
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
        // Hash alive/dead state only (not ages) for stagnation detection
        for row in &self.grid {
            for &cell in row {
                (cell > 0).hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

impl Screensaver for GameOfLife {
    fn name(&self) -> &str {
        "game_of_life"
    }

    fn description(&self) -> &str {
        "Conway's Game of Life cellular automaton"
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

            // Auto-reset on stagnation
            let stagnation_threshold = (self.auto_reset_secs as f64 / self.step_interval) as u32;
            if self.stagnation_count >= stagnation_threshold {
                self.randomize();
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buf = frame.buffer_mut();

        if self.show_heat_map {
            // Heat map rendering
            for y in 0..area.height.min(self.height) {
                for x in 0..area.width.min(self.width) {
                    let heat = self.heat_map[y as usize][x as usize];
                    if heat > 0.01 {
                        let alive = self.grid[y as usize][x as usize] > 0;
                        let ch = if alive { self.cell_char } else { '·' };
                        buf.cell_mut((x, y)).map(|cell| {
                            cell.set_char(ch);
                            cell.set_style(Style::default().fg(heat_color(heat)));
                        });
                    }
                }
            }
        } else {
            // Normal / age-colored rendering
            for y in 0..area.height.min(self.height) {
                for x in 0..area.width.min(self.width) {
                    let age = self.grid[y as usize][x as usize];
                    if age > 0 {
                        let color = if self.color_by_age {
                            age_color(age)
                        } else {
                            self.theme.secondary
                        };
                        buf.cell_mut((x, y)).map(|cell| {
                            cell.set_char(self.cell_char);
                            cell.set_style(Style::default().fg(color));
                        });
                    }
                }
            }
        }

        // Show generation counter and mode in bottom-left
        let mode_str = if self.show_heat_map { " [HEAT]" } else { "" };
        let gen_str = format!("Gen: {}{}", self.generation, mode_str);
        let y = area.height.saturating_sub(1);
        for (i, ch) in gen_str.chars().enumerate() {
            let x = i as u16 + 1;
            if x < area.width {
                buf.cell_mut((x, y)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(self.theme.dim));
                });
            }
        }

        if self.paused {
            let msg = "PAUSED";
            let x = area.width.saturating_sub(msg.len() as u16 + 2);
            for (i, ch) in msg.chars().enumerate() {
                buf.cell_mut((x + i as u16, y)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(Color::Yellow));
                });
            }
        }
    }

    fn handle_event(&mut self, event: &Event) -> EventResponse {
        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                KeyCode::Char(' ') => {
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
                _ => EventResponse::Ignored,
            }
        } else {
            EventResponse::Ignored
        }
    }
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
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
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
        // Block should survive with increased age
        assert!(gol.grid[2][2] > 0);
        assert!(gol.grid[2][3] > 0);
        assert!(gol.grid[3][2] > 0);
        assert!(gol.grid[3][3] > 0);
    }

    #[test]
    fn oscillator_blinker() {
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
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
        let config = GameOfLifeConfig {
            auto_reset_secs: 5,
            ..Default::default()
        };
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
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
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
        gol.init(10, 10);
        gol.paused = true;
        let generation = gol.generation;
        gol.update(Duration::from_millis(200));
        assert_eq!(gol.generation, generation);
    }

    #[test]
    fn age_increases_for_survivors() {
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
        gol.width = 6;
        gol.height = 6;
        gol.grid = empty_grid(6, 6);
        gol.next = empty_grid(6, 6);
        gol.heat_map = vec![vec![0.0; 6]; 6];
        // 2x2 block: stable, age should increase each step
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
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
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
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
        gol.width = 5;
        gol.height = 5;
        gol.grid = empty_grid(5, 5);
        gol.next = empty_grid(5, 5);
        gol.heat_map = vec![vec![0.0; 5]; 5];
        gol.heat_map[2][2] = 0.5;
        // Cell is dead, heat should decay
        gol.step();
        assert!(gol.heat_map[2][2] < 0.5);
    }

    #[test]
    fn age_color_returns_valid_colors() {
        // Just verify it doesn't panic for various ages
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
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
        assert!(!gol.show_heat_map);
        let event = Event::Key(KeyEvent::from(KeyCode::Char('h')));
        assert_eq!(gol.handle_event(&event), EventResponse::Continue);
        assert!(gol.show_heat_map);
        assert_eq!(gol.handle_event(&event), EventResponse::Continue);
        assert!(!gol.show_heat_map);
    }
}
