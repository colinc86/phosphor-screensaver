use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::Frame;

use crate::config::OverlayConfig;

/// Render the clock overlay on top of the screensaver.
pub fn render_clock(frame: &mut Frame, config: &OverlayConfig) {
    if !config.clock.enabled {
        return;
    }

    let area = frame.area();
    let now = chrono::Local::now();
    let time_str = if config.clock.show_seconds {
        now.format("%H:%M:%S").to_string()
    } else {
        now.format(&config.clock.format).to_string()
    };

    let (x, y) = clock_position(&config.clock.position, area, time_str.len() as u16);

    let buf = frame.buffer_mut();
    for (i, ch) in time_str.chars().enumerate() {
        let px = x + i as u16;
        if px < area.width && y < area.height {
            buf.cell_mut((px, y)).map(|cell| {
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                );
            });
        }
    }
}

/// Render the lock overlay (centered message).
pub fn render_lock(frame: &mut Frame, message: &str) {
    let area = frame.area();
    let x = area.width.saturating_sub(message.len() as u16) / 2;
    let y = area.height.saturating_sub(1) - 2;

    let buf = frame.buffer_mut();
    for (i, ch) in message.chars().enumerate() {
        let px = x + i as u16;
        if px < area.width && y < area.height {
            buf.cell_mut((px, y)).map(|cell| {
                cell.set_char(ch);
                cell.set_style(Style::default().fg(Color::DarkGray));
            });
        }
    }
}

fn clock_position(position: &str, area: Rect, width: u16) -> (u16, u16) {
    match position {
        "top-left" => (1, 0),
        "bottom-left" => (1, area.height.saturating_sub(1)),
        "bottom-right" => (area.width.saturating_sub(width + 1), area.height.saturating_sub(1)),
        // top-right (default)
        _ => (area.width.saturating_sub(width + 1), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_position_top_right() {
        let area = Rect::new(0, 0, 80, 24);
        let (x, y) = clock_position("top-right", area, 5);
        assert_eq!(x, 74);
        assert_eq!(y, 0);
    }

    #[test]
    fn clock_position_top_left() {
        let area = Rect::new(0, 0, 80, 24);
        let (x, y) = clock_position("top-left", area, 5);
        assert_eq!(x, 1);
        assert_eq!(y, 0);
    }

    #[test]
    fn clock_position_bottom_left() {
        let area = Rect::new(0, 0, 80, 24);
        let (x, y) = clock_position("bottom-left", area, 5);
        assert_eq!(x, 1);
        assert_eq!(y, 23);
    }

    #[test]
    fn clock_position_bottom_right() {
        let area = Rect::new(0, 0, 80, 24);
        let (x, y) = clock_position("bottom-right", area, 5);
        assert_eq!(x, 74);
        assert_eq!(y, 23);
    }
}
