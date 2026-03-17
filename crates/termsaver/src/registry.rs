use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::config::Config;
use crate::plugin_host::{PluginRenderer, SubprocessPlugin};

/// A discovered subprocess plugin.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub path: PathBuf,
    pub info: termsaver_plugin::PluginInfo,
}

/// Cached plugin discovery (expensive: runs --info on each binary).
static DISCOVERED: OnceLock<Vec<DiscoveredPlugin>> = OnceLock::new();

/// Get the list of discovered plugins (cached after first call).
pub fn plugins() -> &'static [DiscoveredPlugin] {
    DISCOVERED.get_or_init(discover_plugins)
}

/// Get all plugin names including the "random" virtual entry.
#[allow(dead_code)]
pub fn names() -> Vec<&'static str> {
    let mut out: Vec<&str> = plugins().iter().map(|p| p.name.as_str()).collect();
    out.push("random");
    out
}

/// Find a plugin by name.
pub fn find(name: &str) -> Option<&'static DiscoveredPlugin> {
    plugins().iter().find(|p| p.name == name)
}

/// Create a PluginRenderer by name. Returns None if not found.
pub fn create_plugin(name: &str, config: &Config) -> Option<Box<dyn PluginRenderer>> {
    if name == "random" {
        return Some(create_random_plugin(config));
    }
    let plugin = find(name)?;
    let config_json = config.screensaver_config_json(name);
    SubprocessPlugin::spawn(
        plugin.path.to_str().unwrap_or(""),
        name,
        &config_json,
    )
    .ok()
    .map(|s| Box::new(s) as Box<dyn PluginRenderer>)
}

/// Create a random PluginRenderer.
pub fn create_random_plugin(config: &Config) -> Box<dyn PluginRenderer> {
    use rand::Rng;
    let all = plugins();
    if all.is_empty() {
        panic!("No screensaver plugins found. Ensure termsaver-saver-* binaries are in the same directory as termsaver.");
    }
    let mut rng = rand::rng();
    let idx = rng.random_range(0..all.len());
    let name = &all[idx].name;
    let config_json = config.screensaver_config_json(name);
    SubprocessPlugin::spawn(
        all[idx].path.to_str().unwrap_or(""),
        name,
        &config_json,
    )
    .unwrap_or_else(|e| {
        panic!("Failed to spawn random screensaver '{}': {}", name, e);
    })
    .into()
}

impl From<SubprocessPlugin> for Box<dyn PluginRenderer> {
    fn from(p: SubprocessPlugin) -> Self {
        Box::new(p)
    }
}

/// Discover subprocess plugin binaries from search paths.
fn discover_plugins() -> Vec<DiscoveredPlugin> {
    let mut plugins = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in plugin_search_paths() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if fname.starts_with("termsaver-saver-") && is_executable(&path) {
                    if let Some(info) = query_plugin_info(&path) {
                        if seen.insert(info.name.clone()) {
                            plugins.push(DiscoveredPlugin {
                                name: info.name.clone(),
                                path: path.clone(),
                                info,
                            });
                        }
                    }
                }
            }
        }
    }
    // Sort by name for consistent ordering
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

fn plugin_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // User plugin dir
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("termsaver").join("plugins"));
    }

    // System dirs
    paths.push(PathBuf::from("/usr/local/share/termsaver/plugins"));
    paths.push(PathBuf::from("/usr/share/termsaver/plugins"));

    // Same directory as host binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.to_path_buf());
        }
    }

    paths
}

fn is_executable(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn query_plugin_info(path: &PathBuf) -> Option<termsaver_plugin::PluginInfo> {
    let output = Command::new(path)
        .arg("--info")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_search_paths_includes_exe_dir() {
        let paths = plugin_search_paths();
        assert!(!paths.is_empty());
        // Should include the directory of the current executable
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));
        if let Some(dir) = exe_dir {
            assert!(paths.contains(&dir));
        }
    }

    #[test]
    fn discover_finds_plugins_in_target_dir() {
        // This test relies on the saver binaries being built in the same target dir
        let all = discover_plugins();
        // In test environment, we should find at least some plugins
        // (they're in target/debug/ alongside the test binary)
        if !all.is_empty() {
            assert!(all.iter().any(|p| !p.name.is_empty()));
            // Check dedup: no duplicate names
            let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
            let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
            assert_eq!(names.len(), unique.len());
        }
    }
}
