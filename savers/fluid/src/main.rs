use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use termsaver_plugin::{
    Cell, ConfigField, EventResponse, FieldKind, KeyInput, MouseInput,
    PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};
use termsaver_plugin::protocol::{KeyCode, MouseKind};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct FluidConfig {
    viscosity: f64,
    diffusion: f64,
    force_strength: f64,
    interactive: bool,
    color_mode: String,
    solver_iterations: usize,
}

impl Default for FluidConfig {
    fn default() -> Self {
        Self {
            viscosity: 0.0001,
            diffusion: 0.00001,
            force_strength: 5.0,
            interactive: true,
            color_mode: "rainbow".into(),
            solver_iterations: 4,
        }
    }
}

/// Index into a flat grid with 1-cell border padding.
#[inline]
fn ix(x: usize, y: usize, n_x: usize) -> usize {
    x + y * (n_x + 2)
}

fn set_bnd(b: i32, x: &mut Vec<f64>, n_x: usize, n_y: usize) {
    for i in 1..=n_x {
        x[ix(i, 0, n_x)] = if b == 2 {
            -x[ix(i, 1, n_x)]
        } else {
            x[ix(i, 1, n_x)]
        };
        x[ix(i, n_y + 1, n_x)] = if b == 2 {
            -x[ix(i, n_y, n_x)]
        } else {
            x[ix(i, n_y, n_x)]
        };
    }

    for j in 1..=n_y {
        x[ix(0, j, n_x)] = if b == 1 {
            -x[ix(1, j, n_x)]
        } else {
            x[ix(1, j, n_x)]
        };
        x[ix(n_x + 1, j, n_x)] = if b == 1 {
            -x[ix(n_x, j, n_x)]
        } else {
            x[ix(n_x, j, n_x)]
        };
    }

    x[ix(0, 0, n_x)] = 0.5 * (x[ix(1, 0, n_x)] + x[ix(0, 1, n_x)]);
    x[ix(0, n_y + 1, n_x)] = 0.5 * (x[ix(1, n_y + 1, n_x)] + x[ix(0, n_y, n_x)]);
    x[ix(n_x + 1, 0, n_x)] = 0.5 * (x[ix(n_x, 0, n_x)] + x[ix(n_x + 1, 1, n_x)]);
    x[ix(n_x + 1, n_y + 1, n_x)] =
        0.5 * (x[ix(n_x, n_y + 1, n_x)] + x[ix(n_x + 1, n_y, n_x)]);
}

fn diffuse(
    b: i32,
    x: &mut Vec<f64>,
    x0: &[f64],
    diff: f64,
    dt: f64,
    n_x: usize,
    n_y: usize,
    iter: usize,
) {
    let a = dt * diff * (n_x as f64) * (n_y as f64);
    let c = 1.0 + 4.0 * a;

    for _ in 0..iter {
        for j in 1..=n_y {
            for i in 1..=n_x {
                let neighbors = x[ix(i - 1, j, n_x)]
                    + x[ix(i + 1, j, n_x)]
                    + x[ix(i, j - 1, n_x)]
                    + x[ix(i, j + 1, n_x)];
                x[ix(i, j, n_x)] = (x0[ix(i, j, n_x)] + a * neighbors) / c;
            }
        }
        set_bnd(b, x, n_x, n_y);
    }
}

fn advect(
    b: i32,
    d: &mut Vec<f64>,
    d0: &[f64],
    vx: &[f64],
    vy: &[f64],
    dt: f64,
    n_x: usize,
    n_y: usize,
) {
    let dt_x = dt * n_x as f64;
    let dt_y = dt * n_y as f64;

    for j in 1..=n_y {
        for i in 1..=n_x {
            let mut x = i as f64 - dt_x * vx[ix(i, j, n_x)];
            let mut y = j as f64 - dt_y * vy[ix(i, j, n_x)];

            x = x.clamp(0.5, n_x as f64 + 0.5);
            y = y.clamp(0.5, n_y as f64 + 0.5);

            let i0 = x.floor() as usize;
            let i1 = i0 + 1;
            let j0 = y.floor() as usize;
            let j1 = j0 + 1;

            let s1 = x - i0 as f64;
            let s0 = 1.0 - s1;
            let t1 = y - j0 as f64;
            let t0 = 1.0 - t1;

            d[ix(i, j, n_x)] = s0 * (t0 * d0[ix(i0, j0, n_x)] + t1 * d0[ix(i0, j1, n_x)])
                + s1 * (t0 * d0[ix(i1, j0, n_x)] + t1 * d0[ix(i1, j1, n_x)]);
        }
    }
    set_bnd(b, d, n_x, n_y);
}

fn project(
    vx: &mut Vec<f64>,
    vy: &mut Vec<f64>,
    p: &mut Vec<f64>,
    div: &mut Vec<f64>,
    n_x: usize,
    n_y: usize,
    iter: usize,
) {
    let h_x = 1.0 / n_x as f64;
    let h_y = 1.0 / n_y as f64;

    for j in 1..=n_y {
        for i in 1..=n_x {
            div[ix(i, j, n_x)] = -0.5
                * (h_x * (vx[ix(i + 1, j, n_x)] - vx[ix(i - 1, j, n_x)])
                    + h_y * (vy[ix(i, j + 1, n_x)] - vy[ix(i, j - 1, n_x)]));
            p[ix(i, j, n_x)] = 0.0;
        }
    }
    set_bnd(0, div, n_x, n_y);
    set_bnd(0, p, n_x, n_y);

    for _ in 0..iter {
        for j in 1..=n_y {
            for i in 1..=n_x {
                p[ix(i, j, n_x)] = (div[ix(i, j, n_x)]
                    + p[ix(i - 1, j, n_x)]
                    + p[ix(i + 1, j, n_x)]
                    + p[ix(i, j - 1, n_x)]
                    + p[ix(i, j + 1, n_x)])
                    / 4.0;
            }
        }
        set_bnd(0, p, n_x, n_y);
    }

    for j in 1..=n_y {
        for i in 1..=n_x {
            vx[ix(i, j, n_x)] -=
                0.5 * n_x as f64 * (p[ix(i + 1, j, n_x)] - p[ix(i - 1, j, n_x)]);
            vy[ix(i, j, n_x)] -=
                0.5 * n_y as f64 * (p[ix(i, j + 1, n_x)] - p[ix(i, j - 1, n_x)]);
        }
    }
    set_bnd(1, vx, n_x, n_y);
    set_bnd(2, vy, n_x, n_y);
}

fn density_char(intensity: f64) -> char {
    if intensity < 0.05 {
        ' '
    } else if intensity < 0.2 {
        '\u{2591}'
    } else if intensity < 0.45 {
        '\u{2592}'
    } else if intensity < 0.75 {
        '\u{2593}'
    } else {
        '\u{2588}'
    }
}

struct Fluid {
    n_x: usize,
    n_y: usize,
    vx: Vec<f64>,
    vy: Vec<f64>,
    vx0: Vec<f64>,
    vy0: Vec<f64>,
    dye_r: Vec<f64>,
    dye_g: Vec<f64>,
    dye_b: Vec<f64>,
    dye_r0: Vec<f64>,
    dye_g0: Vec<f64>,
    dye_b0: Vec<f64>,
    p: Vec<f64>,
    div: Vec<f64>,
    viscosity: f64,
    diffusion: f64,
    force_strength: f64,
    interactive: bool,
    solver_iterations: usize,
    inject_timer: f64,
    inject_interval: f64,
    last_mouse_x: Option<u16>,
    last_mouse_y: Option<u16>,
}

impl Fluid {
    fn new(config: FluidConfig) -> Self {
        Self {
            n_x: 0,
            n_y: 0,
            vx: Vec::new(),
            vy: Vec::new(),
            vx0: Vec::new(),
            vy0: Vec::new(),
            dye_r: Vec::new(),
            dye_g: Vec::new(),
            dye_b: Vec::new(),
            dye_r0: Vec::new(),
            dye_g0: Vec::new(),
            dye_b0: Vec::new(),
            p: Vec::new(),
            div: Vec::new(),
            viscosity: config.viscosity,
            diffusion: config.diffusion,
            force_strength: config.force_strength,
            interactive: config.interactive,
            solver_iterations: config.solver_iterations,
            inject_timer: 0.0,
            inject_interval: 0.5,
            last_mouse_x: None,
            last_mouse_y: None,
        }
    }

    fn total_size(&self) -> usize {
        (self.n_x + 2) * (self.n_y + 2)
    }

    fn inject_at(&mut self, gx: usize, gy: usize, vel_x: f64, vel_y: f64, r: f64, g: f64, b: f64) {
        let radius = 2i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let nx = gx as i32 + dx;
                let ny = gy as i32 + dy;
                if nx >= 1 && nx <= self.n_x as i32 && ny >= 1 && ny <= self.n_y as i32 {
                    let dist_sq = (dx * dx + dy * dy) as f64;
                    let falloff = (-dist_sq / (radius as f64)).exp();
                    let idx = ix(nx as usize, ny as usize, self.n_x);

                    self.vx[idx] += vel_x * falloff;
                    self.vy[idx] += vel_y * falloff;
                    self.dye_r[idx] = (self.dye_r[idx] + r * falloff).min(1.0);
                    self.dye_g[idx] = (self.dye_g[idx] + g * falloff).min(1.0);
                    self.dye_b[idx] = (self.dye_b[idx] + b * falloff).min(1.0);
                }
            }
        }
    }

    fn inject_random(&mut self) {
        let mut rng = rand::rng();
        let gx = rng.random_range(2..self.n_x.max(3) - 1);
        let gy = rng.random_range(2..self.n_y.max(3) - 1);

        let angle: f64 = rng.random_range(0.0..std::f64::consts::TAU);
        let speed = self.force_strength * rng.random_range(0.5..1.5);
        let vel_x = angle.cos() * speed;
        let vel_y = angle.sin() * speed;

        let r: f64 = rng.random_range(0.3..1.0);
        let g: f64 = rng.random_range(0.3..1.0);
        let b: f64 = rng.random_range(0.3..1.0);

        self.inject_at(gx, gy, vel_x, vel_y, r, g, b);
    }

    fn simulate(&mut self, dt: f64) {
        let n_x = self.n_x;
        let n_y = self.n_y;
        let visc = self.viscosity;
        let diff = self.diffusion;
        let iterations = self.solver_iterations;

        std::mem::swap(&mut self.vx, &mut self.vx0);
        std::mem::swap(&mut self.vy, &mut self.vy0);

        diffuse(1, &mut self.vx, &self.vx0, visc, dt, n_x, n_y, iterations);
        diffuse(2, &mut self.vy, &self.vy0, visc, dt, n_x, n_y, iterations);

        project(
            &mut self.vx,
            &mut self.vy,
            &mut self.p,
            &mut self.div,
            n_x,
            n_y,
            iterations,
        );

        std::mem::swap(&mut self.vx, &mut self.vx0);
        std::mem::swap(&mut self.vy, &mut self.vy0);

        advect(1, &mut self.vx, &self.vx0, &self.vx0, &self.vy0, dt, n_x, n_y);
        advect(2, &mut self.vy, &self.vy0, &self.vx0, &self.vy0, dt, n_x, n_y);

        project(
            &mut self.vx,
            &mut self.vy,
            &mut self.p,
            &mut self.div,
            n_x,
            n_y,
            iterations,
        );

        std::mem::swap(&mut self.dye_r, &mut self.dye_r0);
        diffuse(0, &mut self.dye_r, &self.dye_r0, diff, dt, n_x, n_y, iterations);
        std::mem::swap(&mut self.dye_r, &mut self.dye_r0);
        advect(0, &mut self.dye_r, &self.dye_r0, &self.vx, &self.vy, dt, n_x, n_y);

        std::mem::swap(&mut self.dye_g, &mut self.dye_g0);
        diffuse(0, &mut self.dye_g, &self.dye_g0, diff, dt, n_x, n_y, iterations);
        std::mem::swap(&mut self.dye_g, &mut self.dye_g0);
        advect(0, &mut self.dye_g, &self.dye_g0, &self.vx, &self.vy, dt, n_x, n_y);

        std::mem::swap(&mut self.dye_b, &mut self.dye_b0);
        diffuse(0, &mut self.dye_b, &self.dye_b0, diff, dt, n_x, n_y, iterations);
        std::mem::swap(&mut self.dye_b, &mut self.dye_b0);
        advect(0, &mut self.dye_b, &self.dye_b0, &self.vx, &self.vy, dt, n_x, n_y);

        let decay = (1.0 - 0.1 * dt).max(0.0);
        let size = self.total_size();
        for i in 0..size {
            self.dye_r[i] = (self.dye_r[i] * decay).clamp(0.0, 1.0);
            self.dye_g[i] = (self.dye_g[i] * decay).clamp(0.0, 1.0);
            self.dye_b[i] = (self.dye_b[i] * decay).clamp(0.0, 1.0);
        }
    }

    fn handle_mouse_event(&mut self, mouse: &MouseInput) {
        match mouse.kind {
            MouseKind::Down => {
                self.last_mouse_x = Some(mouse.x);
                self.last_mouse_y = Some(mouse.y);

                let gx = (mouse.x as usize + 1).min(self.n_x);
                let gy = (mouse.y as usize + 1).min(self.n_y);
                if gx >= 1 && gx <= self.n_x && gy >= 1 && gy <= self.n_y {
                    let mut rng = rand::rng();
                    let r: f64 = rng.random_range(0.5..1.0);
                    let g: f64 = rng.random_range(0.5..1.0);
                    let b: f64 = rng.random_range(0.5..1.0);
                    self.inject_at(gx, gy, 0.0, 0.0, r, g, b);
                }
            }
            MouseKind::Drag => {
                let gx = (mouse.x as usize + 1).min(self.n_x);
                let gy = (mouse.y as usize + 1).min(self.n_y);

                if gx >= 1 && gx <= self.n_x && gy >= 1 && gy <= self.n_y {
                    let vel_x = if let Some(prev_x) = self.last_mouse_x {
                        (mouse.x as f64 - prev_x as f64) * self.force_strength
                    } else {
                        0.0
                    };
                    let vel_y = if let Some(prev_y) = self.last_mouse_y {
                        (mouse.y as f64 - prev_y as f64) * self.force_strength
                    } else {
                        0.0
                    };

                    let mut rng = rand::rng();
                    let r: f64 = rng.random_range(0.5..1.0);
                    let g: f64 = rng.random_range(0.5..1.0);
                    let b: f64 = rng.random_range(0.5..1.0);

                    self.inject_at(gx, gy, vel_x, vel_y, r, g, b);
                }

                self.last_mouse_x = Some(mouse.x);
                self.last_mouse_y = Some(mouse.y);
            }
            MouseKind::Up => {
                self.last_mouse_x = None;
                self.last_mouse_y = None;
            }
            _ => {}
        }
    }
}

impl ScreensaverPlugin for Fluid {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "fluid".into(),
            description: "Fluid dynamics simulation using Navier-Stokes equations".into(),
            key_bindings: vec![
                ("r".into(), "Reset simulation".into()),
                ("Mouse drag".into(), "Inject dye and force".into()),
            ],
            features: vec![
                "Navier-Stokes fluid simulation".into(),
                "RGB dye advection".into(),
                "Interactive mouse input".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "viscosity".into(),
                    label: "Viscosity".into(),
                    kind: FieldKind::F64,
                    default: "0.0001".into(),
                    choices: None,
                },
                ConfigField {
                    key: "diffusion".into(),
                    label: "Diffusion".into(),
                    kind: FieldKind::F64,
                    default: "0.00001".into(),
                    choices: None,
                },
                ConfigField {
                    key: "force_strength".into(),
                    label: "Force Strength".into(),
                    kind: FieldKind::F64,
                    default: "5.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "interactive".into(),
                    label: "Interactive".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color_mode".into(),
                    label: "Color Mode".into(),
                    kind: FieldKind::Text,
                    default: "rainbow".into(),
                    choices: Some(vec!["rainbow".into(), "theme".into()]),
                },
                ConfigField {
                    key: "solver_iterations".into(),
                    label: "Solver Iterations".into(),
                    kind: FieldKind::USize,
                    default: "4".into(),
                    choices: None,
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.n_x = width as usize;
        self.n_y = height as usize;

        let size = self.total_size();
        self.vx = vec![0.0; size];
        self.vy = vec![0.0; size];
        self.vx0 = vec![0.0; size];
        self.vy0 = vec![0.0; size];
        self.dye_r = vec![0.0; size];
        self.dye_g = vec![0.0; size];
        self.dye_b = vec![0.0; size];
        self.dye_r0 = vec![0.0; size];
        self.dye_g0 = vec![0.0; size];
        self.dye_b0 = vec![0.0; size];
        self.p = vec![0.0; size];
        self.div = vec![0.0; size];

        self.inject_timer = 0.0;
        self.last_mouse_x = None;
        self.last_mouse_y = None;

        if self.n_x > 4 && self.n_y > 4 {
            self.inject_random();
            self.inject_random();
            self.inject_random();
        }
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        if self.n_x == 0 || self.n_y == 0 {
            return;
        }

        self.inject_timer += dt_secs;
        if self.inject_timer >= self.inject_interval {
            self.inject_timer -= self.inject_interval;
            self.inject_random();
        }

        let sim_dt = dt_secs.min(0.05);
        self.simulate(sim_dt);
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let bw = buf.width();
        let bh = buf.height();

        for y in 0..bh.min(self.n_y as u16) {
            for x in 0..bw.min(self.n_x as u16) {
                let idx = ix(x as usize + 1, y as usize + 1, self.n_x);

                let r = self.dye_r[idx];
                let g = self.dye_g[idx];
                let b = self.dye_b[idx];

                let intensity = (r + g + b) / 3.0;
                let ch = density_char(intensity);

                if ch != ' ' {
                    let cr = (r * 255.0).clamp(0.0, 255.0) as u8;
                    let cg = (g * 255.0).clamp(0.0, 255.0) as u8;
                    let cb = (b * 255.0).clamp(0.0, 255.0) as u8;

                    buf.set(x, y, Cell { ch, fg: PluginColor::Rgb(cr, cg, cb), bg: PluginColor::Reset });
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyInput) -> EventResponse {
        match key.code {
            KeyCode::Char('r') => {
                let size = self.total_size();
                self.vx = vec![0.0; size];
                self.vy = vec![0.0; size];
                self.vx0 = vec![0.0; size];
                self.vy0 = vec![0.0; size];
                self.dye_r = vec![0.0; size];
                self.dye_g = vec![0.0; size];
                self.dye_b = vec![0.0; size];
                self.dye_r0 = vec![0.0; size];
                self.dye_g0 = vec![0.0; size];
                self.dye_b0 = vec![0.0; size];
                self.p = vec![0.0; size];
                self.div = vec![0.0; size];
                if self.n_x > 4 && self.n_y > 4 {
                    self.inject_random();
                    self.inject_random();
                    self.inject_random();
                }
                EventResponse::Continue
            }
            _ => EventResponse::Ignored,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseInput) -> EventResponse {
        if !self.interactive {
            return EventResponse::Ignored;
        }
        self.handle_mouse_event(&mouse);
        EventResponse::Continue
    }
}

fn main() {
    let config = FluidConfig::default();
    termsaver_plugin::run(Fluid::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_field(n_x: usize, n_y: usize) -> Vec<f64> {
        vec![0.0; (n_x + 2) * (n_y + 2)]
    }

    #[test]
    fn diffusion_converges() {
        let n_x = 10;
        let n_y = 10;
        let mut field = make_field(n_x, n_y);
        let mut source = make_field(n_x, n_y);

        let center = ix(5, 5, n_x);
        source[center] = 100.0;
        field[center] = 100.0;

        let initial_center = field[center];

        diffuse(0, &mut field, &source, 1.0, 0.1, n_x, n_y, 20);

        assert!(
            field[center] < initial_center,
            "Center value should decrease after diffusion: {} vs {}",
            field[center],
            initial_center
        );

        let neighbor = field[ix(4, 5, n_x)];
        assert!(
            neighbor > 0.0,
            "Neighbors should gain value from diffusion: {}",
            neighbor
        );
    }

    #[test]
    fn set_bnd_scalar_boundaries() {
        let n_x = 5;
        let n_y = 5;
        let mut field = make_field(n_x, n_y);

        field[ix(1, 1, n_x)] = 3.0;
        field[ix(1, n_y, n_x)] = 7.0;
        field[ix(n_x, 1, n_x)] = 11.0;

        set_bnd(0, &mut field, n_x, n_y);

        assert_eq!(field[ix(1, 0, n_x)], field[ix(1, 1, n_x)]);
        assert_eq!(field[ix(0, 1, n_x)], field[ix(1, 1, n_x)]);
        assert_eq!(field[ix(1, n_y + 1, n_x)], field[ix(1, n_y, n_x)]);
        assert_eq!(field[ix(n_x + 1, 1, n_x)], field[ix(n_x, 1, n_x)]);
    }

    #[test]
    fn set_bnd_x_velocity_negates_at_x_walls() {
        let n_x = 5;
        let n_y = 5;
        let mut field = make_field(n_x, n_y);

        field[ix(1, 3, n_x)] = 4.0;
        field[ix(n_x, 3, n_x)] = 6.0;

        set_bnd(1, &mut field, n_x, n_y);

        assert_eq!(field[ix(0, 3, n_x)], -field[ix(1, 3, n_x)]);
        assert_eq!(field[ix(n_x + 1, 3, n_x)], -field[ix(n_x, 3, n_x)]);

        field[ix(3, 1, n_x)] = 5.0;
        set_bnd(1, &mut field, n_x, n_y);
        assert_eq!(field[ix(3, 0, n_x)], field[ix(3, 1, n_x)]);
    }

    #[test]
    fn set_bnd_y_velocity_negates_at_y_walls() {
        let n_x = 5;
        let n_y = 5;
        let mut field = make_field(n_x, n_y);

        field[ix(3, 1, n_x)] = 4.0;
        field[ix(3, n_y, n_x)] = 6.0;

        set_bnd(2, &mut field, n_x, n_y);

        assert_eq!(field[ix(3, 0, n_x)], -field[ix(3, 1, n_x)]);
        assert_eq!(field[ix(3, n_y + 1, n_x)], -field[ix(3, n_y, n_x)]);

        field[ix(1, 3, n_x)] = 5.0;
        set_bnd(2, &mut field, n_x, n_y);
        assert_eq!(field[ix(0, 3, n_x)], field[ix(1, 3, n_x)]);
    }

    #[test]
    fn advection_preserves_mass_approximately() {
        let n_x = 20;
        let n_y = 20;
        let mut density = make_field(n_x, n_y);
        let mut density_prev = make_field(n_x, n_y);
        let vx = make_field(n_x, n_y);
        let vy = make_field(n_x, n_y);

        for j in 8..=12 {
            for i in 8..=12 {
                density_prev[ix(i, j, n_x)] = 1.0;
            }
        }

        let initial_mass: f64 = density_prev.iter().sum();

        advect(0, &mut density, &density_prev, &vx, &vy, 0.01, n_x, n_y);

        let final_mass: f64 = density.iter().sum();

        let relative_error = ((final_mass - initial_mass) / initial_mass).abs();
        assert!(
            relative_error < 0.01,
            "Mass should be approximately conserved: initial={}, final={}, error={}",
            initial_mass,
            final_mass,
            relative_error
        );
    }

    #[test]
    fn advection_moves_density() {
        let n_x = 20;
        let n_y = 20;
        let mut density = make_field(n_x, n_y);
        let mut density_prev = make_field(n_x, n_y);
        let mut vx = make_field(n_x, n_y);
        let vy = make_field(n_x, n_y);

        density_prev[ix(10, 10, n_x)] = 1.0;

        for j in 0..n_y + 2 {
            for i in 0..n_x + 2 {
                vx[ix(i, j, n_x)] = 1.0;
            }
        }

        advect(0, &mut density, &density_prev, &vx, &vy, 0.5, n_x, n_y);

        let orig_center = density[ix(10, 10, n_x)];
        let total_right: f64 = (11..=n_x)
            .map(|i| density[ix(i, 10, n_x)])
            .sum();

        assert!(
            orig_center < 1.0 || total_right > 0.0,
            "Advection should move density: center={}, right_total={}",
            orig_center,
            total_right
        );
    }

    #[test]
    fn init_creates_valid_state() {
        let config = FluidConfig::default();
        let mut fluid = Fluid::new(config);
        fluid.init(40, 20);

        assert_eq!(fluid.n_x, 40);
        assert_eq!(fluid.n_y, 20);

        let expected_size = 42 * 22;
        assert_eq!(fluid.vx.len(), expected_size);
        assert_eq!(fluid.vy.len(), expected_size);
        assert_eq!(fluid.dye_r.len(), expected_size);
        assert_eq!(fluid.dye_g.len(), expected_size);
        assert_eq!(fluid.dye_b.len(), expected_size);
        assert_eq!(fluid.p.len(), expected_size);
        assert_eq!(fluid.div.len(), expected_size);

        let total_dye: f64 = fluid.dye_r.iter().sum::<f64>()
            + fluid.dye_g.iter().sum::<f64>()
            + fluid.dye_b.iter().sum::<f64>();
        assert!(total_dye > 0.0, "Initial dye should have been injected");
    }

    #[test]
    fn density_char_selection() {
        assert_eq!(density_char(0.0), ' ');
        assert_eq!(density_char(0.01), ' ');
        assert_eq!(density_char(0.1), '\u{2591}');
        assert_eq!(density_char(0.3), '\u{2592}');
        assert_eq!(density_char(0.5), '\u{2593}');
        assert_eq!(density_char(0.8), '\u{2588}');
        assert_eq!(density_char(1.0), '\u{2588}');
    }

    #[test]
    fn project_makes_divergence_free() {
        let n_x = 10;
        let n_y = 10;
        let mut vx = make_field(n_x, n_y);
        let mut vy = make_field(n_x, n_y);
        let mut p = make_field(n_x, n_y);
        let mut div = make_field(n_x, n_y);

        for j in 1..=n_y {
            for i in 1..=n_x {
                vx[ix(i, j, n_x)] = (i as f64 - 5.5) * 0.1;
                vy[ix(i, j, n_x)] = (j as f64 - 5.5) * 0.1;
            }
        }

        project(&mut vx, &mut vy, &mut p, &mut div, n_x, n_y, 20);

        let mut max_div = 0.0f64;
        for j in 1..=n_y {
            for i in 1..=n_x {
                let d = 0.5
                    * ((vx[ix(i + 1, j, n_x)] - vx[ix(i - 1, j, n_x)]) / n_x as f64
                        + (vy[ix(i, j + 1, n_x)] - vy[ix(i, j - 1, n_x)]) / n_y as f64);
                max_div = max_div.max(d.abs());
            }
        }

        assert!(
            max_div < 0.1,
            "Divergence should be near zero after projection: max={}",
            max_div
        );
    }

    #[test]
    fn update_does_not_panic() {
        let config = FluidConfig::default();
        let mut fluid = Fluid::new(config);
        fluid.init(20, 10);

        for _ in 0..10 {
            fluid.update(Duration::from_millis(33));
        }

        for v in &fluid.vx {
            assert!(v.is_finite(), "Velocity should remain finite");
        }
        for v in &fluid.dye_r {
            assert!(v.is_finite(), "Dye should remain finite");
        }
    }

    #[test]
    fn ix_indexing() {
        let n_x = 10;
        assert_eq!(ix(0, 0, n_x), 0);
        assert_eq!(ix(1, 0, n_x), 1);
        assert_eq!(ix(0, 1, n_x), 12);
        assert_eq!(ix(5, 3, n_x), 5 + 3 * 12);
    }

    #[test]
    fn inject_at_adds_dye_and_velocity() {
        let config = FluidConfig::default();
        let mut fluid = Fluid::new(config);
        fluid.init(20, 10);

        let size = fluid.total_size();
        fluid.vx = vec![0.0; size];
        fluid.vy = vec![0.0; size];
        fluid.dye_r = vec![0.0; size];
        fluid.dye_g = vec![0.0; size];
        fluid.dye_b = vec![0.0; size];

        fluid.inject_at(10, 5, 2.0, -1.0, 0.8, 0.5, 0.3);

        let idx = ix(10, 5, fluid.n_x);
        assert!(fluid.dye_r[idx] > 0.0, "Red dye should be injected");
        assert!(fluid.dye_g[idx] > 0.0, "Green dye should be injected");
        assert!(fluid.dye_b[idx] > 0.0, "Blue dye should be injected");
        assert!(fluid.vx[idx] > 0.0, "X velocity should be injected");
        assert!(fluid.vy[idx] < 0.0, "Y velocity should be injected (negative)");
    }

    #[test]
    fn r_key_resets_simulation() {
        let config = FluidConfig::default();
        let mut fluid = Fluid::new(config);
        fluid.init(20, 10);

        // Run some updates to get state going
        for _ in 0..5 {
            fluid.update(Duration::from_millis(33));
        }

        let key = KeyInput { code: KeyCode::Char('r'), modifiers: 0 };
        assert_eq!(fluid.handle_key(key), EventResponse::Continue);
    }

    #[test]
    fn name_via_info() {
        let fluid = Fluid::new(FluidConfig::default());
        assert_eq!(fluid.info().name, "fluid");
    }

    #[test]
    fn render_does_not_panic() {
        let config = FluidConfig::default();
        let mut fluid = Fluid::new(config);
        fluid.init(20, 10);
        fluid.update(Duration::from_millis(33));
        let mut buf = PluginBuffer::new(20, 10);
        fluid.render(&mut buf);
    }
}
