/// Time-of-day ambiance modifiers.
///
/// Computes brightness, warmth, and energy multipliers based on the current hour.
/// Screensavers can query these to subtly adapt their appearance.

#[derive(Debug, Clone, Copy)]
pub struct AmbientState {
    /// Overall brightness multiplier (0.3 - 1.0)
    pub brightness: f64,
    /// Color warmth: negative = cool/blue shift, positive = warm/orange shift (-1.0 to 1.0)
    pub warmth: f64,
    /// Speed/energy multiplier (0.7 - 1.0)
    pub energy: f64,
}

impl AmbientState {
    /// Compute ambient state from the current hour (0-23) and intensity (0.0-1.0).
    /// Intensity controls how much the time-of-day affects the output.
    /// At intensity 0.0, all values are neutral (1.0, 0.0, 1.0).
    pub fn from_hour(hour: u32, intensity: f64) -> Self {
        let intensity = intensity.clamp(0.0, 1.0);
        let (raw_brightness, raw_warmth, raw_energy) = match hour {
            // Night (10pm - 4am): dim, cool, slow
            22 | 23 | 0 | 1 | 2 | 3 | 4 => (0.35, -0.7, 0.7),
            // Early morning (5am - 7am): warming up
            5 => (0.5, -0.3, 0.8),
            6 => (0.65, 0.0, 0.85),
            7 => (0.8, 0.2, 0.9),
            // Day (8am - 5pm): full vibrancy
            8..=17 => (1.0, 0.0, 1.0),
            // Evening (6pm - 9pm): warm, gradually dimming
            18 => (0.9, 0.3, 0.95),
            19 => (0.8, 0.5, 0.9),
            20 => (0.6, 0.4, 0.85),
            21 => (0.5, 0.2, 0.8),
            _ => (1.0, 0.0, 1.0),
        };

        // Blend toward neutral (1.0, 0.0, 1.0) based on intensity
        Self {
            brightness: 1.0 + (raw_brightness - 1.0) * intensity,
            warmth: raw_warmth * intensity,
            energy: 1.0 + (raw_energy - 1.0) * intensity,
        }
    }

    /// Compute from the current system time.
    pub fn from_now(intensity: f64) -> Self {
        let hour = chrono::Local::now().hour();
        Self::from_hour(hour, intensity)
    }

    /// Apply brightness modifier to an RGB color value.
    pub fn apply_color(&self, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        let mut rf = r as f64 * self.brightness;
        let gf = g as f64 * self.brightness;
        let mut bf = b as f64 * self.brightness;

        // Apply warmth shift
        if self.warmth > 0.0 {
            // Warm: boost red, reduce blue
            rf *= 1.0 + self.warmth * 0.2;
            bf *= 1.0 - self.warmth * 0.15;
        } else if self.warmth < 0.0 {
            // Cool: boost blue, reduce red
            bf *= 1.0 + (-self.warmth) * 0.2;
            rf *= 1.0 - (-self.warmth) * 0.15;
        }

        (
            (rf.clamp(0.0, 255.0)) as u8,
            (gf.clamp(0.0, 255.0)) as u8,
            (bf.clamp(0.0, 255.0)) as u8,
        )
    }

    /// Apply energy modifier to a speed value.
    #[allow(dead_code)]
    pub fn apply_speed(&self, speed: f64) -> f64 {
        speed * self.energy
    }

    /// Create a neutral state (no modification).
    pub fn neutral() -> Self {
        Self {
            brightness: 1.0,
            warmth: 0.0,
            energy: 1.0,
        }
    }

    /// Check if this state would have no visible effect.
    pub fn is_neutral(&self) -> bool {
        (self.brightness - 1.0).abs() < f64::EPSILON
            && self.warmth.abs() < f64::EPSILON
            && (self.energy - 1.0).abs() < f64::EPSILON
    }

    /// Apply brightness/warmth to a ratatui Color, passing through non-Rgb variants.
    pub fn apply_ratatui_color(&self, color: ratatui::style::Color) -> ratatui::style::Color {
        match color {
            ratatui::style::Color::Rgb(r, g, b) => {
                let (r, g, b) = self.apply_color(r, g, b);
                ratatui::style::Color::Rgb(r, g, b)
            }
            other => other,
        }
    }
}

use chrono::Timelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daytime_is_full_brightness() {
        let state = AmbientState::from_hour(12, 1.0);
        assert_eq!(state.brightness, 1.0);
        assert_eq!(state.warmth, 0.0);
        assert_eq!(state.energy, 1.0);
    }

    #[test]
    fn nighttime_is_dim() {
        let state = AmbientState::from_hour(2, 1.0);
        assert!(state.brightness < 0.5);
        assert!(state.warmth < 0.0); // cool
        assert!(state.energy < 1.0);
    }

    #[test]
    fn evening_is_warm() {
        let state = AmbientState::from_hour(19, 1.0);
        assert!(state.warmth > 0.0);
    }

    #[test]
    fn zero_intensity_is_neutral() {
        let state = AmbientState::from_hour(2, 0.0);
        assert_eq!(state.brightness, 1.0);
        assert_eq!(state.warmth, 0.0);
        assert_eq!(state.energy, 1.0);
    }

    #[test]
    fn half_intensity_partial_effect() {
        let full = AmbientState::from_hour(2, 1.0);
        let half = AmbientState::from_hour(2, 0.5);
        // Half intensity should be between neutral and full
        assert!(half.brightness > full.brightness);
        assert!(half.brightness < 1.0);
    }

    #[test]
    fn apply_color_at_night() {
        let state = AmbientState::from_hour(2, 1.0);
        let (r, _g, b) = state.apply_color(200, 200, 200);
        // Should be dimmer and cooler (blue shift)
        assert!(r < 200);
        assert!(b > r); // blue boosted relative to red
    }

    #[test]
    fn apply_color_daytime_unchanged() {
        let state = AmbientState::from_hour(12, 1.0);
        let (r, g, b) = state.apply_color(200, 200, 200);
        assert_eq!(r, 200);
        assert_eq!(g, 200);
        assert_eq!(b, 200);
    }

    #[test]
    fn apply_speed_night() {
        let state = AmbientState::from_hour(2, 1.0);
        let speed = state.apply_speed(10.0);
        assert!(speed < 10.0);
    }

    #[test]
    fn apply_speed_day() {
        let state = AmbientState::from_hour(12, 1.0);
        let speed = state.apply_speed(10.0);
        assert_eq!(speed, 10.0);
    }

    #[test]
    fn color_clamps_to_valid_range() {
        let state = AmbientState::from_hour(19, 1.0); // warm evening
        let (r, _g, b) = state.apply_color(255, 255, 255);
        // Just verify it doesn't panic and produces valid u8 values
        let _ = (r, b);
    }

    #[test]
    fn from_now_does_not_panic() {
        let _ = AmbientState::from_now(0.5);
    }

    #[test]
    fn morning_ramp_up() {
        let h5 = AmbientState::from_hour(5, 1.0);
        let h7 = AmbientState::from_hour(7, 1.0);
        assert!(h7.brightness > h5.brightness);
    }

    #[test]
    fn neutral_is_identity() {
        let state = AmbientState::neutral();
        assert!(state.is_neutral());
        let (r, g, b) = state.apply_color(100, 150, 200);
        assert_eq!((r, g, b), (100, 150, 200));
    }

    #[test]
    fn apply_ratatui_color_rgb() {
        let state = AmbientState::from_hour(2, 1.0); // nighttime
        let color = ratatui::style::Color::Rgb(200, 200, 200);
        let result = state.apply_ratatui_color(color);
        if let ratatui::style::Color::Rgb(r, _g, _b) = result {
            assert!(r < 200);
        } else {
            panic!("expected Rgb variant");
        }
    }

    #[test]
    fn apply_ratatui_color_named_passthrough() {
        let state = AmbientState::from_hour(2, 1.0);
        let color = ratatui::style::Color::Red;
        assert_eq!(state.apply_ratatui_color(color), ratatui::style::Color::Red);
    }

    #[test]
    fn daytime_is_neutral() {
        let state = AmbientState::from_hour(12, 1.0);
        assert!(state.is_neutral());
    }

    #[test]
    fn nighttime_is_not_neutral() {
        let state = AmbientState::from_hour(2, 1.0);
        assert!(!state.is_neutral());
    }
}
