use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub rotate: RotateConfig,
    pub overlay: OverlayConfig,
    pub blanking: BlankingConfig,
    pub power: PowerConfig,
    pub theme: ThemeConfig,
    pub schedule: ScheduleConfig,
    pub compositor: CompositorConfig,
    pub screensavers: ScreensaverConfigs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub timeout: u64,
    pub fps: u32,
    pub default_screensaver: String,
    pub fade_enabled: bool,
    pub fade_duration_ms: u64,
    pub ambient_enabled: bool,
    pub ambient_intensity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RotateConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub screensavers: Vec<String>,
    pub transition_style: String,
    pub transition_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub clock: ClockOverlayConfig,
    pub lock: LockOverlayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockOverlayConfig {
    pub enabled: bool,
    pub position: String,
    pub format: String,
    pub show_seconds: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LockOverlayConfig {
    pub enabled: bool,
    pub message: String,
    pub password_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BlankingConfig {
    pub enabled: bool,
    pub delay_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerConfig {
    pub battery_aware: bool,
    pub battery_fps: u32,
    pub ac_fps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleConfig {
    pub enabled: bool,
    pub morning: String,    // 6am-12pm
    pub afternoon: String,  // 12pm-6pm
    pub evening: String,    // 6pm-10pm
    pub night: String,      // 10pm-6am
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreensaverConfigs {
    pub bouncing_ball: BouncingBallConfig,
    pub starfield: StarfieldConfig,
    pub matrix_rain: MatrixRainConfig,
    pub pipes: PipesConfig,
    pub digital_clock: DigitalClockConfig,
    pub game_of_life: GameOfLifeConfig,
    pub analog_clock: AnalogClockConfig,
    pub lissajous: LissajousConfig,
    pub fluid: FluidConfig,
    pub terrain: TerrainConfig,
    pub aurora: AuroraConfig,
    pub plasma: PlasmaConfig,
    pub maze: MazeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BouncingBallConfig {
    pub trail_length: usize,
    pub ball_char: String,
    pub color: String,
    pub ball_count: usize,
    pub gravity: String,
    pub trail_style: String,
    pub collisions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StarfieldConfig {
    pub star_count: usize,
    pub speed: f64,
    pub streak_enabled: bool,
    pub warp_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MatrixRainConfig {
    pub density: f64,
    pub speed: f64,
    pub charset: String,
    pub depth_layers: bool,
    pub glitch_wave: bool,
    pub glitch_interval_secs: u64,
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipesConfig {
    pub palette: String,
    pub turn_probability: f64,
    pub junctions: bool,
    pub flow_enabled: bool,
    pub flow_speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DigitalClockConfig {
    pub style: String,
    pub show_date: bool,
    pub color: String,
    pub color_cycle: bool,
    pub color_cycle_speed: f64,
    pub mode: String,
    pub countdown_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameOfLifeConfig {
    pub initial_density: f64,
    pub cell_char: String,
    pub auto_reset_secs: u64,
    pub color_by_age: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalogClockConfig {
    pub show_seconds: bool,
    pub show_date: bool,
    pub color: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LissajousConfig {
    pub speed: f64,
    pub trail_length: usize,
    pub damping: f64,
    pub color_mode: String,
    pub morph_interval_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FluidConfig {
    pub viscosity: f64,
    pub diffusion: f64,
    pub force_strength: f64,
    pub interactive: bool,
    pub color_mode: String,
    pub solver_iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerrainConfig {
    pub speed: f64,
    pub altitude: f64,
    pub draw_distance: f64,
    pub palette: String,
    pub fog: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuroraConfig {
    pub ribbon_count: usize,
    pub speed: f64,
    pub intensity: f64,
    pub palette: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlasmaConfig {
    pub speed: f64,
    pub scale: f64,
    pub mode: String,
    pub color_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MazeConfig {
    pub generation_speed: f64,
    pub solve_speed: f64,
    pub wall_style: String,
    pub show_exploration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositorConfig {
    pub divider_style: String,
    pub split_ratio: f64,
}

// --- Default implementations ---

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            rotate: RotateConfig::default(),
            overlay: OverlayConfig::default(),
            blanking: BlankingConfig::default(),
            power: PowerConfig::default(),
            theme: ThemeConfig::default(),
            schedule: ScheduleConfig::default(),
            compositor: CompositorConfig::default(),
            screensavers: ScreensaverConfigs::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            timeout: 300,
            fps: 30,
            default_screensaver: "starfield".into(),
            fade_enabled: true,
            fade_duration_ms: 500,
            ambient_enabled: false,
            ambient_intensity: 0.5,
        }
    }
}

impl Default for RotateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 120,
            screensavers: vec![
                "starfield".into(),
                "matrix_rain".into(),
                "pipes".into(),
            ],
            transition_style: "dissolve".into(),
            transition_duration_ms: 800,
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            clock: ClockOverlayConfig::default(),
            lock: LockOverlayConfig::default(),
        }
    }
}

impl Default for ClockOverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "top-right".into(),
            format: "%H:%M".into(),
            show_seconds: false,
        }
    }
}

impl Default for LockOverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            message: "Press any key to return".into(),
            password_required: false,
        }
    }
}

impl Default for BlankingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_secs: 600,
        }
    }
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            battery_aware: true,
            battery_fps: 15,
            ac_fps: 30,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
        }
    }
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            morning: String::new(),
            afternoon: String::new(),
            evening: String::new(),
            night: String::new(),
        }
    }
}

impl Default for ScreensaverConfigs {
    fn default() -> Self {
        Self {
            bouncing_ball: BouncingBallConfig::default(),
            starfield: StarfieldConfig::default(),
            matrix_rain: MatrixRainConfig::default(),
            pipes: PipesConfig::default(),
            digital_clock: DigitalClockConfig::default(),
            game_of_life: GameOfLifeConfig::default(),
            analog_clock: AnalogClockConfig::default(),
            lissajous: LissajousConfig::default(),
            fluid: FluidConfig::default(),
            terrain: TerrainConfig::default(),
            aurora: AuroraConfig::default(),
            plasma: PlasmaConfig::default(),
            maze: MazeConfig::default(),
        }
    }
}

impl Default for BouncingBallConfig {
    fn default() -> Self {
        Self {
            trail_length: 20,
            ball_char: "\u{25CF}".into(),
            color: "cyan".into(),
            ball_count: 1,
            gravity: "none".into(),
            trail_style: "dots".into(),
            collisions: true,
        }
    }
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

impl Default for MatrixRainConfig {
    fn default() -> Self {
        Self {
            density: 0.8,
            speed: 1.0,
            charset: "katakana".into(),
            depth_layers: false,
            glitch_wave: false,
            glitch_interval_secs: 25,
            interactive: false,
        }
    }
}

impl Default for PipesConfig {
    fn default() -> Self {
        Self {
            palette: "rainbow".into(),
            turn_probability: 0.3,
            junctions: true,
            flow_enabled: true,
            flow_speed: 1.0,
        }
    }
}

impl Default for DigitalClockConfig {
    fn default() -> Self {
        Self {
            style: "block".into(),
            show_date: true,
            color: "green".into(),
            color_cycle: false,
            color_cycle_speed: 1.0,
            mode: "clock".into(),
            countdown_minutes: 25,
        }
    }
}

impl Default for GameOfLifeConfig {
    fn default() -> Self {
        Self {
            initial_density: 0.3,
            cell_char: "█".into(),
            auto_reset_secs: 60,
            color_by_age: true,
        }
    }
}

impl Default for AnalogClockConfig {
    fn default() -> Self {
        Self {
            show_seconds: true,
            show_date: true,
            color: "cyan".into(),
            timezone: String::new(),
        }
    }
}

impl Default for LissajousConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            trail_length: 3000,
            damping: 0.002,
            color_mode: "rainbow".into(),
            morph_interval_secs: 8.0,
        }
    }
}

impl Default for FluidConfig {
    fn default() -> Self {
        Self {
            viscosity: 0.0001,
            diffusion: 0.00001,
            force_strength: 5.0,
            interactive: true,
            color_mode: "rainbow".into(),
            solver_iterations: 4,
        }
    }
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

impl Default for MazeConfig {
    fn default() -> Self {
        Self {
            generation_speed: 1.0,
            solve_speed: 1.0,
            wall_style: "box".into(),
            show_exploration: true,
        }
    }
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            divider_style: "line".into(),
            split_ratio: 0.5,
        }
    }
}

impl Config {
    /// Load config from the default path (~/.config/termsaver/config.toml).
    /// Returns default config if the file doesn't exist.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(&path).ok())
            .and_then(|contents| toml::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// Load config from a specific path.
    #[allow(dead_code)]
    pub fn load_from(path: &std::path::Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        let config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save config to the default path.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path().ok_or_else(|| {
            ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine config directory",
            ))
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    /// Return the default config file path.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("termsaver").join("config.toml"))
    }

    /// Apply CLI overrides on top of loaded config.
    pub fn merge_cli(&mut self, screensaver: Option<&str>, fps: Option<u32>, no_fade: bool) {
        if let Some(s) = screensaver {
            self.general.default_screensaver = s.to_string();
        }
        if let Some(f) = fps {
            self.general.fps = f;
        }
        if no_fade {
            self.general.fade_enabled = false;
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Toml(toml::de::Error),
    Serialize(toml::ser::Error),
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Toml(e)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        ConfigError::Serialize(e)
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {e}"),
            ConfigError::Toml(e) => write!(f, "TOML parse error: {e}"),
            ConfigError::Serialize(e) => write!(f, "TOML serialize error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = Config::default();
        assert_eq!(config.general.timeout, 300);
        assert_eq!(config.general.fps, 30);
        assert_eq!(config.general.default_screensaver, "starfield");
        assert!(config.general.fade_enabled);
    }

    #[test]
    fn round_trip_toml() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.general.timeout, config.general.timeout);
        assert_eq!(deserialized.general.fps, config.general.fps);
        assert_eq!(
            deserialized.general.default_screensaver,
            config.general.default_screensaver
        );
    }

    #[test]
    fn partial_toml_uses_defaults() {
        let toml_str = r#"
[general]
fps = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.fps, 60);
        assert_eq!(config.general.timeout, 300); // default
        assert_eq!(config.general.default_screensaver, "starfield"); // default
    }

    #[test]
    fn merge_cli_overrides() {
        let mut config = Config::default();
        config.merge_cli(Some("pipes"), Some(60), true);
        assert_eq!(config.general.default_screensaver, "pipes");
        assert_eq!(config.general.fps, 60);
        assert!(!config.general.fade_enabled);
    }

    #[test]
    fn merge_cli_none_keeps_defaults() {
        let mut config = Config::default();
        config.merge_cli(None, None, false);
        assert_eq!(config.general.default_screensaver, "starfield");
        assert_eq!(config.general.fps, 30);
        assert!(config.general.fade_enabled);
    }

    #[test]
    fn save_and_load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config::default();
        let contents = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&path, &contents).unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.general.fps, config.general.fps);
    }
}
