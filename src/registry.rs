use crate::config::Config;
use crate::screensaver::Screensaver;
use crate::screensavers::{
    bouncing_ball::BouncingBall, digital_clock::DigitalClock, game_of_life::GameOfLife,
    matrix_rain::MatrixRain, pipes::Pipes, starfield::Starfield,
};
use crate::theme::Theme;

/// Information about a registered screensaver.
pub struct ScreensaverInfo {
    pub name: &'static str,
    pub description: &'static str,
}

/// All available screensavers.
pub const SCREENSAVERS: &[ScreensaverInfo] = &[
    ScreensaverInfo {
        name: "bouncing_ball",
        description: "A ball bouncing around the screen with a colorful trail",
    },
    ScreensaverInfo {
        name: "starfield",
        description: "A parallax starfield flying through space",
    },
    ScreensaverInfo {
        name: "matrix_rain",
        description: "Falling characters in the style of The Matrix",
    },
    ScreensaverInfo {
        name: "pipes",
        description: "Random pipe drawing with colorful box-drawing characters",
    },
    ScreensaverInfo {
        name: "digital_clock",
        description: "A large ASCII-art digital clock display",
    },
    ScreensaverInfo {
        name: "game_of_life",
        description: "Conway's Game of Life cellular automaton",
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
        _ => None,
    }
}

/// Create a random screensaver.
pub fn create_random(config: &Config) -> Box<dyn Screensaver> {
    use rand::Rng;
    let mut rng = rand::rng();
    let idx = rng.random_range(0..SCREENSAVERS.len());
    create(SCREENSAVERS[idx].name, config).unwrap()
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
            assert_eq!(saver.name(), info.name);
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
    }

    #[test]
    fn random_returns_valid() {
        let config = Config::default();
        let saver = create_random(&config);
        assert!(names().contains(&saver.name()));
    }
}
