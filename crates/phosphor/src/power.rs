use std::fs;
use std::path::Path;

/// Current power state of the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// Connected to AC power.
    AC,
    /// Running on battery.
    Battery,
    /// Could not determine power state.
    Unknown,
}

/// Detect the current power state by reading sysfs.
pub fn detect_power_state() -> PowerState {
    detect_power_state_from("/sys/class/power_supply")
}

/// Testable version that accepts a custom sysfs path.
pub fn detect_power_state_from(base_path: &str) -> PowerState {
    let base = Path::new(base_path);
    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return PowerState::Unknown,
    };

    for entry in entries.flatten() {
        let type_path = entry.path().join("type");
        let type_content = match fs::read_to_string(&type_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if type_content.trim() != "Battery" {
            continue;
        }

        let status_path = entry.path().join("status");
        let status = match fs::read_to_string(&status_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        return match status.trim() {
            "Discharging" => PowerState::Battery,
            "Charging" | "Full" | "Not charging" => PowerState::AC,
            _ => PowerState::Unknown,
        };
    }

    // No battery found — probably a desktop, treat as AC
    PowerState::AC
}

/// Get the appropriate FPS based on power state and config.
pub fn fps_for_power_state(
    state: PowerState,
    battery_fps: u32,
    ac_fps: u32,
    battery_aware: bool,
) -> u32 {
    if !battery_aware {
        return ac_fps;
    }
    match state {
        PowerState::Battery => battery_fps,
        PowerState::AC | PowerState::Unknown => ac_fps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_power_supply(dir: &Path, name: &str, supply_type: &str, status: &str) {
        let supply_dir = dir.join(name);
        fs::create_dir_all(&supply_dir).unwrap();
        fs::write(supply_dir.join("type"), supply_type).unwrap();
        fs::write(supply_dir.join("status"), status).unwrap();
    }

    #[test]
    fn detects_battery_discharging() {
        let tmp = TempDir::new().unwrap();
        create_power_supply(tmp.path(), "BAT0", "Battery", "Discharging");
        let state = detect_power_state_from(tmp.path().to_str().unwrap());
        assert_eq!(state, PowerState::Battery);
    }

    #[test]
    fn detects_battery_charging_as_ac() {
        let tmp = TempDir::new().unwrap();
        create_power_supply(tmp.path(), "BAT0", "Battery", "Charging");
        let state = detect_power_state_from(tmp.path().to_str().unwrap());
        assert_eq!(state, PowerState::AC);
    }

    #[test]
    fn detects_battery_full_as_ac() {
        let tmp = TempDir::new().unwrap();
        create_power_supply(tmp.path(), "BAT0", "Battery", "Full");
        let state = detect_power_state_from(tmp.path().to_str().unwrap());
        assert_eq!(state, PowerState::AC);
    }

    #[test]
    fn no_battery_returns_ac() {
        let tmp = TempDir::new().unwrap();
        create_power_supply(tmp.path(), "AC0", "Mains", "Online");
        let state = detect_power_state_from(tmp.path().to_str().unwrap());
        assert_eq!(state, PowerState::AC);
    }

    #[test]
    fn missing_dir_returns_unknown() {
        let state = detect_power_state_from("/nonexistent/path");
        assert_eq!(state, PowerState::Unknown);
    }

    #[test]
    fn fps_selection() {
        assert_eq!(fps_for_power_state(PowerState::Battery, 15, 30, true), 15);
        assert_eq!(fps_for_power_state(PowerState::AC, 15, 30, true), 30);
        assert_eq!(fps_for_power_state(PowerState::Unknown, 15, 30, true), 30);
        assert_eq!(fps_for_power_state(PowerState::Battery, 15, 30, false), 30);
    }
}
