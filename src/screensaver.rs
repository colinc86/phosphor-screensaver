use crossterm::event::Event;
use ratatui::Frame;
use std::time::Duration;

/// Response from a screensaver handling an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EventResponse {
    /// Continue running the screensaver.
    Continue,
    /// Exit the screensaver.
    Exit,
    /// The screensaver didn't handle this event — let the app decide.
    Ignored,
}

/// Every screensaver implements this trait.
#[allow(dead_code)]
pub trait Screensaver {
    /// Human-readable name for display in menus.
    fn name(&self) -> &str;

    /// One-line description.
    fn description(&self) -> &str;

    /// Initialize or reinitialize state for the given terminal dimensions.
    /// Called once on startup and again on resize.
    fn init(&mut self, width: u16, height: u16);

    /// Advance simulation by `dt`.
    fn update(&mut self, dt: Duration);

    /// Render the current frame.
    fn render(&self, frame: &mut Frame);

    /// Handle a terminal event. Default: ignore everything (app exits on any key).
    fn handle_event(&mut self, _event: &Event) -> EventResponse {
        EventResponse::Ignored
    }
}
