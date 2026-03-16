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

/// Render the password prompt overlay.
pub fn render_password_prompt(frame: &mut Frame, password_len: usize, message: Option<&str>) {
    let area = frame.area();
    let buf = frame.buffer_mut();

    // Box dimensions
    let box_width = 40u16.min(area.width.saturating_sub(4));
    let box_height = 5u16;
    let box_x = (area.width.saturating_sub(box_width)) / 2;
    let box_y = (area.height.saturating_sub(box_height)) / 2;

    // Draw box background
    for y in box_y..box_y + box_height {
        for x in box_x..box_x + box_width {
            if x < area.width && y < area.height {
                buf.cell_mut((x, y)).map(|cell| {
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(Color::Rgb(30, 30, 40)));
                });
            }
        }
    }

    // Draw border
    let border_style = Style::default().fg(Color::DarkGray).bg(Color::Rgb(30, 30, 40));
    for x in box_x..box_x + box_width {
        if box_y < area.height {
            buf.cell_mut((x, box_y)).map(|c| { c.set_char('─'); c.set_style(border_style); });
        }
        let bottom = box_y + box_height - 1;
        if bottom < area.height {
            buf.cell_mut((x, bottom)).map(|c| { c.set_char('─'); c.set_style(border_style); });
        }
    }
    for y in box_y..box_y + box_height {
        if box_x < area.width {
            buf.cell_mut((box_x, y)).map(|c| { c.set_char('│'); c.set_style(border_style); });
        }
        let right = box_x + box_width - 1;
        if right < area.width {
            buf.cell_mut((right, y)).map(|c| { c.set_char('│'); c.set_style(border_style); });
        }
    }
    // Corners
    let corners = [
        (box_x, box_y, '┌'),
        (box_x + box_width - 1, box_y, '┐'),
        (box_x, box_y + box_height - 1, '└'),
        (box_x + box_width - 1, box_y + box_height - 1, '┘'),
    ];
    for (x, y, ch) in corners {
        if x < area.width && y < area.height {
            buf.cell_mut((x, y)).map(|c| { c.set_char(ch); c.set_style(border_style); });
        }
    }

    // "Password:" label
    let label = "Password: ";
    let label_y = box_y + 2;
    let label_x = box_x + 2;
    let input_style = Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 40));
    for (i, ch) in label.chars().enumerate() {
        let x = label_x + i as u16;
        if x < area.width && label_y < area.height {
            buf.cell_mut((x, label_y)).map(|c| { c.set_char(ch); c.set_style(input_style); });
        }
    }

    // Password mask (asterisks)
    let mask_x = label_x + label.len() as u16;
    let max_mask = (box_width - 4 - label.len() as u16) as usize;
    let mask_len = password_len.min(max_mask);
    for i in 0..mask_len {
        let x = mask_x + i as u16;
        if x < area.width && label_y < area.height {
            buf.cell_mut((x, label_y)).map(|c| {
                c.set_char('*');
                c.set_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 40)));
            });
        }
    }

    // Cursor
    let cursor_x = mask_x + mask_len as u16;
    if cursor_x < area.width && label_y < area.height {
        buf.cell_mut((cursor_x, label_y)).map(|c| {
            c.set_char('_');
            c.set_style(Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 40)));
        });
    }

    // Auth message (error/success)
    if let Some(msg) = message {
        let msg_y = box_y + 3;
        let msg_x = box_x + (box_width.saturating_sub(msg.len() as u16)) / 2;
        for (i, ch) in msg.chars().enumerate() {
            let x = msg_x + i as u16;
            if x < area.width && msg_y < area.height {
                buf.cell_mut((x, msg_y)).map(|c| {
                    c.set_char(ch);
                    c.set_style(Style::default().fg(Color::Red).bg(Color::Rgb(30, 30, 40)));
                });
            }
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
