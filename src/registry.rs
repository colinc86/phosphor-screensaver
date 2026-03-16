use crate::config::Config;
use crate::screensaver::Screensaver;
use crate::screensavers::{
    analog_clock::AnalogClock, bouncing_ball::BouncingBall, digital_clock::DigitalClock,
    fluid::Fluid, game_of_life::GameOfLife, lissajous::Lissajous, matrix_rain::MatrixRain,
    pipes::Pipes, starfield::Starfield, terrain::Terrain,
};
use crate::theme::Theme;

/// Information about a registered screensaver.
pub struct ScreensaverInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub key_bindings: &'static [(&'static str, &'static str)],
    pub features: &'static [&'static str],
}

/// All available screensavers.
pub const SCREENSAVERS: &[ScreensaverInfo] = &[
    ScreensaverInfo {
        name: "bouncing_ball",
        description: "A ball bouncing around the screen with a colorful trail",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["Physics-based bouncing", "Configurable trail length", "Color theming"],
    },
    ScreensaverInfo {
        name: "starfield",
        description: "A parallax starfield with warp drive streaks",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["3D-to-2D projection", "Depth-based brightness", "Warp drive streaks"],
    },
    ScreensaverInfo {
        name: "matrix_rain",
        description: "Falling characters in the style of The Matrix",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["Katakana/ASCII/binary charsets", "Configurable density and speed"],
    },
    ScreensaverInfo {
        name: "pipes",
        description: "Pipe drawing with junctions and flowing liquid",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["Box-drawing characters", "Junction detection", "Flowing liquid animation"],
    },
    ScreensaverInfo {
        name: "digital_clock",
        description: "A large ASCII-art digital clock display",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["5x5 block font", "Optional date display", "Blinking colon"],
    },
    ScreensaverInfo {
        name: "game_of_life",
        description: "Conway's Game of Life with age coloring and heat map",
        key_bindings: &[
            ("Space", "Pause/resume"),
            ("r", "Randomize"),
            ("n", "Step one generation"),
            ("h", "Toggle heat map"),
        ],
        features: &["Interactive controls", "Age-based coloring", "Heat map visualization", "Stagnation detection"],
    },
    ScreensaverInfo {
        name: "analog_clock",
        description: "An analog clock face with hour, minute, and second hands",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["Trigonometric rendering", "Timezone support", "Second hand toggle"],
    },
    ScreensaverInfo {
        name: "lissajous",
        description: "Evolving mathematical curves with Braille subpixel rendering",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["Braille subpixel rendering", "Morphing curves", "Rainbow/theme coloring"],
    },
    ScreensaverInfo {
        name: "fluid",
        description: "Real-time 2D fluid dynamics with mouse interaction",
        key_bindings: &[
            ("r", "Reset simulation"),
            ("Mouse drag", "Stir fluid and inject dye"),
        ],
        features: &["Navier-Stokes solver", "Mouse interaction", "Configurable viscosity"],
    },
    ScreensaverInfo {
        name: "terrain",
        description: "Procedural 3D terrain flyover with depth haze",
        key_bindings: &[("Any key", "Exit screensaver")],
        features: &["Procedural generation", "Depth fog", "Configurable altitude and speed"],
    },
    ScreensaverInfo {
        name: "random",
        description: "Randomly selects a screensaver each time",
        key_bindings: &[],
        features: &["Picks from all available screensavers"],
    },
];

/// Create a screensaver by name.
pub fn create(name: &str, config: &Config) -> Option<Box<dyn Screensaver>> {
    let theme = Theme::from_name(&config.theme.name);
    match name {
        "bouncing_ball" => Some(Box::new(BouncingBall::new(
            &config.screensavers.bouncing_ball,
            theme,
        ))),
        "starfield" => Some(Box::new(Starfield::new(
            &config.screensavers.starfield,
            theme,
        ))),
        "matrix_rain" => Some(Box::new(MatrixRain::new(
            &config.screensavers.matrix_rain,
            theme,
        ))),
        "pipes" => Some(Box::new(Pipes::new(&config.screensavers.pipes, theme))),
        "digital_clock" => Some(Box::new(DigitalClock::new(
            &config.screensavers.digital_clock,
            theme,
        ))),
        "game_of_life" => Some(Box::new(GameOfLife::new(
            &config.screensavers.game_of_life,
            theme,
        ))),
        "analog_clock" => Some(Box::new(AnalogClock::new(
            &config.screensavers.analog_clock,
            theme,
        ))),
        "lissajous" => Some(Box::new(Lissajous::new(
            &config.screensavers.lissajous,
            theme,
        ))),
        "fluid" => Some(Box::new(Fluid::new(&config.screensavers.fluid, theme))),
        "terrain" => Some(Box::new(Terrain::new(
            &config.screensavers.terrain,
            theme,
        ))),
        "random" => Some(create_random(config)),
        _ => None,
    }
}

/// Create a random screensaver (excludes "random" itself to avoid recursion).
pub fn create_random(config: &Config) -> Box<dyn Screensaver> {
    use rand::Rng;
    let mut rng = rand::rng();
    let real_savers: Vec<&ScreensaverInfo> = SCREENSAVERS
        .iter()
        .filter(|s| s.name != "random")
        .collect();
    let idx = rng.random_range(0..real_savers.len());
    let theme = Theme::from_name(&config.theme.name);
    match real_savers[idx].name {
        "bouncing_ball" => Box::new(BouncingBall::new(
            &config.screensavers.bouncing_ball,
            theme,
        )),
        "starfield" => Box::new(Starfield::new(&config.screensavers.starfield, theme)),
        "matrix_rain" => Box::new(MatrixRain::new(&config.screensavers.matrix_rain, theme)),
        "pipes" => Box::new(Pipes::new(&config.screensavers.pipes, theme)),
        "digital_clock" => Box::new(DigitalClock::new(
            &config.screensavers.digital_clock,
            theme,
        )),
        "game_of_life" => Box::new(GameOfLife::new(
            &config.screensavers.game_of_life,
            theme,
        )),
        "analog_clock" => Box::new(AnalogClock::new(
            &config.screensavers.analog_clock,
            theme,
        )),
        "lissajous" => Box::new(Lissajous::new(&config.screensavers.lissajous, theme)),
        "fluid" => Box::new(Fluid::new(&config.screensavers.fluid, theme)),
        "terrain" => Box::new(Terrain::new(&config.screensavers.terrain, theme)),
        _ => Box::new(Starfield::new(&config.screensavers.starfield, theme)),
    }
}

/// Get all screensaver names.
#[allow(dead_code)]
pub fn names() -> Vec<&'static str> {
    SCREENSAVERS.iter().map(|s| s.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_screensavers_constructible() {
        let config = Config::default();
        for info in SCREENSAVERS {
            let saver = create(info.name, &config);
            assert!(saver.is_some(), "Failed to create: {}", info.name);
            let saver = saver.unwrap();
            if info.name != "random" {
                assert_eq!(saver.name(), info.name);
            }
        }
    }

    #[test]
    fn unknown_returns_none() {
        let config = Config::default();
        assert!(create("nonexistent", &config).is_none());
    }

    #[test]
    fn names_returns_all() {
        let all = names();
        assert_eq!(all.len(), SCREENSAVERS.len());
        assert!(all.contains(&"bouncing_ball"));
        assert!(all.contains(&"game_of_life"));
        assert!(all.contains(&"lissajous"));
        assert!(all.contains(&"fluid"));
        assert!(all.contains(&"terrain"));
    }

    #[test]
    fn random_returns_valid() {
        let config = Config::default();
        let saver = create_random(&config);
        assert!(names().contains(&saver.name()));
    }
}
