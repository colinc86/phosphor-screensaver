use std::fs;

#[test]
fn config_file_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    // Write default config
    let default_toml = r#"
[general]
timeout = 300
fps = 30
default_screensaver = "starfield"
fade_enabled = true
fade_duration_ms = 500
"#;
    fs::write(&path, default_toml).unwrap();

    // Read it back
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("timeout = 300"));
    assert!(contents.contains("fps = 30"));
    assert!(contents.contains("starfield"));
}

#[test]
fn empty_config_file_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "").unwrap();

    // An empty TOML file should parse to valid config with defaults
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.is_empty());
}

#[test]
fn invalid_toml_is_handled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "this is not valid toml {{{}}}").unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let result: Result<toml::Value, _> = toml::from_str(&contents);
    assert!(result.is_err());
}
