use std::time::Duration;

use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct CubeConfig {
    speed: f64,
    color: String,
    perspective: bool,
    auto_axes: bool,
}

impl Default for CubeConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            color: "cyan".into(),
            perspective: true,
            auto_axes: true,
        }
    }
}

// 8 vertices of a unit cube at (±1, ±1, ±1)
const VERTICES: [[f64; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];

// 12 edges connecting adjacent vertices
const EDGES: [[usize; 2]; 12] = [
    // front face
    [0, 1],
    [1, 2],
    [2, 3],
    [3, 0],
    // back face
    [4, 5],
    [5, 6],
    [6, 7],
    [7, 4],
    // connecting edges
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

fn braille_bit(dx: usize, dy: usize) -> u8 {
    match (dx, dy) {
        (0, 0) => 1 << 0,
        (0, 1) => 1 << 1,
        (0, 2) => 1 << 2,
        (1, 0) => 1 << 3,
        (1, 1) => 1 << 4,
        (1, 2) => 1 << 5,
        (0, 3) => 1 << 6,
        (1, 3) => 1 << 7,
        _ => 0,
    }
}

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

fn rotate_x(v: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c]
}

fn rotate_y(v: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
}

fn rotate_z(v: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
}

fn rotate_vertex(v: [f64; 3], ax: f64, ay: f64, az: f64) -> [f64; 3] {
    rotate_z(rotate_y(rotate_x(v, ax), ay), az)
}

/// Project a 3D point to 2D dot-space coordinates.
/// Returns (dot_x, dot_y) in the braille sub-pixel grid.
fn project(v: [f64; 3], dot_w: usize, dot_h: usize, perspective: bool) -> (f64, f64) {
    let cx = dot_w as f64 / 2.0;
    let cy = dot_h as f64 / 2.0;
    let scale = cx.min(cy) * 0.35;

    let (sx, sy) = if perspective {
        let camera_z: f64 = -5.0;
        let fov: f64 = 5.0;
        let dz = v[2] - camera_z;
        let factor = fov / dz;
        (v[0] * factor, v[1] * factor)
    } else {
        (v[0], v[1])
    };

    (cx + sx * scale, cy + sy * scale)
}

/// Bresenham line drawing into the braille dot grid.
#[allow(clippy::too_many_arguments)]
fn draw_line(
    grid: &mut [u8],
    edge_grid: &mut [u8],
    dot_w: usize,
    dot_h: usize,
    cell_cols: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    edge_idx: u8,
) {
    let mut ix0 = x0.round() as isize;
    let mut iy0 = y0.round() as isize;
    let ix1 = x1.round() as isize;
    let iy1 = y1.round() as isize;

    let dx = (ix1 - ix0).abs();
    let dy = -(iy1 - iy0).abs();
    let sx: isize = if ix0 < ix1 { 1 } else { -1 };
    let sy: isize = if iy0 < iy1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        // Plot point
        if ix0 >= 0 && iy0 >= 0 && (ix0 as usize) < dot_w && (iy0 as usize) < dot_h {
            let px = ix0 as usize;
            let py = iy0 as usize;
            let cell_x = px / 2;
            let cell_y = py / 4;
            if cell_x < cell_cols {
                let sub_x = px % 2;
                let sub_y = py % 4;
                let idx = cell_y * cell_cols + cell_x;
                grid[idx] |= braille_bit(sub_x, sub_y);
                edge_grid[idx] = edge_idx;
            }
        }

        if ix0 == ix1 && iy0 == iy1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            ix0 += sx;
        }
        if e2 <= dx {
            err += dx;
            iy0 += sy;
        }
    }
}

fn color_for_name(name: &str) -> PluginColor {
    match name {
        "white" => PluginColor::Rgb(255, 255, 255),
        "green" => PluginColor::Rgb(0, 255, 128),
        _ => PluginColor::Rgb(0, 220, 255), // cyan (default)
    }
}

fn rainbow_color(edge_idx: u8) -> PluginColor {
    let hue = (edge_idx as f64 / 12.0) * 360.0;
    let (r, g, b) = hsv_to_rgb(hue % 360.0, 1.0, 1.0);
    PluginColor::Rgb(r, g, b)
}

struct RotatingCube {
    width: u16,
    height: u16,
    dot_width: usize,
    dot_height: usize,
    angle_x: f64,
    angle_y: f64,
    angle_z: f64,
    t: f64,
    config: CubeConfig,
}

impl RotatingCube {
    fn new(config: CubeConfig) -> Self {
        Self {
            width: 80,
            height: 24,
            dot_width: 160,
            dot_height: 96,
            angle_x: 0.0,
            angle_y: 0.0,
            angle_z: 0.0,
            t: 0.0,
            config,
        }
    }

    fn axis_speeds(&self) -> (f64, f64, f64) {
        if self.config.auto_axes {
            let wx = 0.5 + 0.5 * (self.t * 0.23 * std::f64::consts::TAU).sin();
            let wy = 0.5 + 0.5 * (self.t * 0.37 * std::f64::consts::TAU).sin();
            let wz = 0.5 + 0.5 * (self.t * 0.17 * std::f64::consts::TAU).sin();
            (wx, wy, wz)
        } else {
            (1.0, 1.0, 1.0)
        }
    }
}

impl ScreensaverPlugin for RotatingCube {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "rotating_cube".into(),
            description: "Wireframe 3D cube with Braille subpixel rendering".into(),
            interactive: false,
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Braille subpixel rendering".into(),
                "Perspective and orthographic projection".into(),
                "Auto-varying rotation axes".into(),
                "Rainbow and solid color modes".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "speed".into(),
                    label: "Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "color".into(),
                    label: "Color".into(),
                    kind: FieldKind::Text,
                    default: "cyan".into(),
                    choices: Some(vec![
                        "white".into(),
                        "cyan".into(),
                        "green".into(),
                        "rainbow".into(),
                    ]),
                },
                ConfigField {
                    key: "perspective".into(),
                    label: "Perspective".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
                ConfigField {
                    key: "auto_axes".into(),
                    label: "Auto Axes".into(),
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
        self.dot_width = width as usize * 2;
        self.dot_height = height as usize * 4;
        self.angle_x = 0.0;
        self.angle_y = 0.0;
        self.angle_z = 0.0;
        self.t = 0.0;
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let speed = self.config.speed;
        let (wx, wy, wz) = self.axis_speeds();

        self.angle_x += dt_secs * speed * wx;
        self.angle_y += dt_secs * speed * wy;
        self.angle_z += dt_secs * speed * wz * 0.7;
        self.t += dt_secs;
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let cell_cols = buf.width() as usize;
        let cell_rows = buf.height() as usize;

        if cell_cols == 0 || cell_rows == 0 {
            return;
        }

        let dot_w = cell_cols * 2;
        let dot_h = cell_rows * 4;
        let grid_size = cell_cols * cell_rows;
        let mut braille_grid: Vec<u8> = vec![0; grid_size];
        let mut edge_grid: Vec<u8> = vec![0; grid_size];

        // Rotate all vertices
        let rotated: Vec<[f64; 3]> = VERTICES
            .iter()
            .map(|&v| rotate_vertex(v, self.angle_x, self.angle_y, self.angle_z))
            .collect();

        // Project to dot-space
        let projected: Vec<(f64, f64)> = rotated
            .iter()
            .map(|&v| project(v, dot_w, dot_h, self.config.perspective))
            .collect();

        // Draw all 12 edges
        for (ei, edge) in EDGES.iter().enumerate() {
            let (x0, y0) = projected[edge[0]];
            let (x1, y1) = projected[edge[1]];
            draw_line(
                &mut braille_grid,
                &mut edge_grid,
                dot_w,
                dot_h,
                cell_cols,
                x0,
                y0,
                x1,
                y1,
                ei as u8,
            );
        }

        // Render braille cells
        let is_rainbow = self.config.color == "rainbow";
        let solid_color = color_for_name(&self.config.color);

        for cy in 0..cell_rows {
            for cx in 0..cell_cols {
                let idx = cy * cell_cols + cx;
                let pattern = braille_grid[idx];
                if pattern == 0 {
                    continue;
                }

                let ch = char::from_u32(0x2800 + pattern as u32).unwrap_or(' ');
                let fg = if is_rainbow {
                    rainbow_color(edge_grid[idx])
                } else {
                    solid_color
                };

                buf.set(
                    cx as u16,
                    cy as u16,
                    Cell {
                        ch,
                        fg,
                        bg: PluginColor::Reset,
                    },
                );
            }
        }
    }
}

fn main() {
    let config = CubeConfig::default();
    phosphor_plugin::run(RotatingCube::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cube() -> RotatingCube {
        RotatingCube::new(CubeConfig::default())
    }

    // --- Geometry ---

    #[test]
    fn vertex_count() {
        assert_eq!(VERTICES.len(), 8);
    }

    #[test]
    fn edge_count() {
        assert_eq!(EDGES.len(), 12);
    }

    #[test]
    fn edge_indices_valid() {
        for edge in &EDGES {
            assert!(edge[0] < 8, "edge start index {} out of range", edge[0]);
            assert!(edge[1] < 8, "edge end index {} out of range", edge[1]);
        }
    }

    // --- Rotation ---

    #[test]
    fn rotation_identity() {
        let v = [1.0, 2.0, 3.0];
        let r = rotate_vertex(v, 0.0, 0.0, 0.0);
        assert!((r[0] - v[0]).abs() < 1e-10);
        assert!((r[1] - v[1]).abs() < 1e-10);
        assert!((r[2] - v[2]).abs() < 1e-10);
    }

    #[test]
    fn rotation_90_x() {
        let v = [0.0, 1.0, 0.0];
        let r = rotate_x(v, std::f64::consts::FRAC_PI_2);
        assert!((r[0] - 0.0).abs() < 1e-10);
        assert!((r[1] - 0.0).abs() < 1e-10);
        assert!((r[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn rotation_90_y() {
        let v = [1.0, 0.0, 0.0];
        let r = rotate_y(v, std::f64::consts::FRAC_PI_2);
        assert!((r[0] - 0.0).abs() < 1e-10);
        assert!((r[1] - 0.0).abs() < 1e-10);
        assert!((r[2] - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn rotation_90_z() {
        let v = [1.0, 0.0, 0.0];
        let r = rotate_z(v, std::f64::consts::FRAC_PI_2);
        assert!((r[0] - 0.0).abs() < 1e-10);
        assert!((r[1] - 1.0).abs() < 1e-10);
        assert!((r[2] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn rotation_180() {
        let v = [1.0, 0.0, 0.0];
        let r = rotate_x(v, std::f64::consts::PI);
        assert!((r[0] - 1.0).abs() < 1e-10);
        assert!((r[1] - 0.0).abs() < 1e-10);
        assert!((r[2] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn rotation_preserves_magnitude() {
        let v: [f64; 3] = [1.0, 2.0, 3.0];
        let mag_before = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let r = rotate_vertex(v, 1.23, 2.34, 3.45);
        let mag_after = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        assert!((mag_before - mag_after).abs() < 1e-10);
    }

    // --- Projection ---

    #[test]
    fn projection_origin_to_center() {
        let (dx, dy) = project([0.0, 0.0, 0.0], 160, 96, true);
        assert!((dx - 80.0).abs() < 1.0);
        assert!((dy - 48.0).abs() < 1.0);
    }

    #[test]
    fn projection_positive_x_goes_right() {
        let (dx_center, _) = project([0.0, 0.0, 0.0], 160, 96, true);
        let (dx_right, _) = project([1.0, 0.0, 0.0], 160, 96, true);
        assert!(dx_right > dx_center);
    }

    #[test]
    fn projection_positive_y_goes_down() {
        let (_, dy_center) = project([0.0, 0.0, 0.0], 160, 96, true);
        let (_, dy_down) = project([0.0, 1.0, 0.0], 160, 96, true);
        assert!(dy_down > dy_center);
    }

    #[test]
    fn perspective_closer_is_larger() {
        let (dx_far, _) = project([1.0, 0.0, 2.0], 160, 96, true);
        let (dx_near, _) = project([1.0, 0.0, -2.0], 160, 96, true);
        let center = 80.0;
        assert!(
            (dx_near - center).abs() > (dx_far - center).abs(),
            "Nearer point should project further from center"
        );
    }

    #[test]
    fn orthographic_ignores_depth() {
        let (dx_a, dy_a) = project([1.0, 1.0, -2.0], 160, 96, false);
        let (dx_b, dy_b) = project([1.0, 1.0, 2.0], 160, 96, false);
        assert!((dx_a - dx_b).abs() < 1e-10);
        assert!((dy_a - dy_b).abs() < 1e-10);
    }

    // --- Braille ---

    #[test]
    fn braille_full_cell() {
        let mut pattern: u8 = 0;
        for dx in 0..2 {
            for dy in 0..4 {
                pattern |= braille_bit(dx, dy);
            }
        }
        assert_eq!(pattern, 0xFF);
    }

    #[test]
    fn braille_out_of_range() {
        assert_eq!(braille_bit(2, 0), 0);
        assert_eq!(braille_bit(0, 4), 0);
    }

    // --- Bresenham ---

    #[test]
    fn bresenham_horizontal_line() {
        let cell_cols = 10;
        let cell_rows = 5;
        let dot_w = cell_cols * 2;
        let dot_h = cell_rows * 4;
        let mut grid = vec![0u8; cell_cols * cell_rows];
        let mut edge = vec![0u8; cell_cols * cell_rows];
        draw_line(
            &mut grid, &mut edge, dot_w, dot_h, cell_cols, 0.0, 0.0, 19.0, 0.0, 0,
        );
        // First row of cells should have content
        let first_row_has_content = grid[..cell_cols].iter().any(|&b| b != 0);
        assert!(first_row_has_content);
    }

    #[test]
    fn bresenham_vertical_line() {
        let cell_cols = 5;
        let cell_rows = 10;
        let dot_w = cell_cols * 2;
        let dot_h = cell_rows * 4;
        let mut grid = vec![0u8; cell_cols * cell_rows];
        let mut edge = vec![0u8; cell_cols * cell_rows];
        draw_line(
            &mut grid, &mut edge, dot_w, dot_h, cell_cols, 0.0, 0.0, 0.0, 39.0, 0,
        );
        // First column of cells should have content
        let first_col_has_content = (0..cell_rows).any(|r| grid[r * cell_cols] != 0);
        assert!(first_col_has_content);
    }

    // --- Init / Update ---

    #[test]
    fn init_sets_dimensions() {
        let mut cube = make_cube();
        cube.init(80, 24);
        assert_eq!(cube.width, 80);
        assert_eq!(cube.height, 24);
        assert_eq!(cube.dot_width, 160);
        assert_eq!(cube.dot_height, 96);
    }

    #[test]
    fn init_resets_angles() {
        let mut cube = make_cube();
        cube.init(80, 24);
        cube.update(Duration::from_secs(5));
        assert!(cube.angle_x != 0.0);
        cube.init(120, 40);
        assert_eq!(cube.angle_x, 0.0);
        assert_eq!(cube.angle_y, 0.0);
        assert_eq!(cube.angle_z, 0.0);
        assert_eq!(cube.t, 0.0);
    }

    #[test]
    fn update_advances_angles() {
        let mut cube = make_cube();
        cube.init(80, 24);
        cube.update(Duration::from_millis(100));
        assert!(cube.angle_x > 0.0 || cube.angle_y > 0.0 || cube.angle_z > 0.0);
        assert!(cube.t > 0.0);
    }

    #[test]
    fn auto_axes_varies_speeds() {
        let mut cube = make_cube();
        cube.init(80, 24);
        let (wx1, wy1, wz1) = cube.axis_speeds();
        cube.t = 5.0;
        let (wx2, wy2, wz2) = cube.axis_speeds();
        let changed =
            (wx1 - wx2).abs() > 0.01 || (wy1 - wy2).abs() > 0.01 || (wz1 - wz2).abs() > 0.01;
        assert!(changed, "Auto-axis speeds should vary over time");
    }

    // --- Render ---

    #[test]
    fn render_no_panic() {
        let mut cube = make_cube();
        cube.init(80, 24);
        cube.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(80, 24);
        cube.render(&mut buf);
    }

    #[test]
    fn render_small_no_panic() {
        let mut cube = make_cube();
        cube.init(4, 2);
        cube.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(4, 2);
        cube.render(&mut buf);
    }

    #[test]
    fn render_large_no_panic() {
        let mut cube = make_cube();
        cube.init(200, 60);
        cube.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(200, 60);
        cube.render(&mut buf);
    }

    #[test]
    fn render_zero_size() {
        let mut cube = make_cube();
        cube.init(0, 0);
        let mut buf = PluginBuffer::new(0, 0);
        cube.render(&mut buf);
    }

    #[test]
    fn render_produces_braille_chars() {
        let mut cube = make_cube();
        cube.init(40, 20);
        cube.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(40, 20);
        cube.render(&mut buf);
        let has_braille = (0..40u16)
            .flat_map(|x| (0..20u16).map(move |y| (x, y)))
            .any(|(x, y)| {
                let cell = buf.get(x, y).unwrap();
                let code = cell.ch as u32;
                (0x2800..=0x28FF).contains(&code)
            });
        assert!(has_braille, "Should produce braille characters");
    }

    #[test]
    fn render_rainbow_mode() {
        let config = CubeConfig {
            color: "rainbow".into(),
            ..Default::default()
        };
        let mut cube = RotatingCube::new(config);
        cube.init(40, 20);
        cube.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(40, 20);
        cube.render(&mut buf);
        // Should have at least two distinct colors
        let mut colors = std::collections::HashSet::new();
        for x in 0..40u16 {
            for y in 0..20u16 {
                let cell = buf.get(x, y).unwrap();
                if let PluginColor::Rgb(r, g, b) = cell.fg {
                    colors.insert((r, g, b));
                }
            }
        }
        assert!(
            colors.len() >= 2,
            "Rainbow mode should produce multiple colors"
        );
    }

    #[test]
    fn render_orthographic_mode() {
        let config = CubeConfig {
            perspective: false,
            ..Default::default()
        };
        let mut cube = RotatingCube::new(config);
        cube.init(40, 20);
        cube.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(40, 20);
        cube.render(&mut buf);
        // Should still produce output
        let has_content = (0..40u16)
            .flat_map(|x| (0..20u16).map(move |y| (x, y)))
            .any(|(x, y)| buf.get(x, y).unwrap().fg != PluginColor::Reset);
        assert!(has_content, "Orthographic mode should still render");
    }

    // --- Info ---

    #[test]
    fn info_name_and_fields() {
        let cube = make_cube();
        let info = cube.info();
        assert_eq!(info.name, "rotating_cube");
        assert_eq!(info.config_fields.len(), 4);
    }
}
