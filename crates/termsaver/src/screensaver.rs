/// Response from a screensaver handling an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResponse {
    /// Continue running the screensaver.
    Continue,
    /// Exit the screensaver.
    Exit,
    /// The screensaver didn't handle this event — let the app decide.
    Ignored,
}
