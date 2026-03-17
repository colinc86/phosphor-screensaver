use ratatui::style::Color;

/// Named color theme palettes.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub dim: Color,
    pub bright: Color,
    pub background: Color,
    pub text: Color,
    pub highlight: Color,
}

#[allow(dead_code)]
impl Theme {
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
            primary: Color::Rgb(200, 145, 30),
            secondary: Color::Rgb(160, 110, 20),
            accent: Color::Rgb(255, 180, 40),
            dim: Color::Rgb(80, 60, 15),
            bright: Color::Rgb(255, 220, 140),
            background: Color::Rgb(10, 8, 5),
            text: Color::Rgb(220, 175, 80),
            highlight: Color::Rgb(255, 200, 60),
        }
    }

    pub fn neon() -> Self {
        Self {
            primary: Color::Rgb(0, 255, 255),
            secondary: Color::Rgb(255, 0, 255),
            accent: Color::Rgb(255, 255, 0),
            dim: Color::Rgb(40, 40, 40),
            bright: Color::Rgb(255, 255, 255),
            background: Color::Black,
            text: Color::Rgb(200, 200, 200),
            highlight: Color::Rgb(0, 255, 128),
        }
    }

    pub fn pastel() -> Self {
        Self {
            primary: Color::Rgb(173, 216, 230),
            secondary: Color::Rgb(152, 251, 152),
            accent: Color::Rgb(255, 182, 193),
            dim: Color::Rgb(60, 60, 70),
            bright: Color::Rgb(240, 240, 245),
            background: Color::Black,
            text: Color::Rgb(200, 200, 210),
            highlight: Color::Rgb(230, 230, 250),
        }
    }

    pub fn monochrome() -> Self {
        Self {
            primary: Color::White,
            secondary: Color::Gray,
            accent: Color::White,
            dim: Color::DarkGray,
            bright: Color::White,
            background: Color::Black,
            text: Color::Gray,
            highlight: Color::White,
        }
    }

    pub fn dracula() -> Self {
        Self {
            primary: Color::Rgb(189, 147, 249),
            secondary: Color::Rgb(80, 250, 123),
            accent: Color::Rgb(255, 121, 198),
            dim: Color::Rgb(68, 71, 90),
            bright: Color::Rgb(248, 248, 242),
            background: Color::Rgb(40, 42, 54),
            text: Color::Rgb(248, 248, 242),
            highlight: Color::Rgb(255, 184, 108),
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
        for name in Theme::available() {
            let theme = Theme::from_name(name);
            // Verify background is dark (not the same as primary)
            assert_ne!(theme.primary, theme.background);
        }
    }

    #[test]
    fn unknown_theme_returns_default() {
        let theme = Theme::from_name("nonexistent");
        let default = Theme::default_theme();
        assert_eq!(theme.primary, default.primary);
    }
}
