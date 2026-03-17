use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::config::Config;
use crate::plugin_host::copy_buffer_to_region;
use crate::registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    Vertical,   // side by side (|)
    Horizontal, // top and bottom (/)
}

pub fn run(
    config: &Config,
    left_name: &str,
    right_name: &str,
    orientation: SplitOrientation,
    preview_duration: Option<Duration>,
) -> io::Result<()> {
    // Create both screensavers as PluginRenderers
    let mut left = registry::create_plugin(left_name, config)
        .unwrap_or_else(|| registry::create_random_plugin(config));
    let mut right = registry::create_plugin(right_name, config)
        .unwrap_or_else(|| registry::create_random_plugin(config));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Init screensavers with their panel sizes
    let term_size = terminal.size()?;
    let size = Rect::new(0, 0, term_size.width, term_size.height);
    let (left_area, right_area, _divider) =
        compute_areas(size, orientation, &config.compositor.split_ratio);
    left.init(left_area.width, left_area.height)?;
    right.init(right_area.width, right_area.height)?;

    let frame_duration = Duration::from_millis(1000 / config.general.fps.max(1) as u64);
    let mut last_frame = Instant::now();
    let start_time = Instant::now();

    loop {
        // Check preview timeout
        if let Some(dur) = preview_duration
            && start_time.elapsed() >= dur
        {
            break;
        }

        // Poll events
        if event::poll(frame_duration)? {
            let ev = event::read()?;
            if let Event::Resize(w, h) = ev {
                let new_size = Rect::new(0, 0, w, h);
                let (la, ra, _) =
                    compute_areas(new_size, orientation, &config.compositor.split_ratio);
                left.init(la.width, la.height)?;
                right.init(ra.width, ra.height)?;
                continue;
            }
            if let Event::Key(KeyEvent { code, .. }) = &ev {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {
                        // Forward to both screensavers
                        let _ = left.handle_event(&ev);
                        let _ = right.handle_event(&ev);
                    }
                }
            }
        }

        // Update
        let now = Instant::now();
        let dt = now - last_frame;
        last_frame = now;
        let _ = left.update(dt);
        let _ = right.update(dt);

        // Pre-render both to buffers
        let left_buf = left.render_to_buffer()?.clone();
        let right_buf = right.render_to_buffer()?.clone();

        // Render
        let term_size = terminal.size()?;
        let size = Rect::new(0, 0, term_size.width, term_size.height);
        let (la, ra, div) = compute_areas(size, orientation, &config.compositor.split_ratio);
        let divider_style_name = &config.compositor.divider_style;

        terminal.draw(|frame| {
            let buf = frame.buffer_mut();

            // Copy left screensaver buffer
            copy_buffer_to_region(&left_buf, buf, la);

            // Copy right screensaver buffer
            copy_buffer_to_region(&right_buf, buf, ra);

            // Draw divider
            if divider_style_name != "none" {
                let div_style = Style::default().fg(Color::DarkGray);
                match orientation {
                    SplitOrientation::Vertical => {
                        for y in div.y..div.y + div.height {
                            if let Some(cell) = buf.cell_mut((div.x, y)) {
                                cell.set_char('\u{2502}');
                                cell.set_style(div_style);
                            }
                        }
                    }
                    SplitOrientation::Horizontal => {
                        for x in div.x..div.x + div.width {
                            if let Some(cell) = buf.cell_mut((x, div.y)) {
                                cell.set_char('\u{2500}');
                                cell.set_style(div_style);
                            }
                        }
                    }
                }
            }
        })?;
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;

    Ok(())
}

fn compute_areas(size: Rect, orientation: SplitOrientation, ratio: &f64) -> (Rect, Rect, Rect) {
    let ratio = ratio.clamp(0.1, 0.9);
    match orientation {
        SplitOrientation::Vertical => {
            let left_w = ((size.width as f64 - 1.0) * ratio) as u16;
            let divider_x = left_w;
            let right_x = left_w + 1;
            let right_w = size.width.saturating_sub(right_x);
            (
                Rect::new(0, 0, left_w, size.height),
                Rect::new(right_x, 0, right_w, size.height),
                Rect::new(divider_x, 0, 1, size.height),
            )
        }
        SplitOrientation::Horizontal => {
            let top_h = ((size.height as f64 - 1.0) * ratio) as u16;
            let divider_y = top_h;
            let bottom_y = top_h + 1;
            let bottom_h = size.height.saturating_sub(bottom_y);
            (
                Rect::new(0, 0, size.width, top_h),
                Rect::new(0, bottom_y, size.width, bottom_h),
                Rect::new(0, divider_y, size.width, 1),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_areas_vertical() {
        let size = Rect::new(0, 0, 80, 24);
        let (left, right, div) = compute_areas(size, SplitOrientation::Vertical, &0.5);
        assert!(left.width > 0);
        assert!(right.width > 0);
        assert_eq!(div.width, 1);
        assert_eq!(left.width + 1 + right.width, 80);
    }

    #[test]
    fn compute_areas_horizontal() {
        let size = Rect::new(0, 0, 80, 24);
        let (top, bottom, div) = compute_areas(size, SplitOrientation::Horizontal, &0.5);
        assert!(top.height > 0);
        assert!(bottom.height > 0);
        assert_eq!(div.height, 1);
        assert_eq!(top.height + 1 + bottom.height, 24);
    }

    #[test]
    fn compute_areas_ratio_clamp() {
        let size = Rect::new(0, 0, 80, 24);
        let (left, right, _) = compute_areas(size, SplitOrientation::Vertical, &0.0);
        assert!(left.width >= 7); // clamped to 0.1
        assert!(right.width > 0);
    }
}
