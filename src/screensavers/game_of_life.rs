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

pub struct GameOfLife {
    grid: Vec<Vec<bool>>,
    next: Vec<Vec<bool>>,
    width: u16,
    height: u16,
    cell_char: char,
    initial_density: f64,
    auto_reset_secs: u64,
    paused: bool,
    generation: u64,
    step_accum: f64,
    step_interval: f64,
    // Stagnation detection
    recent_hashes: Vec<u64>,
    stagnation_count: u32,
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
            paused: false,
            generation: 0,
            step_accum: 0.0,
            step_interval: 0.1,
            recent_hashes: Vec::new(),
            stagnation_count: 0,
            theme,
        }
    }

    fn randomize(&mut self) {
        let mut rng = rand::rng();
        for row in &mut self.grid {
            for cell in row.iter_mut() {
                *cell = rng.random_range(0.0..1.0) < self.initial_density;
            }
        }
        self.generation = 0;
        self.recent_hashes.clear();
        self.stagnation_count = 0;
    }

    /// Count live neighbors for cell at (x, y).
    pub fn count_neighbors(grid: &[Vec<bool>], x: usize, y: usize, w: usize, h: usize) -> u8 {
        let mut count = 0u8;
        for dy in [-1i32, 0, 1] {
            for dx in [-1i32, 0, 1] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as i32 + dx).rem_euclid(w as i32) as usize;
                let ny = (y as i32 + dy).rem_euclid(h as i32) as usize;
                if grid[ny][nx] {
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
                let neighbors = Self::count_neighbors(&self.grid, x, y, w, h);
                self.next[y][x] = match (self.grid[y][x], neighbors) {
                    (true, 2) | (true, 3) => true,
                    (true, _) => false,
                    (false, 3) => true,
                    (false, _) => false,
                };
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
        self.grid.hash(&mut hasher);
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
        self.grid = vec![vec![false; width as usize]; height as usize];
        self.next = vec![vec![false; width as usize]; height as usize];
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

        for y in 0..area.height.min(self.height) {
            for x in 0..area.width.min(self.width) {
                if self.grid[y as usize][x as usize] {
                    buf.cell_mut((x, y)).map(|cell| {
                        cell.set_char(self.cell_char);
                        cell.set_style(Style::default().fg(self.theme.secondary));
                    });
                }
            }
        }

        // Show generation counter in bottom-left
        let gen_str = format!("Gen: {}", self.generation);
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

    fn empty_grid(w: usize, h: usize) -> Vec<Vec<bool>> {
        vec![vec![false; w]; h]
    }

    #[test]
    fn count_neighbors_empty() {
        let grid = empty_grid(5, 5);
        assert_eq!(GameOfLife::count_neighbors(&grid, 2, 2, 5, 5), 0);
    }

    #[test]
    fn count_neighbors_all_alive() {
        let grid = vec![vec![true; 5]; 5];
        assert_eq!(GameOfLife::count_neighbors(&grid, 2, 2, 5, 5), 8);
    }

    #[test]
    fn count_neighbors_wraps() {
        let mut grid = empty_grid(5, 5);
        // Place a cell at (4, 4) — neighbor of (0, 0) via wrapping
        grid[4][4] = true;
        assert_eq!(GameOfLife::count_neighbors(&grid, 0, 0, 5, 5), 1);
    }

    #[test]
    fn still_life_block() {
        // A 2x2 block is a still life
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
        gol.width = 6;
        gol.height = 6;
        gol.grid = empty_grid(6, 6);
        gol.next = empty_grid(6, 6);
        // Place a block at (2,2)
        gol.grid[2][2] = true;
        gol.grid[2][3] = true;
        gol.grid[3][2] = true;
        gol.grid[3][3] = true;

        let before = gol.grid.clone();
        gol.step();
        assert_eq!(gol.grid, before);
    }

    #[test]
    fn oscillator_blinker() {
        // A blinker oscillates between horizontal and vertical
        let config = GameOfLifeConfig::default();
        let theme = Theme::default_theme();
        let mut gol = GameOfLife::new(&config, theme);
        gol.width = 5;
        gol.height = 5;
        gol.grid = empty_grid(5, 5);
        gol.next = empty_grid(5, 5);
        // Horizontal blinker at row 2
        gol.grid[2][1] = true;
        gol.grid[2][2] = true;
        gol.grid[2][3] = true;

        gol.step();
        // Should become vertical
        assert!(!gol.grid[2][1]);
        assert!(gol.grid[1][2]);
        assert!(gol.grid[2][2]);
        assert!(gol.grid[3][2]);
        assert!(!gol.grid[2][3]);

        gol.step();
        // Should return to horizontal
        assert!(gol.grid[2][1]);
        assert!(gol.grid[2][2]);
        assert!(gol.grid[2][3]);
        assert!(!gol.grid[1][2]);
        assert!(!gol.grid[3][2]);
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
        // Empty grid is a still life
        for _ in 0..100 {
            gol.step();
        }
        // Stagnation should have been detected
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
}
