use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use phosphor_plugin::{
    Cell, ConfigField, FieldKind, PluginBuffer, PluginColor, PluginInfo,
    ScreensaverPlugin,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct AuroraConfig {
    ribbon_count: usize,
    speed: f64,
    intensity: f64,
    palette: String,
}

impl Default for AuroraConfig {
    fn default() -> Self {
        Self {
            ribbon_count: 4,
            speed: 0.5,
            intensity: 0.8,
            palette: "classic".into(),
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

struct Ribbon {
    a1: f64,
    f1: f64,
    p1: f64,
    a2: f64,
    f2: f64,
    p2: f64,
    base_y: f64,
    width: f64,
    pulse_freq: f64,
    hue_offset: f64,
}

struct Aurora {
    width: u16,
    height: u16,
    t: f64,
    ribbons: Vec<Ribbon>,
    config: AuroraConfig,
}

impl Aurora {
    fn new(config: AuroraConfig) -> Self {
        Self {
            width: 80,
            height: 24,
            t: 0.0,
            ribbons: Vec::new(),
            config,
        }
    }

    fn generate_ribbons(&mut self) {
        let mut rng = rand::rng();
        self.ribbons.clear();
        let h = self.height as f64;

        for i in 0..self.config.ribbon_count {
            let spread = 0.6 / self.config.ribbon_count as f64;
            let base_y = 0.05 + spread * i as f64 + rng.random_range(0.0..spread);

            self.ribbons.push(Ribbon {
                a1: rng.random_range(2.0..6.0) * (h / 24.0),
                f1: rng.random_range(0.02..0.06),
                p1: rng.random_range(0.3..0.8),
                a2: rng.random_range(1.0..3.0) * (h / 24.0),
                f2: rng.random_range(0.04..0.10),
                p2: rng.random_range(0.5..1.2),
                base_y,
                width: rng.random_range(3.0..7.0) * (h / 24.0),
                pulse_freq: rng.random_range(0.2..0.6),
                hue_offset: rng.random_range(-10.0..10.0),
            });
        }
    }

    fn ribbon_center_y(&self, ribbon: &Ribbon, x: f64) -> f64 {
        let h = self.height as f64;
        let y_offset = ribbon.a1 * (ribbon.f1 * x + ribbon.p1 * self.t).sin()
            + ribbon.a2 * (ribbon.f2 * x + ribbon.p2 * self.t).sin();
        ribbon.base_y * h + y_offset
    }

    fn color_for_position(&self, y: f64, ribbon: &Ribbon) -> (f64, f64, f64) {
        let y_frac = (y / self.height as f64).clamp(0.0, 1.0);
        match self.config.palette.as_str() {
            "warm" => {
                let hue = 0.0 + y_frac * 60.0 + ribbon.hue_offset;
                let (r, g, b) = hsv_to_rgb(hue.rem_euclid(360.0), 0.9, 1.0);
                (r as f64, g as f64, b as f64)
            }
            "mono" => {
                let v = 255.0;
                (v, v, v)
            }
            _ => {
                // "classic": green (120) at top -> cyan (180) -> violet (270) at bottom
                let hue = 120.0 + y_frac * 150.0 + ribbon.hue_offset;
                let (r, g, b) = hsv_to_rgb(hue.clamp(120.0, 280.0), 0.85, 1.0);
                (r as f64, g as f64, b as f64)
            }
        }
    }

    fn intensity_char(intensity: f64) -> char {
        if intensity < 0.1 {
            ' '
        } else if intensity < 0.3 {
            '\u{2591}' // ░
        } else if intensity < 0.6 {
            '\u{2592}' // ▒
        } else if intensity < 0.8 {
            '\u{2593}' // ▓
        } else {
            '\u{2588}' // █
        }
    }
}

impl ScreensaverPlugin for Aurora {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "aurora".into(),
            description: "Northern lights with undulating ribbons of color".into(),
            key_bindings: vec![("Any key".into(), "Exit screensaver".into())],
            features: vec![
                "Layered undulating ribbons".into(),
                "Multiple color palettes".into(),
                "Configurable ribbon count and intensity".into(),
            ],
            config_fields: vec![
                ConfigField {
                    key: "ribbon_count".into(),
                    label: "Ribbon Count".into(),
                    kind: FieldKind::USize,
                    default: "4".into(),
                    choices: None,
                },
                ConfigField {
                    key: "speed".into(),
                    label: "Speed".into(),
                    kind: FieldKind::F64,
                    default: "0.5".into(),
                    choices: None,
                },
                ConfigField {
                    key: "intensity".into(),
                    label: "Intensity".into(),
                    kind: FieldKind::F64,
                    default: "0.8".into(),
                    choices: None,
                },
                ConfigField {
                    key: "palette".into(),
                    label: "Palette".into(),
                    kind: FieldKind::Text,
                    default: "classic".into(),
                    choices: Some(vec!["classic".into(), "warm".into(), "mono".into()]),
                },
            ],
        }
    }

    fn init(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.t = 0.0;
        self.generate_ribbons();
    }

    fn update(&mut self, dt: Duration) {
        self.t += dt.as_secs_f64() * self.config.speed;
    }

    fn render(&self, buf: &mut PluginBuffer) {
        let w = buf.width() as usize;
        let h = buf.height() as usize;

        for cy in 0..h {
            for cx in 0..w {
                let mut r_acc: f64 = 0.0;
                let mut g_acc: f64 = 0.0;
                let mut b_acc: f64 = 0.0;
                let mut max_intensity: f64 = 0.0;

                for ribbon in &self.ribbons {
                    let center_y = self.ribbon_center_y(ribbon, cx as f64);
                    let dist = (cy as f64 - center_y).abs();

                    if dist >= ribbon.width {
                        continue;
                    }

                    let falloff = 1.0 - dist / ribbon.width;
                    let brightness = 0.6 + 0.4 * (self.t * ribbon.pulse_freq).sin();
                    let cell_intensity = falloff * brightness * self.config.intensity;

                    let (cr, cg, cb) = self.color_for_position(cy as f64, ribbon);
                    r_acc += cr * cell_intensity;
                    g_acc += cg * cell_intensity;
                    b_acc += cb * cell_intensity;
                    max_intensity += cell_intensity;
                }

                if max_intensity < 0.1 {
                    continue;
                }

                let r = (r_acc as u32).min(255) as u8;
                let g = (g_acc as u32).min(255) as u8;
                let b = (b_acc as u32).min(255) as u8;
                let ch = Self::intensity_char(max_intensity.min(1.0));

                if ch != ' ' {
                    buf.set(
                        cx as u16,
                        cy as u16,
                        Cell { ch, fg: PluginColor::Rgb(r, g, b), bg: PluginColor::Reset },
                    );
                }
            }
        }
    }
}

fn main() {
    let config = AuroraConfig::default();
    phosphor_plugin::run(Aurora::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_aurora() -> Aurora {
        Aurora::new(AuroraConfig::default())
    }

    #[test]
    fn init_creates_correct_ribbon_count() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        assert_eq!(aurora.ribbons.len(), aurora.config.ribbon_count);
        assert_eq!(aurora.width, 80);
        assert_eq!(aurora.height, 24);
    }

    #[test]
    fn update_advances_time() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        let t_before = aurora.t;
        aurora.update(Duration::from_millis(100));
        assert!(aurora.t > t_before);
    }

    #[test]
    fn ribbons_have_valid_parameters() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        for ribbon in &aurora.ribbons {
            assert!(ribbon.base_y >= 0.0 && ribbon.base_y <= 1.0);
            assert!(ribbon.width > 0.0);
            assert!(ribbon.pulse_freq > 0.0);
            assert!(ribbon.a1 > 0.0);
            assert!(ribbon.f1 > 0.0);
        }
    }

    #[test]
    fn name_and_description() {
        let aurora = make_aurora();
        let info = aurora.info();
        assert_eq!(info.name, "aurora");
        assert!(!info.description.is_empty());
    }

    #[test]
    fn intensity_char_mapping() {
        assert_eq!(Aurora::intensity_char(0.0), ' ');
        assert_eq!(Aurora::intensity_char(0.2), '\u{2591}');
        assert_eq!(Aurora::intensity_char(0.4), '\u{2592}');
        assert_eq!(Aurora::intensity_char(0.7), '\u{2593}');
        assert_eq!(Aurora::intensity_char(0.9), '\u{2588}');
    }

    #[test]
    fn hsv_to_rgb_green() {
        let (r, g, b) = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!(r, 0);
        assert_eq!(g, 255);
        assert_eq!(b, 0);
    }

    #[test]
    fn reinit_resets_state() {
        let mut aurora = make_aurora();
        aurora.init(80, 24);
        aurora.update(Duration::from_secs(5));
        aurora.init(120, 40);
        assert_eq!(aurora.t, 0.0);
        assert_eq!(aurora.width, 120);
        assert_eq!(aurora.height, 40);
        assert_eq!(aurora.ribbons.len(), aurora.config.ribbon_count);
    }

    #[test]
    fn render_produces_output() {
        let mut aurora = make_aurora();
        aurora.init(40, 20);
        aurora.update(Duration::from_millis(500));
        let mut buf = PluginBuffer::new(40, 20);
        aurora.render(&mut buf);
        // At least some cells should have non-default content
        let non_empty = (0..40)
            .flat_map(|x| (0..20).map(move |y| (x, y)))
            .any(|(x, y)| buf.get(x, y).unwrap().fg != PluginColor::Reset);
        assert!(non_empty);
    }
}
