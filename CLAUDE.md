# Termsaver

Terminal screensaver system for bash. Rust binary (ratatui + crossterm) activated by bash's TMOUT/ALRM idle detection.

## Quick Reference

```bash
cargo build                                    # Build (with PAM support)
cargo build --no-default-features              # Build without PAM
cargo test                                     # Run all tests
cargo clippy                                   # Lint
cargo run                                      # Run default screensaver (starfield)
cargo run -- run --screensaver pipes           # Run specific screensaver
cargo run -- preview analog_clock --duration 5 # Preview for N seconds
cargo run -- list                              # List all screensavers
cargo run -- config                            # Interactive TUI config
cargo run -- init bash                         # Print shell integration script
cargo run -- print-timeout                     # Print configured timeout
```

## Architecture

```
main.rs          CLI (clap) → dispatches to subcommands
  ├─ app.rs      Main event loop: raw mode, alternate screen, FPS timing,
  │              auto-rotate, blanking, fade, overlay, password lock
  ├─ auth.rs     PAM-based password authentication (feature-gated)
  ├─ config.rs   TOML config at ~/.config/termsaver/config.toml
  │              All fields #[serde(default)], CLI overrides via merge_cli()
  ├─ registry.rs Screensaver factory: name → Box<dyn Screensaver>
  │              Static SCREENSAVERS array + create() match
  │              "random" delegates to create_random() (excludes itself)
  ├─ screensaver.rs  Screensaver trait definition + EventResponse enum
  ├─ theme.rs    5 color palettes (default, neon, pastel, monochrome, dracula)
  ├─ fade.rs     Fade-to/from-black transitions using random cell fill
  ├─ overlay.rs  Clock overlay, lock message, password prompt
  ├─ power.rs    Battery detection via /sys/class/power_supply
  ├─ tui_config.rs  Interactive settings TUI (termsaver config)
  └─ screensavers/
       ├─ analog_clock.rs  Circular clock face with trig, timezone support
       ├─ bouncing_ball.rs Physics: f64 pos/vel, VecDeque trail
       ├─ starfield.rs     3D→2D projection, depth-based brightness
       ├─ matrix_rain.rs   Column-based rain, katakana/ascii/binary
       ├─ pipes.rs         Box-drawing chars, random walk
       ├─ digital_clock.rs 5×5 block font, blinking colon
       └─ game_of_life.rs  Conway rules, stagnation detection, interactive
```

## Adding a New Screensaver

1. Create `src/screensavers/<name>.rs` — implement `Screensaver` trait
2. Add config struct to `src/config.rs` with `Default` impl and `#[serde(default)]`
3. Add field to `ScreensaverConfigs` struct in `src/config.rs`
4. Register in `src/registry.rs`:
   - Add entry to `SCREENSAVERS` array
   - Add arm to `create()` match
   - Add arm to `create_random()` match
5. Add `pub mod <name>;` to `src/screensavers/mod.rs`
6. Add unit tests in `#[cfg(test)]` block

All steps are required or the screensaver won't be accessible.

## Screensaver Trait Contract

```rust
fn init(&mut self, width: u16, height: u16)  // Called on start AND on resize
fn update(&mut self, dt: Duration)           // Framerate-independent: use dt
fn render(&self, frame: &mut Frame)          // Pure read of state, no mutation
fn handle_event(&mut self, event) -> EventResponse  // Ignored = app exits on key
```

- `update` receives real delta time — always multiply velocities by `dt.as_secs_f64()`
- `render` takes `&self` not `&mut self` — state changes go in `update`
- `handle_event` default returns `Ignored` (any key exits). Return `Continue` to consume keys (e.g., Game of Life uses space/r/n)

## Features (Cargo)

- `pam` (default): Enables password lock via Linux PAM. Requires `libpam.so` at link time. Build without: `cargo build --no-default-features`

## Testing

- **Unit tests**: `#[cfg(test)]` in each module — physics, math, config parsing
- **Snapshots**: `tests/snapshot_tests.rs` using `insta` + `ratatui::backend::TestBackend`
- **Integration**: `tests/config_tests.rs` — TOML round-trip, error handling
- **Accept snapshots**: rename `.snap.new` → `.snap` or install `cargo-insta`
- **No real TTY in CI** — screensaver rendering tests use TestBackend, not a real terminal

## Config Conventions

- TOML at `~/.config/termsaver/config.toml`
- Every config struct: `#[derive(Debug, Clone, Serialize, Deserialize)]` + `#[serde(default)]`
- Precedence: CLI flags > TOML file > built-in defaults
- Per-screensaver config lives in `[screensavers.<name>]` TOML section

## Terminal Safety

- Always enter alternate screen before rendering
- Always restore terminal on exit (raw mode off, alternate screen off, cursor shown)
- The `app::run()` function handles setup/teardown — screensavers should NOT touch terminal state directly
- Fade transitions run inside the alternate screen
