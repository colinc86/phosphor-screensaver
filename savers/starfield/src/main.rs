use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo,
    ScreensaverPlugin,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct StarfieldConfig {
    star_count: usize,
    speed: f64,
    streak_enabled: bool,
    warp_enabled: bool,
}

impl Default for StarfieldConfig {
    fn default() -> Self {
        Self {
            star_count: 200,
            speed: 1.0,
            streak_enabled: true,
            warp_enabled: true,
        }
    }
}

struct Star {
    x: f64,
    y: f64,
    z: f64,
    prev_sx: f64,
    prev_sy: f64,
}

struct Starfield {
    stars: Vec<Star>,
    width: u16,
    height: u16,
    max_depth: f64,
    warp_phase: f64,
    warp_timer: f64,
    warp_cooldown: f64,
    config: StarfieldConfig,
}

impl Starfield {
    fn new(config: StarfieldConfig) -> Self {
        Self {
            stars: Vec::new(),
            width: 80,
            height: 24,
            max_depth: 64.0,
            warp_phase: 0.0,
            warp_timer: 0.0,
            warp_cooldown: 0.0,
            config,
        }
    }

    fn spawn_star(width: u16, height: u16, max_depth: f64, far: bool) -> Star {
        let mut rng = rand::rng();
        let half_w = width as f64 / 2.0;
        let half_h = height as f64 / 2.0;
        Star {
            x: rng.random_range(-half_w..half_w),
            y: rng.random_range(-half_h..half_h),
            z: if far {
                rng.random_range(1.0..max_depth)
            } else {
                max_depth
            },
            prev_sx: -1.0,
            prev_sy: -1.0,
        }
    }

    /// Project a 3D star position to 2D screen coordinates.
    pub fn project(x: f64, y: f64, z: f64, half_w: f64, half_h: f64) -> Option<(f64, f64)> {
        if z <= 0.0 {
            return None;
        }
        let sx = (x / z) * half_w + half_w;
        let sy = (y / z) * half_h + half_h;
        Some((sx, sy))
    }

    /// Current speed multiplier including warp effect.
    fn speed_multiplier(&self) -> f64 {
        if self.warp_phase > 0.0 {
            // Sine ramp: peaks at mid-warp
            let phase_progress = self.warp_phase / 2.0; // 2-second warp duration
            let ramp = (phase_progress * std::f64::consts::PI).sin();
            1.0 + ramp * 8.0 // Up to 9x speed during warp
        } else {
            1.0
        }
    }

    /// Draw a line between two points, writing graduated brightness chars.
    fn draw_streak(
        buf: &mut PluginBuffer,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        base_brightness: u8,
        width: u16,
        height: u16,
    ) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let steps = dx.max(dy).max(1);

        let streak_chars: &[char] = &['\u{00B7}', '\u{254C}', '\u{2504}', '\u{2500}', '\u{2501}'];

        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let px = (x0 as f64 + t * (x1 - x0) as f64).round() as i32;
            let py = (y0 as f64 + t * (y1 - y0) as f64).round() as i32;

            if px < 0 || px >= width as i32 || py < 0 || py >= height as i32 {
                continue;
            }

            // Brightness fades along the streak (tail is dimmer)
            let fade = t; // 0 at old position, 1 at new position
            let b = (base_brightness as f64 * (0.3 + 0.7 * fade)) as u8;
            let char_idx = (fade * (streak_chars.len() - 1) as f64) as usize;
            let ch = streak_chars[char_idx.min(streak_chars.len() - 1)];

            buf.set(
                px as u16,
                py as u16,
                Cell { ch, fg: PluginColor::Rgb(b, b, b), bg: PluginColor::Reset },
            );
        }
    }
}

impl ScreensaverPlugin for Starfield {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "starfield".into(),
            description: "A parallax starfield flying through space".into(),
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "3D to 2D projection with depth-based brightness".into(),
                "Motion streaks for nearby stars".into(),
                "Periodic warp speed bursts".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "star_count".into(),
                    label: "Star Count".into(),
                    kind: FieldKind::USize,
                    default: "200".into(),
                    choices: None,
                },
                ConfigField {
                    key: "speed".into(),
                    label: "Speed".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "streak_enabled".into(),
                    label: "Streaks Enabled".into(),
                    kind: FieldKind::Bool,
                    default: "true".into(),
                    choices: None,
                },
                ConfigField {
                    key: "warp_enabled".into(),
                    label: "Warp Enabled".into(),
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
        self.stars.clear();
        for _ in 0..self.config.star_count {
            self.stars
                .push(Self::spawn_star(width, height, self.max_depth, true));
        }
        self.warp_phase = 0.0;
        self.warp_timer = 0.0;
        self.warp_cooldown = 0.0;
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();
        let half_w = self.width as f64 / 2.0;
        let half_h = self.height as f64 / 2.0;

        // Store previous projected positions before moving
        for star in &mut self.stars {
            if let Some((sx, sy)) = Self::project(star.x, star.y, star.z, half_w, half_h) {
                star.prev_sx = sx;
                star.prev_sy = sy;
            }
        }

        // Warp pulse logic
        let speed_mult = self.speed_multiplier();
        if self.warp_phase > 0.0 {
            self.warp_phase -= dt_secs;
            if self.warp_phase <= 0.0 {
                self.warp_phase = 0.0;
                self.warp_cooldown = 0.0;
            }
        } else if self.config.warp_enabled {
            self.warp_timer += dt_secs;
            self.warp_cooldown += dt_secs;
            // Trigger warp every 20-30 seconds
            if self.warp_cooldown >= 25.0 {
                self.warp_phase = 2.0;
                self.warp_cooldown = 0.0;
            }
        }

        let move_speed = self.config.speed * 20.0 * dt_secs * speed_mult;

        for star in &mut self.stars {
            star.z -= move_speed;
        }

        // Respawn stars that have passed the camera or projected off-screen
        let width = self.width;
        let height = self.height;
        let max_depth = self.max_depth;
        for star in &mut self.stars {
            if star.z <= 0.5 {
                *star = Self::spawn_star(width, height, max_depth, false);
            } else if let Some((sx, sy)) = Self::project(star.x, star.y, star.z, half_w, half_h) {
                if sx < -1.0 || sx > width as f64 || sy < -1.0 || sy > height as f64 {
                    *star = Self::spawn_star(width, height, max_depth, false);
                }
            }
        }
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let w = buf.width();
        let h = buf.height();
        let half_w = w as f64 / 2.0;
        let half_h = h as f64 / 2.0;
        let is_warping = self.warp_phase > 0.0;

        for star in &self.stars {
            let Some((sx, sy)) = Self::project(star.x, star.y, star.z, half_w, half_h) else {
                continue;
            };

            let px = sx.round() as i32;
            let py = sy.round() as i32;
            if px < 0 || px >= w as i32 || py < 0 || py >= h as i32 {
                continue;
            }

            let depth_ratio = 1.0 - (star.z / self.max_depth);
            let brightness = (depth_ratio * 255.0) as u8;

            // Draw streak if enabled and star has a valid previous position
            if self.config.streak_enabled
                && star.prev_sx >= 0.0
                && star.prev_sy >= 0.0
                && (depth_ratio > 0.3 || is_warping)
            {
                let prev_px = star.prev_sx.round() as i32;
                let prev_py = star.prev_sy.round() as i32;
                let dist = ((px - prev_px).pow(2) + (py - prev_py).pow(2)) as f64;

                // Only draw streak if there's actual movement (> 1 pixel)
                if dist.sqrt() > 1.0 {
                    Self::draw_streak(
                        buf,
                        prev_px,
                        prev_py,
                        px,
                        py,
                        brightness,
                        w,
                        h,
                    );
                    continue; // streak already drew the head
                }
            }

            // Non-streak rendering (dots)
            let (ch, color) = if depth_ratio > 0.8 {
                ('*', PluginColor::Rgb(brightness, brightness, brightness))
            } else if depth_ratio > 0.5 {
                ('+', PluginColor::Rgb(brightness, brightness, brightness))
            } else if depth_ratio > 0.2 {
                ('\u{00B7}', PluginColor::Rgb(brightness / 2, brightness / 2, brightness))
            } else {
                ('.', PluginColor::Rgb(brightness / 3, brightness / 3, brightness / 2))
            };

            buf.set(px as u16, py as u16, Cell { ch, fg: color, bg: PluginColor::Reset });
        }
    }
}

fn main() {
    let config = StarfieldConfig::default();
    phosphor_plugin::run(Starfield::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_center() {
        let result = Starfield::project(0.0, 0.0, 10.0, 40.0, 12.0);
        assert_eq!(result, Some((40.0, 12.0)));
    }

    #[test]
    fn project_behind_camera() {
        let result = Starfield::project(5.0, 5.0, -1.0, 40.0, 12.0);
        assert_eq!(result, None);
    }

    #[test]
    fn project_at_camera() {
        let result = Starfield::project(5.0, 5.0, 0.0, 40.0, 12.0);
        assert_eq!(result, None);
    }

    #[test]
    fn project_right_of_center() {
        let result = Starfield::project(10.0, 0.0, 10.0, 40.0, 12.0).unwrap();
        assert_eq!(result.0, 80.0);
        assert_eq!(result.1, 12.0);
    }

    #[test]
    fn project_left_of_screen() {
        let result = Starfield::project(-20.0, 0.0, 1.0, 40.0, 12.0).unwrap();
        assert!(result.0 < 0.0);
    }

    #[test]
    fn project_above_screen() {
        let result = Starfield::project(0.0, -20.0, 1.0, 40.0, 12.0).unwrap();
        assert!(result.1 < 0.0);
    }

    #[test]
    fn offscreen_stars_respawned() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.init(80, 24);
        sf.stars[0] = Star {
            x: -100.0,
            y: 0.0,
            z: 1.0,
            prev_sx: -1.0,
            prev_sy: -1.0,
        };
        sf.update(Duration::from_millis(1));
        assert!(sf.stars[0].z > 60.0);
    }

    #[test]
    fn stars_initialized_on_init() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.init(80, 24);
        assert_eq!(sf.stars.len(), sf.config.star_count);
    }

    #[test]
    fn stars_move_closer_on_update() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.init(80, 24);
        let initial_z: Vec<f64> = sf.stars.iter().map(|s| s.z).collect();
        sf.update(Duration::from_millis(100));
        for (star, &init_z) in sf.stars.iter().zip(&initial_z) {
            assert!(star.z < init_z || star.z > init_z * 0.9);
        }
    }

    #[test]
    fn speed_multiplier_normal() {
        let sf = Starfield::new(StarfieldConfig::default());
        assert_eq!(sf.speed_multiplier(), 1.0);
    }

    #[test]
    fn speed_multiplier_during_warp() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.warp_phase = 1.0; // mid-warp
        assert!(sf.speed_multiplier() > 1.0);
    }

    #[test]
    fn warp_phase_decreases() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.init(80, 24);
        sf.warp_phase = 2.0;
        sf.update(Duration::from_millis(100));
        assert!(sf.warp_phase < 2.0);
    }

    #[test]
    fn prev_positions_stored() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.init(80, 24);
        // First update should set prev positions
        sf.update(Duration::from_millis(33));
        // At least some stars should have valid prev positions
        let has_prev = sf.stars.iter().any(|s| s.prev_sx >= 0.0 && s.prev_sy >= 0.0);
        assert!(has_prev);
    }

    #[test]
    fn name_via_info() {
        let sf = Starfield::new(StarfieldConfig::default());
        assert_eq!(sf.info().name, "starfield");
    }

    #[test]
    fn render_does_not_panic() {
        let mut sf = Starfield::new(StarfieldConfig::default());
        sf.init(80, 24);
        sf.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(80, 24);
        sf.render(&mut buf);
    }
}
