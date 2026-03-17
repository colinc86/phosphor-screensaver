use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub total_sessions: u64,
    pub total_runtime_secs: f64,
    pub first_use: Option<String>, // ISO 8601 date
    pub last_active_date: Option<String>,
    pub streak_days: u64,
    pub screensavers: HashMap<String, ScreensaverStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScreensaverStats {
    pub sessions: u64,
    pub runtime_secs: f64,
}

impl Stats {
    pub fn load() -> Self {
        Self::stats_path()
            .and_then(|path| std::fs::read_to_string(&path).ok())
            .and_then(|contents| toml::from_str(&contents).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::stats_path().ok_or("could not determine stats path")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    pub fn stats_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("phosphor").join("stats.toml"))
    }

    /// Record a completed session
    pub fn record_session(&mut self, screensaver_name: &str, duration: Duration) {
        let secs = duration.as_secs_f64();
        self.total_sessions += 1;
        self.total_runtime_secs += secs;

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        if self.first_use.is_none() {
            self.first_use = Some(today.clone());
        }

        // Update streak
        if let Some(ref last) = self.last_active_date {
            if *last == today {
                // Same day, streak unchanged
            } else {
                // Check if yesterday
                let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
                    .format("%Y-%m-%d")
                    .to_string();
                if *last == yesterday {
                    self.streak_days += 1;
                } else {
                    self.streak_days = 1;
                }
            }
        } else {
            self.streak_days = 1;
        }
        self.last_active_date = Some(today);

        // Update per-screensaver stats
        let entry = self
            .screensavers
            .entry(screensaver_name.to_string())
            .or_default();
        entry.sessions += 1;
        entry.runtime_secs += secs;
    }

    /// Format stats for display
    pub fn format_summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push("Phosphor Usage Statistics".to_string());
        lines.push("=".repeat(40));
        lines.push(String::new());
        lines.push(format!("Total sessions: {}", self.total_sessions));
        lines.push(format!(
            "Total runtime:  {}",
            format_duration(self.total_runtime_secs)
        ));
        if let Some(ref first) = self.first_use {
            lines.push(format!("First use:      {first}"));
        }
        if let Some(ref last) = self.last_active_date {
            lines.push(format!("Last active:    {last}"));
        }
        lines.push(format!("Current streak: {} day(s)", self.streak_days));
        lines.push(String::new());

        if !self.screensavers.is_empty() {
            lines.push("Per-screensaver:".to_string());
            lines.push(format!(
                "  {:<20} {:>8} {:>12}",
                "Name", "Sessions", "Runtime"
            ));
            lines.push(format!("  {}", "-".repeat(44)));

            let mut savers: Vec<_> = self.screensavers.iter().collect();
            savers.sort_by(|a, b| {
                b.1.runtime_secs
                    .partial_cmp(&a.1.runtime_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for (name, stats) in savers {
                lines.push(format!(
                    "  {:<20} {:>8} {:>12}",
                    name,
                    stats.sessions,
                    format_duration(stats.runtime_secs)
                ));
            }
        }

        lines.join("\n")
    }
}

fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stats() {
        let stats = Stats::default();
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.total_runtime_secs, 0.0);
        assert!(stats.screensavers.is_empty());
    }

    #[test]
    fn record_session_increments() {
        let mut stats = Stats::default();
        stats.record_session("starfield", Duration::from_secs(60));
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.screensavers["starfield"].sessions, 1);
        assert!((stats.screensavers["starfield"].runtime_secs - 60.0).abs() < 0.01);
    }

    #[test]
    fn record_multiple_sessions() {
        let mut stats = Stats::default();
        stats.record_session("starfield", Duration::from_secs(30));
        stats.record_session("matrix_rain", Duration::from_secs(45));
        stats.record_session("starfield", Duration::from_secs(20));
        assert_eq!(stats.total_sessions, 3);
        assert_eq!(stats.screensavers["starfield"].sessions, 2);
        assert_eq!(stats.screensavers["matrix_rain"].sessions, 1);
    }

    #[test]
    fn format_duration_short() {
        assert_eq!(format_duration(5.0), "5s");
        assert_eq!(format_duration(65.0), "1m 5s");
        assert_eq!(format_duration(3661.0), "1h 1m 1s");
    }

    #[test]
    fn format_summary_nonempty() {
        let mut stats = Stats::default();
        stats.record_session("starfield", Duration::from_secs(120));
        let summary = stats.format_summary();
        assert!(summary.contains("starfield"));
        assert!(summary.contains("1")); // 1 session
    }

    #[test]
    fn round_trip_toml() {
        let mut stats = Stats::default();
        stats.record_session("pipes", Duration::from_secs(100));
        let serialized = toml::to_string_pretty(&stats).unwrap();
        let deserialized: Stats = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.total_sessions, 1);
        assert_eq!(deserialized.screensavers["pipes"].sessions, 1);
    }
}
