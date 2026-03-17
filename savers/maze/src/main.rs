use std::collections::{BinaryHeap, HashMap};
use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct MazeConfig {
    generation_speed: f64,
    solve_speed: f64,
    wall_style: String,
    show_exploration: bool,
}

impl Default for MazeConfig {
    fn default() -> Self {
        Self {
            generation_speed: 1.0,
            solve_speed: 1.0,
            wall_style: "box".into(),
            show_exploration: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Generating,
    Pausing,
    Solving,
    ShowingSolution,
    Dissolving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellState {
    Wall,
    Passage,
    Visited,
    DeadEnd,
    Solution,
    Cursor,
}

#[derive(Debug, Clone, PartialEq)]
struct AStarNode {
    pos: (usize, usize),
    f_score: f64,
}

impl Eq for AStarNode {}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .f_score
            .partial_cmp(&self.f_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct Maze {
    width: u16,
    height: u16,
    grid: Vec<Vec<CellState>>,
    grid_rows: usize,
    grid_cols: usize,
    maze_rows: usize,
    maze_cols: usize,
    phase: Phase,
    gen_stack: Vec<(usize, usize)>,
    gen_visited: Vec<Vec<bool>>,
    open_set: BinaryHeap<AStarNode>,
    came_from: HashMap<(usize, usize), (usize, usize)>,
    g_score: HashMap<(usize, usize), f64>,
    solution_path: Vec<(usize, usize)>,
    solve_done: bool,
    phase_timer: f64,
    steps_per_frame: f64,
    step_accum: f64,
    dissolve_cells: Vec<(usize, usize)>,
    dissolve_idx: usize,
    config: MazeConfig,
}

impl Maze {
    fn new(config: MazeConfig) -> Self {
        Self {
            width: 80,
            height: 24,
            grid: Vec::new(),
            grid_rows: 0,
            grid_cols: 0,
            maze_rows: 0,
            maze_cols: 0,
            phase: Phase::Generating,
            gen_stack: Vec::new(),
            gen_visited: Vec::new(),
            open_set: BinaryHeap::new(),
            came_from: HashMap::new(),
            g_score: HashMap::new(),
            solution_path: Vec::new(),
            solve_done: false,
            phase_timer: 0.0,
            steps_per_frame: 0.0,
            step_accum: 0.0,
            dissolve_cells: Vec::new(),
            dissolve_idx: 0,
            config,
        }
    }

    fn reset_maze(&mut self) {
        self.maze_cols = (self.width as usize / 2).max(2);
        self.maze_rows = (self.height as usize).max(2);
        self.grid_cols = 2 * self.maze_cols + 1;
        self.grid_rows = 2 * self.maze_rows + 1;
        self.grid = vec![vec![CellState::Wall; self.grid_cols]; self.grid_rows];

        self.gen_visited = vec![vec![false; self.maze_cols]; self.maze_rows];
        self.gen_stack.clear();

        self.gen_visited[0][0] = true;
        self.gen_stack.push((0, 0));
        self.grid[1][1] = CellState::Cursor;

        self.phase = Phase::Generating;
        self.phase_timer = 0.0;
        self.step_accum = 0.0;
        self.steps_per_frame = (self.maze_cols * self.maze_rows) as f64 / 120.0;
        self.steps_per_frame = (self.steps_per_frame * self.config.generation_speed).max(1.0);

        self.open_set.clear();
        self.came_from.clear();
        self.g_score.clear();
        self.solution_path.clear();
        self.solve_done = false;
        self.dissolve_cells.clear();
        self.dissolve_idx = 0;
    }

    fn maze_to_grid(mr: usize, mc: usize) -> (usize, usize) {
        (2 * mr + 1, 2 * mc + 1)
    }

    fn step_generation(&mut self) {
        if self.gen_stack.is_empty() {
            return;
        }

        let &(mr, mc) = self.gen_stack.last().unwrap();
        let (gr, gc) = Self::maze_to_grid(mr, mc);

        if self.grid[gr][gc] == CellState::Cursor {
            self.grid[gr][gc] = CellState::Passage;
        }

        let mut neighbors = Vec::new();
        if mr > 0 && !self.gen_visited[mr - 1][mc] {
            neighbors.push((mr - 1, mc));
        }
        if mr + 1 < self.maze_rows && !self.gen_visited[mr + 1][mc] {
            neighbors.push((mr + 1, mc));
        }
        if mc > 0 && !self.gen_visited[mr][mc - 1] {
            neighbors.push((mr, mc - 1));
        }
        if mc + 1 < self.maze_cols && !self.gen_visited[mr][mc + 1] {
            neighbors.push((mr, mc + 1));
        }

        if let Some(&chosen) = {
            let mut rng = rand::rng();
            if neighbors.is_empty() {
                None
            } else {
                Some(&neighbors[rng.random_range(0..neighbors.len())])
            }
        } {
            let (nr, nc) = chosen;
            let wall_r = gr as isize + (nr as isize - mr as isize);
            let wall_c = gc as isize + (nc as isize - mc as isize);
            self.grid[wall_r as usize][wall_c as usize] = CellState::Passage;

            self.gen_visited[nr][nc] = true;
            let (ngr, ngc) = Self::maze_to_grid(nr, nc);
            self.grid[ngr][ngc] = CellState::Cursor;
            self.gen_stack.push((nr, nc));
        } else {
            self.gen_stack.pop();
            if let Some(&(br, bc)) = self.gen_stack.last() {
                let (bgr, bgc) = Self::maze_to_grid(br, bc);
                self.grid[bgr][bgc] = CellState::Cursor;
            }
        }
    }

    fn start_solving(&mut self) {
        for row in &mut self.grid {
            for cell in row.iter_mut() {
                if *cell == CellState::Cursor {
                    *cell = CellState::Passage;
                }
            }
        }

        let start = (1usize, 1usize);
        let goal_r = self.grid_rows - 2;
        let goal_c = self.grid_cols - 2;

        self.open_set.clear();
        self.came_from.clear();
        self.g_score.clear();
        self.solution_path.clear();
        self.solve_done = false;

        self.g_score.insert(start, 0.0);
        let h = Self::heuristic(start, (goal_r, goal_c));
        self.open_set.push(AStarNode {
            pos: start,
            f_score: h,
        });

        self.steps_per_frame = (self.grid_rows * self.grid_cols) as f64 / 120.0;
        self.steps_per_frame = (self.steps_per_frame * self.config.solve_speed).max(1.0);
        self.step_accum = 0.0;
    }

    fn heuristic(a: (usize, usize), b: (usize, usize)) -> f64 {
        let dr = (a.0 as f64 - b.0 as f64).abs();
        let dc = (a.1 as f64 - b.1 as f64).abs();
        dr + dc
    }

    fn step_solve(&mut self) {
        if self.solve_done || self.open_set.is_empty() {
            self.solve_done = true;
            return;
        }

        let goal = (self.grid_rows - 2, self.grid_cols - 2);

        let current = self.open_set.pop().unwrap();
        let (cr, cc) = current.pos;

        if (cr, cc) == goal {
            self.reconstruct_path(goal);
            self.solve_done = true;
            return;
        }

        if self.config.show_exploration && self.grid[cr][cc] == CellState::Passage {
            self.grid[cr][cc] = CellState::Visited;
        }

        let current_g = self.g_score.get(&(cr, cc)).copied().unwrap_or(f64::MAX);

        let deltas: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dr, dc) in deltas {
            let nr = cr as isize + dr;
            let nc = cc as isize + dc;
            if nr < 0 || nc < 0 {
                continue;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            if nr >= self.grid_rows || nc >= self.grid_cols {
                continue;
            }
            if self.grid[nr][nc] == CellState::Wall {
                continue;
            }

            let tentative_g = current_g + 1.0;
            let existing_g = self.g_score.get(&(nr, nc)).copied().unwrap_or(f64::MAX);
            if tentative_g < existing_g {
                self.came_from.insert((nr, nc), (cr, cc));
                self.g_score.insert((nr, nc), tentative_g);
                let f = tentative_g + Self::heuristic((nr, nc), goal);
                self.open_set.push(AStarNode {
                    pos: (nr, nc),
                    f_score: f,
                });
            }
        }
    }

    fn reconstruct_path(&mut self, goal: (usize, usize)) {
        let mut path = vec![goal];
        let mut current = goal;
        while let Some(&prev) = self.came_from.get(&current) {
            path.push(prev);
            current = prev;
        }
        path.reverse();

        if self.config.show_exploration {
            for row in &mut self.grid {
                for cell in row.iter_mut() {
                    if *cell == CellState::Visited {
                        *cell = CellState::DeadEnd;
                    }
                }
            }
        }

        for &(r, c) in &path {
            self.grid[r][c] = CellState::Solution;
        }
        self.solution_path = path;
    }

    fn start_dissolve(&mut self) {
        let mut rng = rand::rng();
        self.dissolve_cells.clear();
        for r in 0..self.grid_rows {
            for c in 0..self.grid_cols {
                self.dissolve_cells.push((r, c));
            }
        }
        for i in (1..self.dissolve_cells.len()).rev() {
            let j = rng.random_range(0..=i);
            self.dissolve_cells.swap(i, j);
        }
        self.dissolve_idx = 0;
    }

    fn wall_char(&self) -> char {
        match self.config.wall_style.as_str() {
            "hash" => '#',
            "block" => '\u{2588}',
            _ => '\u{2588}',
        }
    }
}

impl ScreensaverPlugin for Maze {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "maze".into(),
            description: "Maze generation and solving with animated pathfinding".into(),
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Animated maze generation".into(),
                "A* pathfinding solver".into(),
                "Dissolve transition between mazes".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "generation_speed".into(),
                    label: "Generation Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "solve_speed".into(),
                    label: "Solve Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "wall_style".into(),
                    label: "Wall Style".into(),
                    kind: FieldKind::Text,
                    default: "box".into(),
                    choices: Some(vec!["box".into(), "thick".into(), "ascii".into()]),
                },
                ConfigField {
                    key: "show_exploration".into(),
                    label: "Show Exploration".into(),
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
        self.reset_maze();
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();

        match self.phase {
            Phase::Generating => {
                self.step_accum += self.steps_per_frame;
                let steps = self.step_accum as usize;
                self.step_accum -= steps as f64;
                for _ in 0..steps {
                    if self.gen_stack.is_empty() {
                        self.phase = Phase::Pausing;
                        self.phase_timer = 0.0;
                        for row in &mut self.grid {
                            for cell in row.iter_mut() {
                                if *cell == CellState::Cursor {
                                    *cell = CellState::Passage;
                                }
                            }
                        }
                        break;
                    }
                    self.step_generation();
                }
            }
            Phase::Pausing => {
                self.phase_timer += dt_secs;
                if self.phase_timer >= 1.0 {
                    self.phase = Phase::Solving;
                    self.start_solving();
                }
            }
            Phase::Solving => {
                self.step_accum += self.steps_per_frame;
                let steps = self.step_accum as usize;
                self.step_accum -= steps as f64;
                for _ in 0..steps {
                    if self.solve_done {
                        self.phase = Phase::ShowingSolution;
                        self.phase_timer = 0.0;
                        break;
                    }
                    self.step_solve();
                }
            }
            Phase::ShowingSolution => {
                self.phase_timer += dt_secs;
                if self.phase_timer >= 2.0 {
                    self.phase = Phase::Dissolving;
                    self.start_dissolve();
                }
            }
            Phase::Dissolving => {
                let dissolve_speed =
                    (self.grid_rows * self.grid_cols) as f64 / 60.0;
                self.step_accum += dissolve_speed;
                let steps = self.step_accum as usize;
                self.step_accum -= steps as f64;
                for _ in 0..steps {
                    if self.dissolve_idx >= self.dissolve_cells.len() {
                        self.reset_maze();
                        return;
                    }
                    let (r, c) = self.dissolve_cells[self.dissolve_idx];
                    self.grid[r][c] = CellState::Wall;
                    self.dissolve_idx += 1;
                }
            }
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let wall_ch = self.wall_char();

        for r in 0..self.grid_rows {
            for c in 0..self.grid_cols {
                let ty = r as u16;
                let tx = (c as u16) * 2;
                if ty >= buf.height() || tx + 1 >= buf.width() {
                    continue;
                }

                let (ch, fg) = match self.grid[r][c] {
                    CellState::Wall => (wall_ch, PluginColor::Rgb(60, 60, 60)),
                    CellState::Passage => (' ', PluginColor::Reset),
                    CellState::Cursor => ('\u{2588}', PluginColor::Rgb(255, 255, 0)),
                    CellState::Visited => ('\u{00B7}', PluginColor::Rgb(120, 120, 40)),
                    CellState::DeadEnd => ('\u{00B7}', PluginColor::Rgb(120, 40, 40)),
                    CellState::Solution => ('\u{2588}', PluginColor::Rgb(0, 255, 255)),
                };

                buf.set(tx, ty, Cell { ch, fg, bg: PluginColor::Reset });
                buf.set(tx + 1, ty, Cell { ch, fg, bg: PluginColor::Reset });
            }
        }
    }
}

fn main() {
    let config = MazeConfig::default();
    phosphor_plugin::run(Maze::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_maze() -> Maze {
        let config = MazeConfig::default();
        Maze::new(config)
    }

    #[test]
    fn test_name_and_description() {
        let maze = make_maze();
        assert_eq!(maze.info().name, "maze");
        assert!(!maze.info().description.is_empty());
    }

    #[test]
    fn test_init_creates_valid_grid() {
        let mut maze = make_maze();
        maze.init(80, 24);
        assert_eq!(maze.grid_rows, 2 * maze.maze_rows + 1);
        assert_eq!(maze.grid_cols, 2 * maze.maze_cols + 1);
        assert!(!maze.grid.is_empty());
        assert_eq!(maze.grid.len(), maze.grid_rows);
        assert_eq!(maze.grid[0].len(), maze.grid_cols);
    }

    #[test]
    fn test_phase_starts_as_generating() {
        let mut maze = make_maze();
        maze.init(80, 24);
        assert_eq!(maze.phase, Phase::Generating);
    }

    #[test]
    fn test_update_advances_generation() {
        let mut maze = make_maze();
        maze.init(80, 24);
        let initial_stack = maze.gen_stack.len();
        for _ in 0..50 {
            maze.update(Duration::from_millis(33));
        }
        let changed = maze.gen_stack.len() != initial_stack
            || maze.phase != Phase::Generating;
        assert!(changed);
    }

    #[test]
    fn test_maze_dimensions_for_terminal_size() {
        let mut maze = make_maze();
        maze.init(120, 40);
        assert_eq!(maze.maze_cols, 60);
        assert_eq!(maze.maze_rows, 40);
        assert_eq!(maze.grid_cols, 121);
        assert_eq!(maze.grid_rows, 81);
    }

    #[test]
    fn test_small_terminal() {
        let mut maze = make_maze();
        maze.init(10, 5);
        assert_eq!(maze.maze_cols, 5);
        assert_eq!(maze.maze_rows, 5);
        assert_eq!(maze.grid_cols, 11);
        assert_eq!(maze.grid_rows, 11);
    }

    #[test]
    fn test_full_generation_completes() {
        let mut maze = make_maze();
        maze.init(20, 10);
        for _ in 0..5000 {
            maze.update(Duration::from_millis(33));
            if maze.phase != Phase::Generating {
                break;
            }
        }
        assert_ne!(maze.phase, Phase::Generating);
    }

    #[test]
    fn test_astar_node_ordering() {
        let a = AStarNode {
            pos: (0, 0),
            f_score: 5.0,
        };
        let b = AStarNode {
            pos: (1, 1),
            f_score: 3.0,
        };
        assert!(a < b);
    }

    #[test]
    fn test_heuristic() {
        let h = Maze::heuristic((0, 0), (3, 4));
        assert!((h - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn render_does_not_panic() {
        let mut maze = make_maze();
        maze.init(80, 24);
        for _ in 0..10 {
            maze.update(Duration::from_millis(33));
        }
        let mut buf = PluginBuffer::new(80, 24);
        maze.render(&mut buf);
    }
}
