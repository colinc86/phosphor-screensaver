use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::config::OverlayConfig;

/// Dark background color for overlay pads.
const BG_COLOR: Color = Color::Rgb(15, 13, 10);

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

    let text_width = time_str.len() as u16;
    let (x, y) = clock_position(&config.clock.position, area, text_width);

    // Background pad: 1 cell horizontal padding on each side
    let bg_x = x.saturating_sub(1);
    let bg_width = text_width + 2;
    let buf = frame.buffer_mut();
    for bx in bg_x..bg_x + bg_width {
        if bx < area.width
            && y < area.height
            && let Some(cell) = buf.cell_mut((bx, y))
        {
            cell.set_char(' ');
            cell.set_style(Style::default().bg(BG_COLOR));
        }
    }

    // Draw clock text
    for (i, ch) in time_str.chars().enumerate() {
        let px = x + i as u16;
        if px < area.width
            && y < area.height
            && let Some(cell) = buf.cell_mut((px, y))
        {
            cell.set_char(ch);
            cell.set_style(Style::default().fg(Color::Rgb(160, 160, 170)).bg(BG_COLOR));
        }
    }
}

/// Render the on-screen display (screensaver name after +/- cycling).
/// Centered horizontally, 3/4 down the screen. Fades out over 2 seconds.
pub fn render_osd(frame: &mut Frame, name: &str, elapsed: std::time::Duration) {
    let area = frame.area();
    let fade = 1.0 - (elapsed.as_secs_f64() / 2.0).min(1.0);
    if fade <= 0.0 {
        return;
    }

    let brightness = (160.0 * fade) as u8;
    let bg_alpha = (15.0 * fade) as u8;

    let text = format!(" {} ", name);
    let text_width = text.len() as u16;
    let x = area.width.saturating_sub(text_width) / 2;
    let y = area.height * 3 / 4;

    let buf = frame.buffer_mut();
    for (i, ch) in text.chars().enumerate() {
        let px = x + i as u16;
        if px < area.width
            && y < area.height
            && let Some(cell) = buf.cell_mut((px, y))
        {
            cell.set_char(ch);
            cell.set_style(
                Style::default()
                    .fg(Color::Rgb(brightness, brightness, brightness))
                    .bg(Color::Rgb(
                        bg_alpha,
                        bg_alpha.saturating_sub(2),
                        bg_alpha.saturating_sub(5),
                    )),
            );
        }
    }
}

/// Render the info overlay (CWD, hostname) over the screensaver.
pub fn render_info(frame: &mut Frame, config: &OverlayConfig) {
    if !config.info.enabled {
        return;
    }

    let mut parts: Vec<String> = Vec::new();

    if config.info.show_cwd
        && let Ok(cwd) = std::env::current_dir()
    {
        let display = abbreviate_home(&cwd.display().to_string());
        parts.push(display);
    }

    if config.info.show_hostname
        && let Some(hostname) = get_hostname()
    {
        parts.push(hostname);
    }

    if parts.is_empty() {
        return;
    }

    let info_str = parts.join(" | ");
    // Truncate long strings from the left
    let area = frame.area();
    let max_len = (area.width as usize).saturating_sub(4);
    let info_str = if info_str.len() > max_len {
        format!("...{}", &info_str[info_str.len() - max_len + 3..])
    } else {
        info_str
    };

    let text_width = info_str.len() as u16;
    let (x, y) = info_position(&config.info.position, area, text_width);

    // Background pad: 1 cell horizontal padding on each side
    let bg_x = x.saturating_sub(1);
    let bg_width = text_width + 2;
    let buf = frame.buffer_mut();
    for bx in bg_x..bg_x + bg_width {
        if bx < area.width
            && y < area.height
            && let Some(cell) = buf.cell_mut((bx, y))
        {
            cell.set_char(' ');
            cell.set_style(Style::default().bg(BG_COLOR));
        }
    }

    // Draw info text (dimmer than clock)
    for (i, ch) in info_str.chars().enumerate() {
        let px = x + i as u16;
        if px < area.width
            && y < area.height
            && let Some(cell) = buf.cell_mut((px, y))
        {
            cell.set_char(ch);
            cell.set_style(Style::default().fg(Color::Rgb(100, 100, 110)).bg(BG_COLOR));
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
        if px < area.width
            && y < area.height
            && let Some(cell) = buf.cell_mut((px, y))
        {
            cell.set_char(ch);
            cell.set_style(Style::default().fg(Color::DarkGray));
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
            if x < area.width
                && y < area.height
                && let Some(cell) = buf.cell_mut((x, y))
            {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(Color::Rgb(30, 30, 40)));
            }
        }
    }

    // Draw border
    let border_style = Style::default()
        .fg(Color::DarkGray)
        .bg(Color::Rgb(30, 30, 40));
    for x in box_x..box_x + box_width {
        if box_y < area.height
            && let Some(c) = buf.cell_mut((x, box_y))
        {
            c.set_char('─');
            c.set_style(border_style);
        }
        let bottom = box_y + box_height - 1;
        if bottom < area.height
            && let Some(c) = buf.cell_mut((x, bottom))
        {
            c.set_char('─');
            c.set_style(border_style);
        }
    }
    for y in box_y..box_y + box_height {
        if box_x < area.width
            && let Some(c) = buf.cell_mut((box_x, y))
        {
            c.set_char('│');
            c.set_style(border_style);
        }
        let right = box_x + box_width - 1;
        if right < area.width
            && let Some(c) = buf.cell_mut((right, y))
        {
            c.set_char('│');
            c.set_style(border_style);
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
        if x < area.width
            && y < area.height
            && let Some(c) = buf.cell_mut((x, y))
        {
            c.set_char(ch);
            c.set_style(border_style);
        }
    }

    // "Password:" label
    let label = "Password: ";
    let label_y = box_y + 2;
    let label_x = box_x + 2;
    let input_style = Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 40));
    for (i, ch) in label.chars().enumerate() {
        let x = label_x + i as u16;
        if x < area.width
            && label_y < area.height
            && let Some(c) = buf.cell_mut((x, label_y))
        {
            c.set_char(ch);
            c.set_style(input_style);
        }
    }

    // Password mask (asterisks)
    let mask_x = label_x + label.len() as u16;
    let max_mask = (box_width - 4 - label.len() as u16) as usize;
    let mask_len = password_len.min(max_mask);
    for i in 0..mask_len {
        let x = mask_x + i as u16;
        if x < area.width
            && label_y < area.height
            && let Some(c) = buf.cell_mut((x, label_y))
        {
            c.set_char('*');
            c.set_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 40)));
        }
    }

    // Cursor
    let cursor_x = mask_x + mask_len as u16;
    if cursor_x < area.width
        && label_y < area.height
        && let Some(c) = buf.cell_mut((cursor_x, label_y))
    {
        c.set_char('_');
        c.set_style(Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 40)));
    }

    // Auth message (error/success)
    if let Some(msg) = message {
        let msg_y = box_y + 3;
        let msg_x = box_x + (box_width.saturating_sub(msg.len() as u16)) / 2;
        for (i, ch) in msg.chars().enumerate() {
            let x = msg_x + i as u16;
            if x < area.width
                && msg_y < area.height
                && let Some(c) = buf.cell_mut((x, msg_y))
            {
                c.set_char(ch);
                c.set_style(Style::default().fg(Color::Red).bg(Color::Rgb(30, 30, 40)));
            }
        }
    }
}

fn clock_position(position: &str, area: Rect, width: u16) -> (u16, u16) {
    match position {
        "top-left" => (1, 0),
        "bottom-left" => (1, area.height.saturating_sub(1)),
        "bottom-right" => (
            area.width.saturating_sub(width + 1),
            area.height.saturating_sub(1),
        ),
        // top-right (default)
        _ => (area.width.saturating_sub(width + 1), 0),
    }
}

fn info_position(position: &str, area: Rect, width: u16) -> (u16, u16) {
    match position {
        "top-left" => (1, 0),
        "top-right" => (area.width.saturating_sub(width + 1), 0),
        "bottom-right" => (
            area.width.saturating_sub(width + 1),
            area.height.saturating_sub(1),
        ),
        // bottom-left (default)
        _ => (1, area.height.saturating_sub(1)),
    }
}

/// Abbreviate home directory to ~/...
fn abbreviate_home(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if path.starts_with(&home_str) {
            return format!("~{}", &path[home_str.len()..]);
        }
    }
    path.to_string()
}

/// Get hostname via libc gethostname.
fn get_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes into buf up to its length and null-terminates.
    let ret = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if ret != 0 {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..len].to_vec()).ok()
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

    #[test]
    fn info_position_default_bottom_left() {
        let area = Rect::new(0, 0, 80, 24);
        let (x, y) = info_position("bottom-left", area, 10);
        assert_eq!(x, 1);
        assert_eq!(y, 23);
    }

    #[test]
    fn info_position_top_right() {
        let area = Rect::new(0, 0, 80, 24);
        let (x, y) = info_position("top-right", area, 10);
        assert_eq!(x, 69);
        assert_eq!(y, 0);
    }

    #[test]
    fn abbreviate_home_replaces_prefix() {
        if let Some(home) = dirs::home_dir() {
            let path = format!("{}/projects/foo", home.display());
            let result = abbreviate_home(&path);
            assert_eq!(result, "~/projects/foo");
        }
    }

    #[test]
    fn abbreviate_home_no_match() {
        let result = abbreviate_home("/tmp/something");
        assert_eq!(result, "/tmp/something");
    }

    #[test]
    fn get_hostname_returns_something() {
        // Should succeed on any unix system
        let hostname = get_hostname();
        assert!(hostname.is_some());
        assert!(!hostname.unwrap().is_empty());
    }
}
