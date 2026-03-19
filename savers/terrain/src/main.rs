use std::time::Duration;

use phosphor_plugin::protocol::KeyCode;
use phosphor_plugin::{
    Cell, ConfigField, EventResponse, FieldKind, KeyInput, PluginBuffer, PluginColor, PluginInfo,
    ScreensaverPlugin,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct TerrainConfig {
    speed: f64,
    altitude: f64,
    draw_distance: f64,
    palette: String,
    fog: bool,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            altitude: 80.0,
            draw_distance: 300.0,
            palette: "earth".into(),
            fog: true,
        }
    }
}

/// Permutation table size for value noise.
const PERM_SIZE: usize = 256;

/// Number of octaves for fractal noise layering.
const OCTAVES: usize = 4;

/// Terrain height thresholds for biome selection.
const WATER_THRESHOLD: f64 = 0.3;
const GRASS_THRESHOLD: f64 = 0.5;
const ROCK_THRESHOLD: f64 = 0.75;

struct Terrain {
    width: u16,
    height: u16,
    cam_x: f64,
    cam_z: f64,
    strafe_x: f64,
    altitude_offset: f64,
    perm: [u8; PERM_SIZE],
    config: TerrainConfig,
}

impl Terrain {
    fn new(config: TerrainConfig) -> Self {
        Self {
            width: 80,
            height: 24,
            cam_x: 0.0,
            cam_z: 0.0,
            strafe_x: 0.0,
            altitude_offset: 0.0,
            perm: Self::build_permutation_table(),
            config,
        }
    }

    /// Build a shuffled permutation table for noise hashing.
    fn build_permutation_table() -> [u8; PERM_SIZE] {
        let mut perm = [0u8; PERM_SIZE];
        for (i, slot) in perm.iter_mut().enumerate() {
            *slot = i as u8;
        }
        // Fisher-Yates shuffle using a simple LCG seeded deterministically.
        let mut seed: u64 = 0xDEAD_BEEF_CAFE_1234;
        for i in (1..PERM_SIZE).rev() {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (seed >> 33) as usize % (i + 1);
            perm.swap(i, j);
        }
        perm
    }

    /// Hash two integer coordinates into a pseudo-random value in [0, 1].
    fn hash(&self, ix: i32, iy: i32) -> f64 {
        let x_idx = (ix & 0xFF) as usize;
        let y_idx = (iy & 0xFF) as usize;
        let h = self.perm[(self.perm[x_idx] as usize ^ y_idx) & 0xFF];
        h as f64 / 255.0
    }

    /// Smoothstep interpolation for noise blending.
    fn smoothstep(t: f64) -> f64 {
        t * t * (3.0 - 2.0 * t)
    }

    /// 2D value noise evaluated at continuous coordinates.
    /// Returns a value in [0, 1].
    fn noise2d(&self, x: f64, y: f64) -> f64 {
        let ix = x.floor() as i32;
        let iy = y.floor() as i32;
        let fx = x - x.floor();
        let fy = y - y.floor();

        let sx = Self::smoothstep(fx);
        let sy = Self::smoothstep(fy);

        let v00 = self.hash(ix, iy);
        let v10 = self.hash(ix + 1, iy);
        let v01 = self.hash(ix, iy + 1);
        let v11 = self.hash(ix + 1, iy + 1);

        let top = v00 + sx * (v10 - v00);
        let bottom = v01 + sx * (v11 - v01);

        top + sy * (bottom - top)
    }

    /// Fractal Brownian Motion: layer multiple octaves of value noise.
    /// Returns a value in [0, 1].
    fn fbm(&self, x: f64, y: f64) -> f64 {
        let mut value = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut max_amplitude = 0.0;

        for _ in 0..OCTAVES {
            value += amplitude * self.noise2d(x * frequency, y * frequency);
            max_amplitude += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }

        value / max_amplitude
    }

    /// Get terrain color palette based on configured palette name.
    fn biome_style(&self, height: f64, fog_factor: f64) -> (char, PluginColor) {
        let (ch, base_color) = match self.config.palette.as_str() {
            "alien" => {
                if height < WATER_THRESHOLD {
                    ('~', PluginColor::Rgb(128, 0, 255)) // purple water
                } else if height < GRASS_THRESHOLD {
                    ('.', PluginColor::Rgb(0, 255, 255)) // cyan vegetation
                } else if height < ROCK_THRESHOLD {
                    ('^', PluginColor::Rgb(255, 0, 255)) // magenta rock
                } else {
                    ('*', PluginColor::Rgb(255, 255, 0)) // yellow peaks
                }
            }
            "mono" => {
                if height < WATER_THRESHOLD {
                    ('~', PluginColor::Rgb(100, 100, 100))
                } else if height < GRASS_THRESHOLD {
                    ('.', PluginColor::Rgb(160, 160, 160))
                } else if height < ROCK_THRESHOLD {
                    ('^', PluginColor::Rgb(200, 200, 200))
                } else {
                    ('*', PluginColor::Rgb(255, 255, 255))
                }
            }
            // "earth" and default
            _ => {
                if height < WATER_THRESHOLD {
                    ('~', PluginColor::Rgb(30, 90, 200)) // blue water
                } else if height < GRASS_THRESHOLD {
                    ('.', PluginColor::Rgb(34, 139, 34)) // green grass
                } else if height < ROCK_THRESHOLD {
                    ('^', PluginColor::Rgb(139, 137, 137)) // gray rock
                } else {
                    ('*', PluginColor::Rgb(240, 240, 255)) // white snow
                }
            }
        };

        if self.config.fog {
            let color = Self::apply_fog(base_color, fog_factor);
            (ch, color)
        } else {
            (ch, base_color)
        }
    }

    /// Blend a color toward dark gray/black based on fog factor (0 = no fog, 1 = fully fogged).
    fn apply_fog(color: PluginColor, fog_factor: f64) -> PluginColor {
        let fog = fog_factor.clamp(0.0, 1.0);
        match color {
            PluginColor::Rgb(r, g, b) => {
                // Fog target: very dark gray
                let fr = (r as f64 * (1.0 - fog) + 20.0 * fog) as u8;
                let fg = (g as f64 * (1.0 - fog) + 20.0 * fog) as u8;
                let fb = (b as f64 * (1.0 - fog) + 20.0 * fog) as u8;
                PluginColor::Rgb(fr, fg, fb)
            }
            _ => color,
        }
    }
}

impl ScreensaverPlugin for Terrain {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "terrain".into(),
            description: "Voxel-space terrain flyover with procedural value noise".into(),
            interactive: true,
            key_bindings: vec![
                ("←/→".into(), "Strafe camera left/right".into()),
                ("↑/↓".into(), "Adjust altitude".into()),
            ],
            features: vec![
                "Procedural fractal noise terrain".into(),
                "Biome-based coloring".into(),
                "Distance fog".into(),
                "Arrow key camera control".into(),
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
                    key: "altitude".into(),
                    label: "Altitude".into(),
                    kind: FieldKind::F64,
                    default: "80.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "draw_distance".into(),
                    label: "Draw Distance".into(),
                    kind: FieldKind::F64,
                    default: "300.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "palette".into(),
                    label: "Palette".into(),
                    kind: FieldKind::Text,
                    default: "earth".into(),
                    choices: Some(vec!["earth".into(), "alien".into(), "desert".into()]),
                },
                ConfigField {
                    key: "fog".into(),
                    label: "Fog".into(),
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
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        self.cam_x += self.strafe_x * dt_secs;
        self.strafe_x *= 0.92;
        self.cam_z += self.config.speed * 10.0 * dt_secs;
    }

    fn handle_key(&mut self, key: KeyInput) -> EventResponse {
        match key.code {
            KeyCode::Left => {
                self.strafe_x -= 20.0;
                EventResponse::Continue
            }
            KeyCode::Right => {
                self.strafe_x += 20.0;
                EventResponse::Continue
            }
            KeyCode::Up => {
                self.altitude_offset += 10.0;
                EventResponse::Continue
            }
            KeyCode::Down => {
                self.altitude_offset =
                    (self.altitude_offset - 10.0).max(10.0 - self.config.altitude);
                EventResponse::Continue
            }
            _ => EventResponse::Ignored,
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let w = buf.width() as usize;
        let h = buf.height() as usize;

        if w == 0 || h == 0 {
            return;
        }

        let half_w = w as f64 / 2.0;
        let fov = 1.0; // field of view scale factor

        // For each terminal column, do a voxel-space column raycast
        for col in 0..w {
            // Ray angle for this column: spread across the field of view
            let ray_angle = (col as f64 - half_w) / half_w * fov;

            // Direction vector for the ray (forward is +Z)
            let dir_x = ray_angle;
            let dir_z = 1.0;
            let dir_len = (dir_x * dir_x + dir_z * dir_z).sqrt();
            let norm_x = dir_x / dir_len;
            let norm_z = dir_z / dir_len;

            // Track the highest screen row drawn so far (front-to-back)
            let mut max_drawn_y = h as i32;

            // Step through depth values from near to far
            let near = 1.0;
            let far = self.config.draw_distance;
            let num_steps = 200;
            let step_size = (far - near) / num_steps as f64;

            for step in 0..num_steps {
                let depth = near + step as f64 * step_size;

                // World position at this depth along the ray
                let world_x = self.cam_x + norm_x * depth;
                let world_z = self.cam_z + norm_z * depth;

                // Evaluate terrain height via fractal noise
                let noise_scale = 0.02;
                let terrain_height = self.fbm(world_x * noise_scale, world_z * noise_scale);

                // Project terrain height to screen row
                let height_above_terrain =
                    (self.config.altitude + self.altitude_offset) * (1.0 - terrain_height);
                let projected_y = (height_above_terrain / depth) * (h as f64 / 2.0);
                let screen_y = (h as f64 / 2.0 - projected_y) as i32;

                // Clamp to valid screen range
                let draw_top = screen_y.max(0);
                let draw_bottom = max_drawn_y;

                if draw_top >= draw_bottom {
                    continue;
                }

                // Distance-based fog factor: 0 at near, 1 at far
                let fog_factor = (depth - near) / (far - near);

                // Get biome character and color
                let (ch, color) = self.biome_style(terrain_height, fog_factor);

                // Fill the column from draw_top to draw_bottom
                for row in draw_top..draw_bottom {
                    if row >= 0 && (row as u16) < buf.height() && (col as u16) < buf.width() {
                        buf.set(
                            col as u16,
                            row as u16,
                            Cell {
                                ch,
                                fg: color,
                                bg: PluginColor::Reset,
                            },
                        );
                    }
                }

                max_drawn_y = draw_top;

                // If we've filled to the top of the screen, no need to continue
                if max_drawn_y <= 0 {
                    break;
                }
            }
        }
    }
}

fn main() {
    let config = TerrainConfig::default();
    phosphor_plugin::run(Terrain::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_terrain() -> Terrain {
        Terrain::new(TerrainConfig::default())
    }

    #[test]
    fn noise_in_range() {
        let t = make_terrain();
        // Sample noise at many points, all should be in [0, 1]
        for i in 0..100 {
            for j in 0..100 {
                let x = i as f64 * 0.37 - 20.0;
                let y = j as f64 * 0.41 - 15.0;
                let val = t.noise2d(x, y);
                assert!(
                    (0.0..=1.0).contains(&val),
                    "noise2d({x}, {y}) = {val} out of range"
                );
            }
        }
    }

    #[test]
    fn fbm_in_range() {
        let t = make_terrain();
        for i in 0..50 {
            for j in 0..50 {
                let x = i as f64 * 1.3 - 30.0;
                let y = j as f64 * 1.7 - 40.0;
                let val = t.fbm(x, y);
                assert!(
                    (0.0..=1.0).contains(&val),
                    "fbm({x}, {y}) = {val} out of range"
                );
            }
        }
    }

    #[test]
    fn different_coords_produce_different_values() {
        let t = make_terrain();
        let v1 = t.noise2d(0.0, 0.0);
        let v2 = t.noise2d(7.5, 13.3);
        let v3 = t.noise2d(-42.0, 99.9);
        // It's astronomically unlikely these are all exactly equal
        assert!(
            !(v1 == v2 && v2 == v3),
            "Different coordinates should produce different noise values"
        );
    }

    #[test]
    fn init_creates_valid_state() {
        let mut t = make_terrain();
        t.init(120, 40);
        assert_eq!(t.width, 120);
        assert_eq!(t.height, 40);
        assert_eq!(t.cam_z, 0.0);
        assert_eq!(t.cam_x, 0.0);
        // Permutation table should be populated
        let mut has_nonzero = false;
        for &v in &t.perm {
            if v != 0 {
                has_nonzero = true;
                break;
            }
        }
        assert!(
            has_nonzero,
            "Permutation table should contain non-zero entries"
        );
    }

    #[test]
    fn camera_advances_on_update() {
        let mut t = make_terrain();
        t.init(80, 24);
        let initial_z = t.cam_z;
        t.update(Duration::from_millis(100));
        assert!(
            t.cam_z > initial_z,
            "Camera should advance forward: {} should be > {}",
            t.cam_z,
            initial_z
        );
    }

    #[test]
    fn camera_speed_scales_with_dt() {
        let mut t1 = make_terrain();
        let mut t2 = make_terrain();
        t1.init(80, 24);
        t2.init(80, 24);

        t1.update(Duration::from_millis(100));
        t2.update(Duration::from_millis(200));

        let ratio = t2.cam_z / t1.cam_z;
        assert!(
            (ratio - 2.0).abs() < 0.001,
            "Double dt should give double movement, got ratio {ratio}"
        );
    }

    #[test]
    fn permutation_table_is_permutation() {
        let t = make_terrain();
        let mut counts = [0u32; PERM_SIZE];
        for &v in &t.perm {
            counts[v as usize] += 1;
        }
        for (i, &count) in counts.iter().enumerate() {
            assert_eq!(
                count, 1,
                "Value {i} appears {count} times, expected exactly 1"
            );
        }
    }

    #[test]
    fn smoothstep_boundaries() {
        assert!((Terrain::smoothstep(0.0) - 0.0).abs() < f64::EPSILON);
        assert!((Terrain::smoothstep(1.0) - 1.0).abs() < f64::EPSILON);
        assert!((Terrain::smoothstep(0.5) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn fog_at_zero_preserves_color() {
        let color = PluginColor::Rgb(100, 150, 200);
        let result = Terrain::apply_fog(color, 0.0);
        assert_eq!(result, PluginColor::Rgb(100, 150, 200));
    }

    #[test]
    fn fog_at_one_approaches_dark() {
        let color = PluginColor::Rgb(200, 200, 200);
        let result = Terrain::apply_fog(color, 1.0);
        // Should be close to fog target (20, 20, 20)
        if let PluginColor::Rgb(r, g, b) = result {
            assert!(
                r <= 25 && g <= 25 && b <= 25,
                "Full fog should be near-black, got ({r}, {g}, {b})"
            );
        } else {
            panic!("Expected Rgb color");
        }
    }

    #[test]
    fn name_via_info() {
        let t = make_terrain();
        assert_eq!(t.info().name, "terrain");
    }

    #[test]
    fn render_does_not_panic() {
        let mut t = make_terrain();
        t.init(40, 20);
        t.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(40, 20);
        t.render(&mut buf);
    }

    #[test]
    fn arrow_keys_change_state() {
        let mut t = make_terrain();
        t.init(80, 24);

        let left = KeyInput {
            code: KeyCode::Left,
            modifiers: 0,
        };
        assert_eq!(t.handle_key(left), EventResponse::Continue);
        assert!(t.strafe_x < 0.0);

        let right = KeyInput {
            code: KeyCode::Right,
            modifiers: 0,
        };
        assert_eq!(t.handle_key(right), EventResponse::Continue);

        let up = KeyInput {
            code: KeyCode::Up,
            modifiers: 0,
        };
        assert_eq!(t.handle_key(up), EventResponse::Continue);
        assert!(t.altitude_offset > 0.0);

        let down = KeyInput {
            code: KeyCode::Down,
            modifiers: 0,
        };
        assert_eq!(t.handle_key(down), EventResponse::Continue);
    }

    #[test]
    fn unhandled_key_returns_ignored() {
        let mut t = make_terrain();
        let key = KeyInput {
            code: KeyCode::Char('z'),
            modifiers: 0,
        };
        assert_eq!(t.handle_key(key), EventResponse::Ignored);
    }

    #[test]
    fn info_is_interactive() {
        let t = make_terrain();
        assert!(t.info().interactive);
    }
}
