use std::time::Duration;

use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo, ScreensaverPlugin,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct PlasmaConfig {
    speed: f64,
    scale: f64,
    mode: String,
    color_mode: String,
}

impl Default for PlasmaConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            scale: 8.0,
            mode: "fill".into(),
            color_mode: "rainbow".into(),
        }
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

fn plasma_value(x: f64, y: f64, t: f64, scale: f64) -> f64 {
    let v = (x / scale + t).sin()
        + (y / scale + t * 0.7).sin()
        + ((x + y) / scale + t * 1.3).sin()
        + ((x * x + y * y).sqrt() / scale + t * 0.5).sin();
    (v + 4.0) / 8.0
}

fn value_to_color(v: f64, color_mode: &str) -> PluginColor {
    match color_mode {
        "fire" => {
            let (r, g, b) = if v < 0.25 {
                let t = v / 0.25;
                ((t * 180.0) as u8, 0, 0)
            } else if v < 0.5 {
                let t = (v - 0.25) / 0.25;
                (180 + (t * 75.0) as u8, (t * 165.0) as u8, 0)
            } else if v < 0.75 {
                let t = (v - 0.5) / 0.25;
                (255, 165 + (t * 90.0) as u8, (t * 50.0) as u8)
            } else {
                let t = (v - 0.75) / 0.25;
                (255, 255, 50 + (t * 205.0) as u8)
            };
            PluginColor::Rgb(r, g, b)
        }
        "ocean" => {
            let (r, g, b) = if v < 0.25 {
                let t = v / 0.25;
                (0, 0, (t * 180.0) as u8)
            } else if v < 0.5 {
                let t = (v - 0.25) / 0.25;
                (0, (t * 200.0) as u8, 180 + (t * 75.0) as u8)
            } else if v < 0.75 {
                let t = (v - 0.5) / 0.25;
                (0, 200 + (t * 55.0) as u8, 255 - (t * 100.0) as u8)
            } else {
                let t = (v - 0.75) / 0.25;
                ((t * 255.0) as u8, 255, 155 + (t * 100.0) as u8)
            };
            PluginColor::Rgb(r, g, b)
        }
        "mono" => {
            let g = (v * 255.0) as u8;
            PluginColor::Rgb(g, g, g)
        }
        _ => {
            let hue = v * 360.0;
            let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
            PluginColor::Rgb(r, g, b)
        }
    }
}

struct Plasma {
    width: u16,
    height: u16,
    t: f64,
    config: PlasmaConfig,
}

impl Plasma {
    fn new(config: PlasmaConfig) -> Self {
        Self {
            width: 80,
            height: 24,
            t: 0.0,
            config,
        }
    }
}

impl ScreensaverPlugin for Plasma {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "plasma".into(),
            description: "Demoscene plasma effect with morphing rainbow waves".into(),
            interactive: false,
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Classic 4-term plasma".into(),
                "Fill and braille render modes".into(),
                "Multiple color palettes".into(),
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
                    key: "scale".into(),
                    label: "Scale".into(),
                    kind: FieldKind::F64,
                    default: "8.0".into(),
                    choices: None,
                },
                ConfigField {
                    key: "mode".into(),
                    label: "Mode".into(),
                    kind: FieldKind::Text,
                    default: "fill".into(),
                    choices: Some(vec!["fill".into(), "braille".into()]),
                },
                ConfigField {
                    key: "color_mode".into(),
                    label: "Color Mode".into(),
                    kind: FieldKind::Text,
                    default: "rainbow".into(),
                    choices: Some(vec![
                        "rainbow".into(),
                        "fire".into(),
                        "ocean".into(),
                        "mono".into(),
                    ]),
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.t = 0.0;
    }

    fn update(&mut self, dt: Duration) {
        self.t += dt.as_secs_f64() * self.config.speed;
    }

    fn render(&self, buf: &mut PluginBuffer) {
        if self.config.mode == "braille" {
            for cy in 0..buf.height() {
                for cx in 0..buf.width() {
                    let mut bits: u8 = 0;
                    let mut val_sum: f64 = 0.0;

                    for dx in 0..2usize {
                        for dy in 0..4usize {
                            let sx = cx as f64 * 2.0 + dx as f64;
                            let sy = cy as f64 * 4.0 + dy as f64;
                            let v = plasma_value(sx, sy, self.t, self.config.scale);
                            val_sum += v;
                            if v > 0.5 {
                                bits |= braille_bit(dx, dy);
                            }
                        }
                    }

                    let avg = val_sum / 8.0;
                    let color = value_to_color(avg, &self.config.color_mode);
                    let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');

                    buf.set(
                        cx,
                        cy,
                        Cell {
                            ch,
                            fg: color,
                            bg: PluginColor::Reset,
                        },
                    );
                }
            }
        } else {
            for y in 0..buf.height() {
                for x in 0..buf.width() {
                    let v = plasma_value(x as f64, y as f64, self.t, self.config.scale);
                    let color = value_to_color(v, &self.config.color_mode);
                    buf.set(
                        x,
                        y,
                        Cell {
                            ch: ' ',
                            fg: PluginColor::Reset,
                            bg: color,
                        },
                    );
                }
            }
        }
    }
}

fn main() {
    let config = PlasmaConfig::default();
    phosphor_plugin::run(Plasma::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_plasma() -> Plasma {
        Plasma::new(PlasmaConfig::default())
    }

    #[test]
    fn init_creates_valid_state() {
        let mut p = default_plasma();
        p.init(120, 40);
        assert_eq!(p.width, 120);
        assert_eq!(p.height, 40);
        assert_eq!(p.t, 0.0);
    }

    #[test]
    fn update_advances_time() {
        let mut p = default_plasma();
        p.init(80, 24);
        p.update(Duration::from_millis(100));
        assert!(p.t > 0.0);
    }

    #[test]
    fn name_via_info() {
        let p = default_plasma();
        assert_eq!(p.info().name, "plasma");
        assert!(!p.info().description.is_empty());
    }

    #[test]
    fn hsv_red() {
        let (r, g, b) = hsv_to_rgb(0.0, 1.0, 1.0);
        assert_eq!(r, 255);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn hsv_green() {
        let (r, g, b) = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 255);
        assert_eq!(b, 0);
    }

    #[test]
    fn hsv_blue() {
        let (r, g, b) = hsv_to_rgb(240.0, 1.0, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 255);
    }

    #[test]
    fn plasma_value_in_range() {
        for x in 0..20 {
            for y in 0..20 {
                let v = plasma_value(x as f64, y as f64, 1.0, 8.0);
                assert!((0.0..=1.0).contains(&v), "v={v} out of range");
            }
        }
    }

    #[test]
    fn render_fills_buffer() {
        let mut p = default_plasma();
        p.init(10, 5);
        p.update(Duration::from_millis(100));
        let mut buf = PluginBuffer::new(10, 5);
        p.render(&mut buf);
        // In fill mode, all cells should have non-Reset bg
        let cell = buf.get(5, 2).unwrap();
        assert_ne!(cell.bg, PluginColor::Reset);
    }
}
