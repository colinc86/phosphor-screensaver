use std::collections::HashMap;
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
    pub screensavers: HashMap<String, toml::Value>,
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
            screensavers: HashMap::new(),
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

    /// Get the screensaver config as JSON (for sending to subprocess plugins).
    pub fn screensaver_config_json(&self, name: &str) -> String {
        self.screensavers
            .get(name)
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_else(|| "{}".into())
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
