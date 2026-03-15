use std::time::{Duration, Instant};

use ratatui::style::{Color, Style};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use rand::Rng;

/// Characters used during fade transitions.
const FADE_CHARS: &[char] = &['░', '▒', '▓', '█', ' '];

/// Run a fade-to-black transition on the terminal.
pub fn fade_out(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    duration_ms: u64,
) -> std::io::Result<()> {
    if duration_ms == 0 {
        return Ok(());
    }

    let duration = Duration::from_millis(duration_ms);
    let start = Instant::now();
    let mut rng = rand::rng();

    // Pre-generate random cell order for smooth reveal
    let size = terminal.size()?;
    let total_cells = (size.width as usize) * (size.height as usize);
    let mut cell_order: Vec<(u16, u16)> = Vec::with_capacity(total_cells);
    for y in 0..size.height {
        for x in 0..size.width {
            cell_order.push((x, y));
        }
    }
    // Shuffle for random fill pattern
    for i in (1..cell_order.len()).rev() {
        let j = rng.random_range(0..=i);
        cell_order.swap(i, j);
    }

    loop {
        let elapsed = start.elapsed();
        if elapsed >= duration {
            break;
        }

        let progress = elapsed.as_secs_f64() / duration.as_secs_f64();
        let cells_to_fill = (progress * total_cells as f64) as usize;

        terminal.draw(|frame| {
            let buf = frame.buffer_mut();
            for &(x, y) in &cell_order[..cells_to_fill.min(cell_order.len())] {
                let fade_idx = ((1.0 - progress) * (FADE_CHARS.len() - 1) as f64) as usize;
                let ch = FADE_CHARS[fade_idx.min(FADE_CHARS.len() - 1)];
                buf.cell_mut((x, y)).map(|cell| {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(Color::DarkGray));
                });
            }
        })?;

        std::thread::sleep(Duration::from_millis(16)); // ~60 FPS for smooth fade
    }

    // Final: fully black
    terminal.draw(|frame| {
        let area = frame.area();
        for y in 0..area.height {
            for x in 0..area.width {
                frame.buffer_mut().cell_mut((x, y)).map(|cell| {
                    cell.set_char(' ');
                    cell.set_style(Style::default());
                });
            }
        }
    })?;

    Ok(())
}

/// Run a fade-from-black transition (reverse of fade_out).
pub fn fade_in(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    duration_ms: u64,
) -> std::io::Result<()> {
    if duration_ms == 0 {
        return Ok(());
    }

    let duration = Duration::from_millis(duration_ms);
    let start = Instant::now();

    loop {
        let elapsed = start.elapsed();
        if elapsed >= duration {
            break;
        }

        let progress = elapsed.as_secs_f64() / duration.as_secs_f64();

        terminal.draw(|frame| {
            let area = frame.area();
            let buf = frame.buffer_mut();
            let fade_idx = (progress * (FADE_CHARS.len() - 1) as f64) as usize;
            let ch = FADE_CHARS[fade_idx.min(FADE_CHARS.len() - 1)];
            for y in 0..area.height {
                for x in 0..area.width {
                    buf.cell_mut((x, y)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(Style::default().fg(Color::DarkGray));
                    });
                }
            }
        })?;

        std::thread::sleep(Duration::from_millis(16));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_chars_not_empty() {
        assert!(!FADE_CHARS.is_empty());
    }

    #[test]
    fn fade_chars_ends_with_space() {
        assert_eq!(*FADE_CHARS.last().unwrap(), ' ');
    }
}
