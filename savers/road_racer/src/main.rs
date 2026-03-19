use std::time::Duration;

use phosphor_plugin::{
    Cell, ConfigField, EventResponse, FieldKind, KeyCode, KeyInput, PluginBuffer, PluginColor,
    PluginInfo, ScreensaverPlugin,
};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RacerConfig {
    speed_multiplier: f64,
    night_mode: bool,
}

impl Default for RacerConfig {
    fn default() -> Self {
        Self {
            speed_multiplier: 1.0,
            night_mode: false,
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct RoadRacer {
    config: RacerConfig,
    width: u16,
    height: u16,

    // Driving state
    speed: f64,
    position: f64,
    distance: f64,
    steer: f64,

    // Road definition
    road_segments: Vec<f64>,
    segment_length: f64,

    // Autopilot
    last_input_time: f64,
    autopilot: bool,

    elapsed: f64,
}

impl RoadRacer {
    fn new(config: RacerConfig) -> Self {
        let segments = generate_road(200);
        Self {
            config,
            width: 80,
            height: 24,
            speed: 0.4,
            position: 0.0,
            distance: 0.0,
            steer: 0.0,
            road_segments: segments,
            segment_length: 5.0,
            last_input_time: -10.0, // start in autopilot
            autopilot: true,
            elapsed: 0.0,
        }
    }

    /// Get the curvature at a given world distance.
    fn curvature_at(&self, dist: f64) -> f64 {
        let idx = (dist / self.segment_length) as usize % self.road_segments.len();
        self.road_segments[idx]
    }

    /// Average curvature over a look-ahead window for autopilot.
    fn avg_curvature_ahead(&self, look_dist: f64) -> f64 {
        let steps = 10;
        let step_d = look_dist / steps as f64;
        let mut total = 0.0;
        for i in 0..steps {
            total += self.curvature_at(self.distance + (i as f64 + 1.0) * step_d);
        }
        total / steps as f64
    }

    /// Road area starts at this row (everything above is sky/hills).
    fn road_start_row(&self) -> u16 {
        let h = self.height as f64;
        // Sky takes ~35% of screen
        (h * 0.35) as u16
    }

    /// Returns true if position is off-road.
    fn is_offroad(&self) -> bool {
        self.position.abs() > 0.9
    }
}

// ---------------------------------------------------------------------------
// Road generation
// ---------------------------------------------------------------------------

fn generate_road(count: usize) -> Vec<f64> {
    (0..count)
        .map(|i| {
            let t = i as f64;
            0.5 * (t * 0.03).sin() + 0.3 * (t * 0.07 + 1.0).sin() + 0.15 * (t * 0.13 + 2.0).sin()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn dim_color(c: PluginColor, factor: f64) -> PluginColor {
    match c {
        PluginColor::Rgb(r, g, b) => PluginColor::Rgb(
            (r as f64 * factor) as u8,
            (g as f64 * factor) as u8,
            (b as f64 * factor) as u8,
        ),
        other => other,
    }
}

fn lerp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> PluginColor {
    let t = t.clamp(0.0, 1.0);
    PluginColor::Rgb(
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
    )
}

// ---------------------------------------------------------------------------
// Plugin trait impl
// ---------------------------------------------------------------------------

impl ScreensaverPlugin for RoadRacer {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "road_racer".into(),
            description: "90s OutRun-style pseudo-3D road racer with autopilot".into(),
            interactive: true,
            key_bindings: vec![
                ("Left/Right".into(), "Steer".into()),
                ("Up".into(), "Accelerate".into()),
                ("Down".into(), "Brake".into()),
                ("Idle 2s".into(), "Autopilot engages".into()),
            ],
            features: vec![
                "Scanline pseudo-3D road rendering".into(),
                "Procedural curves from layered sines".into(),
                "Autopilot when idle".into(),
                "Night mode with headlights".into(),
                "Roadside scenery (trees, posts, bushes)".into(),
                "Off-road speed penalty and screen shake".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "speed_multiplier".into(),
                    label: "Speed Multiplier".into(),
                    kind: FieldKind::F64,
                    default: "1.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "night_mode".into(),
                    label: "Night Mode".into(),
                    kind: FieldKind::Bool,
                    default: "false".into(),
                    choices: None,
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.speed = 0.4;
        self.position = 0.0;
        self.distance = 0.0;
        self.steer = 0.0;
        self.elapsed = 0.0;
        self.last_input_time = -10.0;
        self.autopilot = true;
    }

    fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64() * self.config.speed_multiplier;
        self.elapsed += dt_secs;

        // Autopilot re-engages after 2s idle
        if self.elapsed - self.last_input_time > 2.0 {
            self.autopilot = true;
        }

        if self.autopilot {
            // Look ahead at average curvature to anticipate curves
            let avg_curve = self.avg_curvature_ahead(60.0);
            // Target position: steer into the curve
            let target_pos = -avg_curve * 0.4;
            // Strong proportional controller — set steer directly
            self.steer = (target_pos - self.position) * 4.0;
            self.steer = self.steer.clamp(-3.0, 3.0);
            // Cruise at moderate speed
            self.speed += (0.5 - self.speed) * 2.0 * dt_secs;
        }

        // Apply steering to position
        self.position += self.steer * dt_secs;

        // Steering decays toward zero (auto-center)
        let decay = (-4.0 * dt_secs).exp();
        self.steer *= decay;

        // Gentle centrifugal drift from road curvature
        let cur_curve = self.curvature_at(self.distance);
        self.position += cur_curve * self.speed * dt_secs * 0.12;

        // Clamp position
        self.position = self.position.clamp(-1.5, 1.5);

        // Off-road drag
        if self.is_offroad() {
            self.speed *= (-1.5 * dt_secs).exp();
            self.speed = self.speed.max(0.05);
        }

        // Advance distance
        self.distance += self.speed * 80.0 * dt_secs;
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let w = buf.width() as usize;
        let h = buf.height() as usize;
        if w == 0 || h == 0 {
            return;
        }

        let night = self.config.night_mode;
        let road_start = self.road_start_row() as usize;

        // -- Sky --
        for y in 0..road_start.min(h) {
            let t = if road_start > 0 {
                y as f64 / road_start as f64
            } else {
                0.0
            };
            let sky_color = if night {
                lerp_color((5, 5, 20), (10, 10, 40), t)
            } else {
                lerp_color((30, 30, 120), (100, 150, 255), t)
            };
            for x in 0..w {
                // Stars in night mode
                let ch = if night && simple_hash(x as u32, y as u32, 7919).is_multiple_of(80) {
                    '.'
                } else {
                    ' '
                };
                let fg = if night {
                    PluginColor::Rgb(200, 200, 200)
                } else {
                    sky_color
                };
                buf.set(
                    x as u16,
                    y as u16,
                    Cell {
                        ch,
                        fg,
                        bg: sky_color,
                    },
                );
            }
        }

        // -- Hills silhouette at horizon --
        let hill_rows = 3.min(road_start);
        for dy in 0..hill_rows {
            let y = (road_start - hill_rows + dy).min(h - 1);
            for x in 0..w {
                let hill_h =
                    2.0 + 1.5 * (x as f64 * 0.04).sin() + 1.0 * (x as f64 * 0.09 + 1.5).sin();
                if (dy as f64) < hill_h {
                    let c = if night {
                        PluginColor::Rgb(10, 20, 10)
                    } else {
                        PluginColor::Rgb(30, 80, 30)
                    };
                    buf.set(
                        x as u16,
                        y as u16,
                        Cell {
                            ch: ' ',
                            fg: c,
                            bg: c,
                        },
                    );
                }
            }
        }

        // -- Road area (scanline pseudo-3D) --
        let road_rows = h.saturating_sub(road_start);
        if road_rows == 0 {
            return;
        }

        let camera_depth = 0.8;
        let base_road_width = 0.35;

        // Shake offset when off-road
        let shake_x: i32 = if self.is_offroad() {
            let s = (self.elapsed * 37.0).sin();
            if s > 0.3 {
                1
            } else if s < -0.3 {
                -1
            } else {
                0
            }
        } else {
            0
        };

        // Headlight cone parameters for night mode
        let car_screen_x = w as f64 / 2.0 + self.position * w as f64 * 0.25;

        // Pre-compute z values for each road row (bottom=near to top=far)
        // and accumulate curvature incrementally (the real OutRun technique).
        let mut z_values = Vec::with_capacity(road_rows);
        let mut curve_offsets = Vec::with_capacity(road_rows);
        {
            let mut prev_z = 0.0_f64;
            let mut cum_offset = 0.0_f64;
            for row_idx in 0..road_rows {
                let row_frac = (row_idx as f64 + 1.0) / road_rows as f64;
                let z = camera_depth / row_frac;
                let dz = z - prev_z;
                let seg_curve = self.curvature_at(self.distance + z * 5.0);
                cum_offset += seg_curve * dz * 0.6;
                z_values.push(z);
                curve_offsets.push(cum_offset);
                prev_z = z;
            }
        }

        for row_idx in 0..road_rows {
            let screen_y = road_start + road_rows - 1 - row_idx; // bottom to top
            if screen_y >= h {
                continue;
            }

            let z = z_values[row_idx];
            let perspective = camera_depth / z;

            // Road width at this depth
            let road_half = base_road_width * perspective * w as f64;

            // Curve shifts the road center; player position shifts opposite
            let curve_offset = curve_offsets[row_idx];
            let road_center = w as f64 / 2.0 + curve_offset * perspective * w as f64 * 0.4
                - self.position * perspective * w as f64 * 0.3
                + shake_x as f64;

            // Stripe pattern (scrolling)
            let stripe_z = z * 10.0 + self.distance;
            let stripe = ((stripe_z * 0.15) as i64).rem_euclid(2) == 0;

            for x in 0..w {
                let dx = x as f64 - road_center;
                let abs_dx = dx.abs();

                let (ch, mut fg, mut bg);

                if abs_dx < road_half - 2.0 {
                    // --- Road surface ---
                    let center_dist = abs_dx;
                    let center_dash_width = road_half * 0.02 + 0.8;

                    if center_dist < center_dash_width && stripe {
                        // Center dashes
                        ch = ' ';
                        fg = PluginColor::Rgb(255, 255, 255);
                        bg = PluginColor::Rgb(255, 255, 255);
                    } else if stripe {
                        ch = ' ';
                        fg = PluginColor::Rgb(100, 100, 100);
                        bg = PluginColor::Rgb(100, 100, 100);
                    } else {
                        ch = ' ';
                        fg = PluginColor::Rgb(80, 80, 80);
                        bg = PluginColor::Rgb(80, 80, 80);
                    }
                } else if abs_dx < road_half {
                    // --- Road edge line ---
                    if stripe {
                        ch = ' ';
                        fg = PluginColor::Rgb(255, 50, 50);
                        bg = PluginColor::Rgb(255, 50, 50);
                    } else {
                        ch = ' ';
                        fg = PluginColor::Rgb(255, 255, 255);
                        bg = PluginColor::Rgb(255, 255, 255);
                    }
                } else if abs_dx < road_half + 2.0 {
                    // --- Rumble strip ---
                    if stripe {
                        ch = ' ';
                        fg = PluginColor::Rgb(255, 50, 50);
                        bg = PluginColor::Rgb(255, 50, 50);
                    } else {
                        ch = ' ';
                        fg = PluginColor::Rgb(255, 255, 255);
                        bg = PluginColor::Rgb(255, 255, 255);
                    }
                } else {
                    // --- Grass ---
                    if stripe {
                        ch = ' ';
                        fg = PluginColor::Rgb(30, 160, 30);
                        bg = PluginColor::Rgb(30, 160, 30);
                    } else {
                        ch = ' ';
                        fg = PluginColor::Rgb(20, 120, 20);
                        bg = PluginColor::Rgb(20, 120, 20);
                    }
                }

                // Night mode dimming + headlight cone
                if night {
                    let headlight_dist =
                        ((x as f64 - car_screen_x).abs() / (row_idx as f64 + 5.0)).min(2.0);

                    let brightness: f64 = if headlight_dist < 0.6 {
                        0.8
                    } else if headlight_dist < 1.2 {
                        0.5
                    } else {
                        0.25
                    };

                    // Road markings stay brighter
                    let is_marking = matches!(fg, PluginColor::Rgb(255, 255, 255))
                        || matches!(fg, PluginColor::Rgb(255, 50, 50));
                    let factor = if is_marking {
                        brightness.max(0.7)
                    } else {
                        brightness
                    };

                    fg = dim_color(fg, factor);
                    bg = dim_color(bg, factor);
                }

                buf.set(x as u16, screen_y as u16, Cell { ch, fg, bg });
            }

            // --- Scenery objects along this scanline ---
            let seg_idx = ((z * 10.0 + self.distance) / self.segment_length) as u32;
            // Only place scenery every few segments, and only for certain depths
            if seg_idx.is_multiple_of(3) && row_idx > 2 {
                let hash = simple_hash(seg_idx, 0, 31337);
                let side: f64 = if hash.is_multiple_of(2) { 1.0 } else { -1.0 };
                let obj_x = road_center + side * (road_half + 3.0 + (hash % 5) as f64);
                let obj_x_i = obj_x as i32;

                // Scale: near objects are bigger
                let scale = (perspective * 3.0).clamp(0.3, 1.5);

                let obj_type = hash % 7;
                if obj_x_i >= 0 && obj_x_i < w as i32 {
                    let ox = obj_x_i as u16;
                    let (obj_ch, obj_fg) = match obj_type {
                        0..=2 => {
                            // Tree
                            let green = if night {
                                PluginColor::Rgb(8, 50, 8)
                            } else {
                                PluginColor::Rgb(20, 140, 20)
                            };
                            if scale > 0.6 {
                                ('^', green)
                            } else {
                                ('.', green)
                            }
                        }
                        3..=4 => {
                            // Post/pole
                            let gray = if night {
                                PluginColor::Rgb(40, 40, 40)
                            } else {
                                PluginColor::Rgb(160, 160, 160)
                            };
                            ('|', gray)
                        }
                        _ => {
                            // Bush
                            let green = if night {
                                PluginColor::Rgb(5, 35, 5)
                            } else {
                                PluginColor::Rgb(15, 100, 15)
                            };
                            ('*', green)
                        }
                    };
                    if screen_y < h {
                        let grass_bg = if stripe {
                            if night {
                                PluginColor::Rgb(7, 38, 7)
                            } else {
                                PluginColor::Rgb(30, 160, 30)
                            }
                        } else if night {
                            PluginColor::Rgb(5, 28, 5)
                        } else {
                            PluginColor::Rgb(20, 120, 20)
                        };
                        buf.set(
                            ox,
                            screen_y as u16,
                            Cell {
                                ch: obj_ch,
                                fg: obj_fg,
                                bg: grass_bg,
                            },
                        );

                        // Draw trunk below tree top for near trees
                        if obj_type <= 2 && scale > 0.8 && screen_y + 1 < h {
                            let trunk_fg = if night {
                                PluginColor::Rgb(30, 18, 6)
                            } else {
                                PluginColor::Rgb(100, 60, 20)
                            };
                            buf.set(
                                ox,
                                (screen_y + 1) as u16,
                                Cell {
                                    ch: '|',
                                    fg: trunk_fg,
                                    bg: grass_bg,
                                },
                            );
                        }
                    }
                }
            }
        }

        // -- Car sprite --
        let car_x = (w as f64 / 2.0 + self.position * w as f64 * 0.25 + shake_x as f64) as i32;
        let car_y = h as i32 - 4;

        if car_y >= 0 && car_y + 2 < h as i32 {
            let car_body = PluginColor::Rgb(220, 40, 40);
            let car_window = PluginColor::Rgb(100, 180, 255);
            let car_wheel = PluginColor::Rgb(40, 40, 40);
            let road_bg = PluginColor::Rgb(90, 90, 90);

            // Tilt based on steering
            let tilt: i32 = if self.steer > 0.3 {
                1
            } else if self.steer < -0.3 {
                -1
            } else {
                0
            };

            //   ___
            //  /___\
            // |_|_|_|
            type CarLine<'a> = (i32, &'a [(i32, char, PluginColor, PluginColor)]);
            let car_lines: [CarLine; 3] = [
                (
                    tilt,
                    &[
                        (-1, '_', car_body, road_bg),
                        (0, '_', car_body, road_bg),
                        (1, '_', car_body, road_bg),
                    ],
                ),
                (
                    0,
                    &[
                        (-2, '/', car_body, road_bg),
                        (-1, '_', car_window, road_bg),
                        (0, '_', car_window, road_bg),
                        (1, '_', car_window, road_bg),
                        (2, '\\', car_body, road_bg),
                    ],
                ),
                (
                    0,
                    &[
                        (-2, '|', car_body, road_bg),
                        (-1, '_', car_wheel, road_bg),
                        (0, '|', car_body, road_bg),
                        (1, '_', car_wheel, road_bg),
                        (2, '|', car_body, road_bg),
                    ],
                ),
            ];

            for (row_offset, (line_tilt, parts)) in car_lines.iter().enumerate() {
                let sy = car_y + row_offset as i32;
                if sy < 0 || sy >= h as i32 {
                    continue;
                }
                for &(dx, ch, fg, bg) in *parts {
                    let sx = car_x + dx + line_tilt;
                    if sx >= 0 && sx < w as i32 {
                        let mut final_fg = fg;
                        let mut final_bg = bg;
                        if night {
                            final_fg = dim_color(fg, 0.9);
                            final_bg = dim_color(bg, 0.5);
                        }
                        buf.set(
                            sx as u16,
                            sy as u16,
                            Cell {
                                ch,
                                fg: final_fg,
                                bg: final_bg,
                            },
                        );
                    }
                }
            }
        }

        // -- HUD --
        // Speed bar: bottom-left
        let hud_y = h.saturating_sub(1) as u16;
        let speed_pct = (self.speed * 100.0) as u32;
        let bar_len = 7;
        let filled = ((self.speed * bar_len as f64) as usize).min(bar_len);
        let hud_fg = if night {
            PluginColor::Rgb(180, 180, 180)
        } else {
            PluginColor::Rgb(255, 255, 255)
        };
        let hud_bg = PluginColor::Rgb(0, 0, 0);

        let spd_label = "SPD ";
        let spd_bar: String = (0..bar_len)
            .map(|i| if i < filled { '\u{2588}' } else { '\u{2591}' })
            .collect();
        let spd_num = format!(" {speed_pct:>3}");
        let hud_left = format!("{spd_label}{spd_bar}{spd_num}");

        for (i, ch) in hud_left.chars().enumerate() {
            if i < w {
                buf.set(
                    i as u16,
                    hud_y,
                    Cell {
                        ch,
                        fg: hud_fg,
                        bg: hud_bg,
                    },
                );
            }
        }

        // Distance: bottom-right
        let dist_str = format!("{:.0} m", self.distance);
        let start_x = w.saturating_sub(dist_str.len() + 1);
        for (i, ch) in dist_str.chars().enumerate() {
            let x = start_x + i;
            if x < w {
                buf.set(
                    x as u16,
                    hud_y,
                    Cell {
                        ch,
                        fg: hud_fg,
                        bg: hud_bg,
                    },
                );
            }
        }

        // Autopilot indicator
        if self.autopilot {
            let ap_str = "AUTO";
            let ap_x = (w / 2).saturating_sub(2);
            let ap_fg = PluginColor::Rgb(255, 200, 50);
            for (i, ch) in ap_str.chars().enumerate() {
                let x = ap_x + i;
                if x < w {
                    buf.set(
                        x as u16,
                        hud_y,
                        Cell {
                            ch,
                            fg: ap_fg,
                            bg: hud_bg,
                        },
                    );
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyInput) -> EventResponse {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                // Arrow keys disable autopilot and apply impulse
                self.last_input_time = self.elapsed;
                self.autopilot = false;
            }
            _ => return EventResponse::Ignored,
        }

        match key.code {
            KeyCode::Left => {
                // Each press nudges steering left; terminal key-repeat handles "holding"
                self.steer = (self.steer - 1.5).max(-5.0);
                EventResponse::Continue
            }
            KeyCode::Right => {
                self.steer = (self.steer + 1.5).min(5.0);
                EventResponse::Continue
            }
            KeyCode::Up => {
                self.speed = (self.speed + 0.06).min(1.0);
                EventResponse::Continue
            }
            KeyCode::Down => {
                self.speed = (self.speed - 0.08).max(0.05);
                EventResponse::Continue
            }
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Simple deterministic hash for scenery placement.
fn simple_hash(a: u32, b: u32, seed: u32) -> u32 {
    let mut h = a
        .wrapping_mul(374761393)
        .wrapping_add(b.wrapping_mul(668265263))
        .wrapping_add(seed);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let config = RacerConfig::default();
    phosphor_plugin::run(RoadRacer::new(config));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_racer() -> RoadRacer {
        let mut r = RoadRacer::new(RacerConfig::default());
        r.init(80, 24);
        r
    }

    #[test]
    fn name_via_info() {
        let r = RoadRacer::new(RacerConfig::default());
        assert_eq!(r.info().name, "road_racer");
        assert!(r.info().interactive);
    }

    #[test]
    fn init_resets_state() {
        let mut r = make_racer();
        r.distance = 9999.0;
        r.init(80, 24);
        assert_eq!(r.distance, 0.0);
        assert!(r.autopilot);
    }

    #[test]
    fn update_advances_distance() {
        let mut r = make_racer();
        let old = r.distance;
        r.update(Duration::from_millis(100));
        assert!(r.distance > old);
    }

    #[test]
    fn autopilot_steers_toward_road() {
        let mut r = make_racer();
        r.position = 0.8;
        // Run autopilot for a while
        for _ in 0..200 {
            r.update(Duration::from_millis(33));
        }
        // Autopilot should have corrected back toward center
        assert!(r.position.abs() < 0.8);
    }

    #[test]
    fn key_input_disables_autopilot() {
        let mut r = make_racer();
        assert!(r.autopilot);
        let resp = r.handle_key(KeyInput {
            code: KeyCode::Left,
            modifiers: 0,
        });
        assert_eq!(resp, EventResponse::Continue);
        assert!(!r.autopilot);
        assert!(r.steer < 0.0, "Left key should set negative steer");
    }

    #[test]
    fn non_arrow_key_ignored() {
        let mut r = make_racer();
        let resp = r.handle_key(KeyInput {
            code: KeyCode::Char('a'),
            modifiers: 0,
        });
        assert_eq!(resp, EventResponse::Ignored);
        assert!(r.autopilot, "Non-arrow keys should not disable autopilot");
    }

    #[test]
    fn arrow_keys_steer() {
        let mut r = make_racer();
        r.handle_key(KeyInput {
            code: KeyCode::Right,
            modifiers: 0,
        });
        for _ in 0..30 {
            r.update(Duration::from_millis(33));
        }
        assert!(r.position > 0.0, "Steering right should increase position");
    }

    #[test]
    fn up_arrow_accelerates() {
        let mut r = make_racer();
        r.speed = 0.3;
        r.handle_key(KeyInput {
            code: KeyCode::Up,
            modifiers: 0,
        });
        r.update(Duration::from_millis(100));
        assert!(r.speed > 0.3);
    }

    #[test]
    fn down_arrow_brakes() {
        let mut r = make_racer();
        r.speed = 0.8;
        r.handle_key(KeyInput {
            code: KeyCode::Down,
            modifiers: 0,
        });
        r.update(Duration::from_millis(100));
        assert!(r.speed < 0.8);
    }

    #[test]
    fn offroad_detection() {
        let mut r = make_racer();
        r.position = 0.5;
        assert!(!r.is_offroad());
        r.position = 1.0;
        assert!(r.is_offroad());
    }

    #[test]
    fn offroad_slows_speed() {
        let mut r = make_racer();
        r.position = 1.2;
        r.speed = 0.8;
        r.autopilot = false;
        r.last_input_time = r.elapsed;
        let old_speed = r.speed;
        for _ in 0..60 {
            r.update(Duration::from_millis(16));
        }
        assert!(r.speed < old_speed, "Off-road should slow the car");
    }

    #[test]
    fn render_does_not_panic() {
        let mut r = make_racer();
        for _ in 0..30 {
            r.update(Duration::from_millis(33));
        }
        let mut buf = PluginBuffer::new(80, 24);
        r.render(&mut buf);
    }

    #[test]
    fn render_night_mode_does_not_panic() {
        let mut config = RacerConfig::default();
        config.night_mode = true;
        let mut r = RoadRacer::new(config);
        r.init(80, 24);
        for _ in 0..30 {
            r.update(Duration::from_millis(33));
        }
        let mut buf = PluginBuffer::new(80, 24);
        r.render(&mut buf);
    }

    #[test]
    fn render_small_terminal() {
        let mut r = RoadRacer::new(RacerConfig::default());
        r.init(20, 8);
        r.update(Duration::from_millis(33));
        let mut buf = PluginBuffer::new(20, 8);
        r.render(&mut buf);
    }

    #[test]
    fn road_generation_produces_curves() {
        let segs = generate_road(200);
        assert_eq!(segs.len(), 200);
        // Should have both positive and negative curvatures
        assert!(segs.iter().any(|&c| c > 0.1));
        assert!(segs.iter().any(|&c| c < -0.1));
    }

    #[test]
    fn speed_multiplier_affects_distance() {
        let mut slow = RoadRacer::new(RacerConfig {
            speed_multiplier: 0.5,
            night_mode: false,
        });
        slow.init(80, 24);
        let mut fast = RoadRacer::new(RacerConfig {
            speed_multiplier: 2.0,
            night_mode: false,
        });
        fast.init(80, 24);

        for _ in 0..100 {
            slow.update(Duration::from_millis(33));
            fast.update(Duration::from_millis(33));
        }
        assert!(fast.distance > slow.distance * 2.0);
    }

    #[test]
    fn autopilot_reengages_after_idle() {
        let mut r = make_racer();
        // Manual input
        r.handle_key(KeyInput {
            code: KeyCode::Left,
            modifiers: 0,
        });
        assert!(!r.autopilot);
        // Simulate 3 seconds of idle
        for _ in 0..100 {
            r.update(Duration::from_millis(33));
        }
        assert!(r.autopilot, "Autopilot should re-engage after idle");
    }

    #[test]
    fn simple_hash_deterministic() {
        let h1 = simple_hash(42, 7, 31337);
        let h2 = simple_hash(42, 7, 31337);
        assert_eq!(h1, h2);
        let h3 = simple_hash(43, 7, 31337);
        assert_ne!(h1, h3);
    }

    #[test]
    fn position_clamped() {
        let mut r = make_racer();
        r.autopilot = false;
        r.last_input_time = 999.0;
        r.elapsed = 999.0;
        r.position = 1.5;
        r.steer = 10.0;
        r.update(Duration::from_millis(100));
        assert!(r.position <= 1.5, "Position should be clamped");
    }
}
