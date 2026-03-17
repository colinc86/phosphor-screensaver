//! Theme colors shared between host and plugins.
//! Plugins can use `ThemeColors::from_name()` to get the same palettes as the host.

use crate::buffer::PluginColor;

/// A color theme palette.
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub primary: PluginColor,
    pub secondary: PluginColor,
    pub accent: PluginColor,
    pub dim: PluginColor,
    pub bright: PluginColor,
    pub background: PluginColor,
    pub text: PluginColor,
    pub highlight: PluginColor,
}

impl ThemeColors {
    pub fn from_name(name: &str) -> Self {
        match name {
            "neon" => Self::neon(),
            "pastel" => Self::pastel(),
            "monochrome" => Self::monochrome(),
            "dracula" => Self::dracula(),
            _ => Self::default_theme(),
        }
    }

    pub fn default_theme() -> Self {
        Self {
            primary: PluginColor::Rgb(200, 145, 30),
            secondary: PluginColor::Rgb(160, 110, 20),
            accent: PluginColor::Rgb(255, 180, 40),
            dim: PluginColor::Rgb(80, 60, 15),
            bright: PluginColor::Rgb(255, 220, 140),
            background: PluginColor::Rgb(10, 8, 5),
            text: PluginColor::Rgb(220, 175, 80),
            highlight: PluginColor::Rgb(255, 200, 60),
        }
    }

    pub fn neon() -> Self {
        Self {
            primary: PluginColor::Rgb(0, 255, 255),
            secondary: PluginColor::Rgb(255, 0, 255),
            accent: PluginColor::Rgb(255, 255, 0),
            dim: PluginColor::Rgb(40, 40, 40),
            bright: PluginColor::Rgb(255, 255, 255),
            background: PluginColor::Rgb(0, 0, 0),
            text: PluginColor::Rgb(200, 200, 200),
            highlight: PluginColor::Rgb(0, 255, 128),
        }
    }

    pub fn pastel() -> Self {
        Self {
            primary: PluginColor::Rgb(173, 216, 230),
            secondary: PluginColor::Rgb(152, 251, 152),
            accent: PluginColor::Rgb(255, 182, 193),
            dim: PluginColor::Rgb(60, 60, 70),
            bright: PluginColor::Rgb(240, 240, 245),
            background: PluginColor::Rgb(0, 0, 0),
            text: PluginColor::Rgb(200, 200, 210),
            highlight: PluginColor::Rgb(230, 230, 250),
        }
    }

    pub fn monochrome() -> Self {
        Self {
            primary: PluginColor::Rgb(255, 255, 255),
            secondary: PluginColor::Rgb(128, 128, 128),
            accent: PluginColor::Rgb(255, 255, 255),
            dim: PluginColor::Rgb(128, 128, 128),
            bright: PluginColor::Rgb(255, 255, 255),
            background: PluginColor::Rgb(0, 0, 0),
            text: PluginColor::Rgb(128, 128, 128),
            highlight: PluginColor::Rgb(255, 255, 255),
        }
    }

    pub fn dracula() -> Self {
        Self {
            primary: PluginColor::Rgb(189, 147, 249),
            secondary: PluginColor::Rgb(80, 250, 123),
            accent: PluginColor::Rgb(255, 121, 198),
            dim: PluginColor::Rgb(68, 71, 90),
            bright: PluginColor::Rgb(248, 248, 242),
            background: PluginColor::Rgb(40, 42, 54),
            text: PluginColor::Rgb(248, 248, 242),
            highlight: PluginColor::Rgb(255, 184, 108),
        }
    }

    /// Get all available theme names.
    pub fn available() -> &'static [&'static str] {
        &["default", "neon", "pastel", "monochrome", "dracula"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_constructible() {
        for name in ThemeColors::available() {
            let theme = ThemeColors::from_name(name);
            assert_ne!(theme.primary, theme.background);
        }
    }

    #[test]
    fn unknown_theme_returns_default() {
        let theme = ThemeColors::from_name("nonexistent");
        let default = ThemeColors::default_theme();
        assert_eq!(theme.primary, default.primary);
    }
}
