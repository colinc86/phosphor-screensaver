pub mod buffer;
pub mod config_meta;
pub mod protocol;
pub mod runner;
pub mod theme;

pub use buffer::{Cell, PluginBuffer, PluginColor};
pub use config_meta::{ConfigField, FieldKind, PluginInfo};
pub use protocol::{EventResponse, HostMessage, KeyCode, KeyInput, MouseInput, MouseKind, PluginMessage};
pub use runner::run;
pub use theme::ThemeColors;

use std::time::Duration;

/// Every screensaver plugin implements this trait.
pub trait ScreensaverPlugin {
    /// Metadata for discovery (--info).
    fn info(&self) -> PluginInfo;

    /// Initialize or reinitialize state for the given terminal dimensions.
    fn init(&mut self, width: u16, height: u16);

    /// Advance simulation by `dt`.
    fn update(&mut self, dt: Duration);

    /// Render the current frame into the buffer.
    fn render(&self, buf: &mut PluginBuffer);

    /// Handle a key event. Default: ignored (app exits on any key).
    fn handle_key(&mut self, _key: KeyInput) -> EventResponse {
        EventResponse::Ignored
    }

    /// Handle a mouse event. Default: ignored.
    fn handle_mouse(&mut self, _mouse: MouseInput) -> EventResponse {
        EventResponse::Ignored
    }
}
