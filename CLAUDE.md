# Phosphor

Terminal screensaver system for bash. Rust workspace with a host binary (ratatui + crossterm) and external plugin binaries communicating over stdin/stdout pipes.

## Quick Reference

```bash
cargo build --workspace                        # Build everything
cargo build -p phosphor --no-default-features # Build host without PAM
cargo test --workspace                         # Run all tests
cargo clippy --workspace                       # Lint
cargo run -p phosphor -- run                  # Run default screensaver
cargo run -p phosphor -- run --screensaver pipes  # Run specific screensaver
cargo run -p phosphor -- preview plasma --duration 5
cargo run -p phosphor -- list                 # List all discovered plugins
cargo run -p phosphor -- config               # Interactive TUI config
cargo run -p phosphor -- gallery              # Live gallery of all screensavers
cargo run -p phosphor -- init bash            # Print shell integration script
```

## Architecture

```
phosphor/                              # Cargo workspace
  Cargo.toml                            # workspace members
  crates/
    phosphor/                          # Host binary (orchestrator)
      src/
        main.rs       CLI (clap) dispatches to subcommands
        app.rs        Event loop: raw mode, FPS, auto-rotate, blanking, fade, overlay, lock
        plugin_host.rs PluginRenderer trait + SubprocessPlugin (stdin/stdout pipes)
        registry.rs   Filesystem plugin discovery: phosphor-saver-* binaries
        config.rs     TOML config, screensaver configs as HashMap<String, toml::Value>
        menu.rs       Interactive TUI config, generic config editing via --info metadata
        gallery.rs    Live mosaic of all plugins
        compositor.rs Split-screen dual plugin rendering
        auth.rs       PAM password authentication (feature-gated)
        overlay.rs    Clock overlay, lock message, password prompt
        fade.rs       Fade-to/from-black transitions
        theme.rs      5 color palettes
        screensaver.rs EventResponse enum
    phosphor-plugin/                   # SDK library for plugin authors
      src/
        lib.rs        ScreensaverPlugin trait
        buffer.rs     PluginBuffer + Cell + PluginColor (replaces ratatui Frame)
        protocol.rs   Binary wire protocol (host <-> plugin over pipes)
        runner.rs     Plugin main loop (stdin/stdout message dispatch)
        config_meta.rs PluginInfo + ConfigField for --info discovery
        theme.rs      Theme palettes (same as host, for plugins)
  savers/                               # 13 screensaver plugin binaries
    analog_clock/  aurora/  bouncing_ball/  digital_clock/  fluid/
    game_of_life/  lissajous/  matrix_rain/  maze/  pipes/
    plasma/  starfield/  terrain/
```

## Plugin Architecture

All screensavers are external binaries (`phosphor-saver-*`). The host discovers them, queries metadata via `--info`, and communicates via a binary protocol over stdin/stdout pipes.

**Wire protocol**: `[u8 tag] [u32 payload_len LE] [payload...]`
- Host -> Plugin: Init, Update, Render, KeyEvent, MouseEvent, Config, Shutdown
- Plugin -> Host: Frame (cell data), EventResponse

**Discovery paths** (in order):
1. `~/.config/phosphor/plugins/`
2. `/usr/local/share/phosphor/plugins/`
3. `/usr/share/phosphor/plugins/`
4. Same directory as the host binary

## Adding a New Screensaver

1. Create `savers/<name>/Cargo.toml` with `phosphor-plugin` dependency
2. Create `savers/<name>/src/main.rs`:
   - Define config struct with `#[serde(default)]` + `Default` impl
   - Implement `ScreensaverPlugin` trait (init, update, render, info)
   - Call `phosphor_plugin::run(MyPlugin::new(config))` in main
3. Add to workspace members in root `Cargo.toml`
4. Build: `cargo build --workspace`

The plugin binary name must start with `phosphor-saver-`. No host code changes needed.

## ScreensaverPlugin Trait

```rust
fn info(&self) -> PluginInfo;                    // Metadata for --info discovery
fn init(&mut self, width: u16, height: u16);     // Called on start AND resize
fn update(&mut self, dt: Duration);              // Framerate-independent: use dt
fn render(&self, buf: &mut PluginBuffer);        // Render into buffer, not Frame
fn handle_key(&mut self, key: KeyInput) -> EventResponse;   // Optional
fn handle_mouse(&mut self, mouse: MouseInput) -> EventResponse; // Optional
```

- `render` uses `PluginBuffer` (not ratatui Frame): `buf.set(x, y, Cell { ch, fg, bg })`
- Colors use `PluginColor::Rgb(r, g, b)` or `PluginColor::Reset`
- `info()` must return `PluginInfo` with `config_fields` for the TUI config menu

## Features (Cargo)

- `pam` (default): Enables password lock via Linux PAM. Build without: `cargo build -p phosphor --no-default-features`

## Testing

- **Host tests**: `cargo test -p phosphor` — config, menu, overlay, gallery layout, plugin discovery
- **SDK tests**: `cargo test -p phosphor-plugin` — buffer, protocol, runner, theme
- **Plugin tests**: `cargo test -p phosphor-saver-<name>` — per-screensaver logic
- **All tests**: `cargo test --workspace`
- **Snapshots**: `tests/snapshot_tests.rs` using `insta` + TestBackend

## Config Conventions

- TOML at `~/.config/phosphor/config.toml`
- Host config structs: `#[derive(Debug, Clone, Serialize, Deserialize)]` + `#[serde(default)]`
- Per-screensaver config in `[screensavers.<name>]` TOML section (stored as `HashMap<String, toml::Value>`)
- Precedence: CLI flags > TOML file > built-in defaults
- Plugin configs are sent as JSON on subprocess startup

## Terminal Safety

- Always enter alternate screen before rendering
- Always restore terminal on exit (raw mode off, alternate screen off, cursor shown)
- The `app::run()` function handles setup/teardown — plugins should NOT touch terminal state
- If a plugin crashes, the host restores terminal state and exits gracefully
