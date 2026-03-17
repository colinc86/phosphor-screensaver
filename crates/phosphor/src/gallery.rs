//! Live screensaver gallery — shows all screensavers in a split-screen mosaic.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::Config;
use crate::plugin_host::{PluginRenderer, copy_buffer_to_region};
use crate::registry;

struct PanelLayout {
    cols: usize,
    #[allow(dead_code)]
    rows: usize,
    panel_w: u16,
    panel_h: u16,
}

fn compute_layout(term_w: u16, term_h: u16, count: usize) -> PanelLayout {
    // Find the grid that best fills the terminal
    let mut best_cols = 1;
    let mut best_rows = count;
    let mut best_waste = u32::MAX;

    for cols in 1..=count.min(term_w as usize / 10) {
        let rows = (count + cols - 1) / cols;
        let pw = term_w / cols as u16;
        let ph = (term_h.saturating_sub(1)) / rows as u16; // -1 for status bar
        if pw < 10 || ph < 5 {
            continue;
        }
        let waste = (pw as u32 * ph as u32).abs_diff(
            (term_w as u32 / count as u32) * (term_h as u32 / count as u32),
        );
        let empty_cells = (cols * rows - count) as u32 * pw as u32 * ph as u32;
        let total_waste = waste + empty_cells;
        if total_waste < best_waste {
            best_waste = total_waste;
            best_cols = cols;
            best_rows = rows;
        }
    }

    PanelLayout {
        cols: best_cols,
        rows: best_rows,
        panel_w: term_w / best_cols as u16,
        panel_h: (term_h.saturating_sub(1)) / best_rows as u16,
    }
}

/// Run the gallery view.
pub fn run(config: &Config) -> io::Result<()> {
    // Collect discovered plugins
    let plugin_list = registry::plugins();
    let count = plugin_list.len();

    // Create all screensavers as PluginRenderers
    let mut savers: Vec<Box<dyn PluginRenderer>> = plugin_list
        .iter()
        .map(|p| {
            registry::create_plugin(&p.name, config)
                .unwrap_or_else(|| registry::create_random_plugin(config))
        })
        .collect();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let size = terminal.size()?;
    let mut layout = compute_layout(size.width, size.height, count);

    // Init all screensavers at panel size (minus 2 for border)
    let inner_w = layout.panel_w.saturating_sub(2);
    let inner_h = layout.panel_h.saturating_sub(3); // border + label
    for saver in &mut savers {
        saver.init(inner_w, inner_h)?;
    }

    let mut selected: usize = 0;
    let mut last_frame = Instant::now();
    let frame_duration = Duration::from_millis(1000 / 15); // 15 FPS for gallery

    loop {
        // Poll events
        if event::poll(frame_duration)? {
            let ev = event::read()?;

            if let Event::Resize(w, h) = ev {
                layout = compute_layout(w, h, count);
                let inner_w = layout.panel_w.saturating_sub(2);
                let inner_h = layout.panel_h.saturating_sub(3);
                for saver in &mut savers {
                    saver.init(inner_w, inner_h)?;
                }
                continue;
            }

            if let Event::Key(KeyEvent { code, .. }) = &ev {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Right | KeyCode::Char('l') => {
                        selected = (selected + 1) % count;
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        selected = (selected + count - 1) % count;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + layout.cols).min(count - 1);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(layout.cols);
                    }
                    KeyCode::Enter => {
                        // Launch full-screen
                        drop(terminal);
                        disable_raw_mode()?;
                        let mut stdout2 = io::stdout();
                        execute!(stdout2, LeaveAlternateScreen, crossterm::cursor::Show)?;

                        let mut fullscreen_config = config.clone();
                        fullscreen_config.general.default_screensaver =
                            plugin_list[selected].name.clone();
                        fullscreen_config.general.fade_enabled = false;
                        let _ = crate::app::run(&fullscreen_config, None);

                        // Re-setup terminal after returning
                        enable_raw_mode()?;
                        let mut stdout3 = io::stdout();
                        execute!(stdout3, EnterAlternateScreen, crossterm::cursor::Hide)?;
                        let backend2 = CrosstermBackend::new(stdout3);
                        terminal = Terminal::new(backend2)?;
                        terminal.clear()?;

                        let size = terminal.size()?;
                        layout = compute_layout(size.width, size.height, count);
                        let inner_w = layout.panel_w.saturating_sub(2);
                        let inner_h = layout.panel_h.saturating_sub(3);
                        for saver in &mut savers {
                            saver.init(inner_w, inner_h)?;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
        }

        // Update
        let now = Instant::now();
        let dt = now - last_frame;
        last_frame = now;
        for saver in &mut savers {
            let _ = saver.update(dt);
        }

        // Pre-render all screensavers to their buffers
        let mut saver_buffers: Vec<Option<Buffer>> = Vec::with_capacity(savers.len());
        for saver in &mut savers {
            if let Ok(buf) = saver.render_to_buffer() {
                saver_buffers.push(Some(buf.clone()));
            } else {
                saver_buffers.push(None);
            }
        }

        // Render
        let sel = selected;
        let panel_w = layout.panel_w;
        let panel_h = layout.panel_h;
        let cols = layout.cols;
        let saver_names: Vec<&str> = savers.iter().map(|s| s.name()).collect();
        terminal.draw(|frame| {
            let area = frame.area();

            for (i, saver_buf) in saver_buffers.iter().enumerate() {
                let col = (i % cols) as u16;
                let row = (i / cols) as u16;
                let px = col * panel_w;
                let py = row * panel_h;

                if px + panel_w > area.width || py + panel_h > area.height {
                    continue;
                }

                let is_selected = i == sel;

                // Draw border
                let border_color = if is_selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                };
                let border_style = Style::default().fg(border_color);
                let buf = frame.buffer_mut();
                draw_border(buf, px, py, panel_w, panel_h, border_style);

                // Draw label
                let name = saver_names[i];
                let label_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let label_y = py + panel_h - 1;
                let label_x = px + (panel_w.saturating_sub(name.len() as u16)) / 2;
                for (j, ch) in name.chars().enumerate() {
                    let lx = label_x + j as u16;
                    if lx > px && lx < px + panel_w - 1 && label_y < area.height {
                        buf.cell_mut((lx, label_y)).map(|cell| {
                            cell.set_char(ch);
                            cell.set_style(label_style);
                        });
                    }
                }

                // Copy screensaver buffer into sub-region
                let inner_area = Rect::new(px + 1, py + 1, panel_w - 2, panel_h - 3);
                if let Some(src_buf) = saver_buf {
                    copy_buffer_to_region(src_buf, buf, inner_area);
                }
            }

            // Status bar at bottom
            let status_y = area.height - 1;
            let status = format!(
                " Gallery | {}/{} | arrows: navigate | enter: fullscreen | q: quit ",
                sel + 1,
                count
            );
            let buf = frame.buffer_mut();
            for (i, ch) in status.chars().enumerate() {
                let x = i as u16;
                if x < area.width {
                    buf.cell_mut((x, status_y)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::DarkGray),
                        );
                    });
                }
            }
            // Fill rest of status bar
            for x in status.len() as u16..area.width {
                buf.cell_mut((x, status_y)).map(|cell| {
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(Color::DarkGray));
                });
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

fn draw_border(buf: &mut Buffer, x: u16, y: u16, w: u16, h: u16, style: Style) {
    // Top and bottom
    for bx in x + 1..x + w - 1 {
        buf.cell_mut((bx, y)).map(|c| {
            c.set_char('\u{2500}');
            c.set_style(style);
        });
        buf.cell_mut((bx, y + h - 2)).map(|c| {
            c.set_char('\u{2500}');
            c.set_style(style);
        });
    }
    // Sides
    for by in y + 1..y + h - 2 {
        buf.cell_mut((x, by)).map(|c| {
            c.set_char('\u{2502}');
            c.set_style(style);
        });
        buf.cell_mut((x + w - 1, by)).map(|c| {
            c.set_char('\u{2502}');
            c.set_style(style);
        });
    }
    // Corners
    buf.cell_mut((x, y)).map(|c| {
        c.set_char('\u{250C}');
        c.set_style(style);
    });
    buf.cell_mut((x + w - 1, y)).map(|c| {
        c.set_char('\u{2510}');
        c.set_style(style);
    });
    buf.cell_mut((x, y + h - 2)).map(|c| {
        c.set_char('\u{2514}');
        c.set_style(style);
    });
    buf.cell_mut((x + w - 1, y + h - 2)).map(|c| {
        c.set_char('\u{2518}');
        c.set_style(style);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_layout_basic() {
        let layout = compute_layout(80, 24, 10);
        assert!(layout.cols > 0);
        assert!(layout.rows > 0);
        assert!(layout.panel_w >= 10);
        assert!(layout.panel_h >= 5);
        assert!(layout.cols * layout.rows >= 10);
    }

    #[test]
    fn compute_layout_single() {
        let layout = compute_layout(80, 24, 1);
        assert_eq!(layout.cols, 1);
        assert_eq!(layout.rows, 1);
    }

    #[test]
    fn compute_layout_large_terminal() {
        let layout = compute_layout(200, 60, 10);
        assert!(layout.cols >= 1);
        assert!(layout.panel_w >= 10);
        assert!(layout.panel_h >= 5);
    }

    #[test]
    fn compute_layout_small_terminal() {
        // Even with small terminal, should produce valid layout
        let layout = compute_layout(40, 12, 4);
        assert!(layout.panel_w >= 10);
        assert!(layout.panel_h >= 5);
    }
}
