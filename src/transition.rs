//! Animated transitions between screensavers during auto-rotate.

use rand::Rng;
use ratatui::buffer::Buffer;
use ratatui::style::Style;

/// Transition style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStyle {
    Dissolve,
    Slide,
    Melt,
    None,
}

impl TransitionStyle {
    pub fn from_name(name: &str) -> Self {
        match name {
            "dissolve" => Self::Dissolve,
            "slide" => Self::Slide,
            "melt" => Self::Melt,
            "none" => Self::None,
            _ => Self::Dissolve,
        }
    }
}

/// State for an in-progress transition.
pub struct Transition {
    style: TransitionStyle,
    /// Snapshot of the old screensaver's buffer content: (char, Style) per cell
    old_cells: Vec<Vec<(char, Style)>>,
    /// Random ordering for dissolve, per-column random offsets for melt
    random_order: Vec<usize>,
    /// Per-column melt offsets (randomized start delay)
    melt_offsets: Vec<f64>,
    /// Progress from 0.0 to 1.0
    progress: f64,
    /// Duration in seconds
    duration: f64,
    width: u16,
    height: u16,
}

impl Transition {
    /// Create a new transition, capturing the old buffer state.
    pub fn new(style: TransitionStyle, duration_ms: u64, old_buffer: &Buffer) -> Self {
        let area = old_buffer.area;
        let width = area.width;
        let height = area.height;
        let mut rng = rand::rng();

        // Capture old buffer
        let mut old_cells = vec![vec![(' ', Style::default()); width as usize]; height as usize];
        for y in 0..height {
            for x in 0..width {
                if let Some(cell) = old_buffer.cell((x, y)) {
                    old_cells[y as usize][x as usize] = (cell.symbol().chars().next().unwrap_or(' '), cell.style());
                }
            }
        }

        // Random order for dissolve
        let total = (width as usize) * (height as usize);
        let mut random_order: Vec<usize> = (0..total).collect();
        for i in (1..random_order.len()).rev() {
            let j = rng.random_range(0..=i);
            random_order.swap(i, j);
        }

        // Melt offsets: random start delay per column
        let melt_offsets: Vec<f64> = (0..width)
            .map(|_| rng.random_range(0.0..0.4))
            .collect();

        Self {
            style,
            old_cells,
            random_order,
            melt_offsets,
            progress: 0.0,
            duration: duration_ms as f64 / 1000.0,
            width,
            height,
        }
    }

    /// Advance the transition. Returns true if still in progress.
    pub fn update(&mut self, dt_secs: f64) -> bool {
        if self.style == TransitionStyle::None {
            return false;
        }
        self.progress += dt_secs / self.duration;
        self.progress < 1.0
    }

    /// Apply the transition effect to the current frame buffer.
    /// The buffer should already contain the new screensaver's render.
    /// This overlays old content according to the transition progress.
    pub fn apply(&self, buf: &mut Buffer) {
        match self.style {
            TransitionStyle::Dissolve => self.apply_dissolve(buf),
            TransitionStyle::Slide => self.apply_slide(buf),
            TransitionStyle::Melt => self.apply_melt(buf),
            TransitionStyle::None => {}
        }
    }

    fn apply_dissolve(&self, buf: &mut Buffer) {
        let total = (self.width as usize) * (self.height as usize);
        // Number of cells that have "flipped" to new content
        let flipped = (self.progress * total as f64) as usize;

        // Start with all cells showing old content, then reveal new content
        // Cells NOT yet in the flipped set show old content
        let mut revealed = vec![false; total];
        for &idx in &self.random_order[..flipped.min(total)] {
            revealed[idx] = true;
        }

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y as usize * self.width as usize + x as usize;
                if !revealed[idx] {
                    // Show old content
                    let (ch, style) = self.old_cells[y as usize][x as usize];
                    buf.cell_mut((x, y)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(style);
                    });
                }
            }
        }
    }

    fn apply_slide(&self, buf: &mut Buffer) {
        // Old slides out left, new slides in from right
        let offset = (self.progress * self.width as f64) as u16;

        // Copy current (new) buffer content
        let mut new_cells = vec![vec![(' ', Style::default()); self.width as usize]; self.height as usize];
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(cell) = buf.cell((x, y)) {
                    new_cells[y as usize][x as usize] = (cell.symbol().chars().next().unwrap_or(' '), cell.style());
                }
            }
        }

        for y in 0..self.height {
            for x in 0..self.width {
                if x < self.width.saturating_sub(offset) {
                    // Show old content shifted left
                    let old_x = x as usize + offset as usize;
                    if old_x < self.width as usize {
                        let (ch, style) = self.old_cells[y as usize][old_x];
                        buf.cell_mut((x, y)).map(|cell| {
                            cell.set_char(ch);
                            cell.set_style(style);
                        });
                    }
                }
                // else: new content (already in buffer from render)
            }
        }
    }

    fn apply_melt(&self, buf: &mut Buffer) {
        // Each column "drips" downward at a random start time
        for x in 0..self.width as usize {
            let col_progress = (self.progress - self.melt_offsets[x]).max(0.0) / (1.0 - self.melt_offsets[x]).max(0.001);
            let col_progress = col_progress.clamp(0.0, 1.0);
            let drop = (col_progress * self.height as f64) as usize;

            for y in 0..self.height as usize {
                if y < drop {
                    // Above the melt line: show new content (already in buffer)
                    continue;
                }
                // Below the melt line: show old content shifted down
                let old_y = y - drop;
                if old_y < self.height as usize {
                    let (ch, style) = self.old_cells[old_y][x];
                    buf.cell_mut((x as u16, y as u16)).map(|cell| {
                        cell.set_char(ch);
                        cell.set_style(style);
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn test_buffer(w: u16, h: u16, ch: char) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        for y in 0..h {
            for x in 0..w {
                buf.cell_mut((x, y)).map(|cell| {
                    cell.set_char(ch);
                });
            }
        }
        buf
    }

    #[test]
    fn transition_style_from_name() {
        assert_eq!(TransitionStyle::from_name("dissolve"), TransitionStyle::Dissolve);
        assert_eq!(TransitionStyle::from_name("slide"), TransitionStyle::Slide);
        assert_eq!(TransitionStyle::from_name("melt"), TransitionStyle::Melt);
        assert_eq!(TransitionStyle::from_name("none"), TransitionStyle::None);
        assert_eq!(TransitionStyle::from_name("unknown"), TransitionStyle::Dissolve);
    }

    #[test]
    fn none_transition_completes_immediately() {
        let old = test_buffer(10, 5, 'A');
        let mut t = Transition::new(TransitionStyle::None, 800, &old);
        assert!(!t.update(0.01));
    }

    #[test]
    fn dissolve_progresses() {
        let old = test_buffer(10, 5, 'A');
        let mut t = Transition::new(TransitionStyle::Dissolve, 1000, &old);
        assert!(t.update(0.3)); // 30% done
        assert!(t.update(0.3)); // 60% done
        assert!(t.update(0.3)); // 90% done
        assert!(!t.update(0.2)); // past 100%
    }

    #[test]
    fn dissolve_starts_with_old_content() {
        let old = test_buffer(10, 5, 'A');
        let t = Transition::new(TransitionStyle::Dissolve, 1000, &old);
        // At progress 0, all cells should show old content
        let mut new_buf = test_buffer(10, 5, 'B');
        t.apply(&mut new_buf);
        // All cells should be 'A' since nothing has flipped yet
        for y in 0..5u16 {
            for x in 0..10u16 {
                let sym = new_buf.cell((x, y)).unwrap().symbol().chars().next().unwrap();
                assert_eq!(sym, 'A');
            }
        }
    }

    #[test]
    fn slide_shows_mixed_content() {
        let old = test_buffer(10, 5, 'A');
        let mut t = Transition::new(TransitionStyle::Slide, 1000, &old);
        t.progress = 0.5; // half way
        let mut new_buf = test_buffer(10, 5, 'B');
        t.apply(&mut new_buf);
        // Left half should have old, right half should have new
        let left = new_buf.cell((0, 0)).unwrap().symbol().chars().next().unwrap();
        let right = new_buf.cell((9, 0)).unwrap().symbol().chars().next().unwrap();
        assert_eq!(left, 'A');
        assert_eq!(right, 'B');
    }

    #[test]
    fn melt_offsets_vary() {
        let old = test_buffer(20, 10, 'X');
        let t = Transition::new(TransitionStyle::Melt, 800, &old);
        // Melt offsets should not all be the same
        let first = t.melt_offsets[0];
        let has_different = t.melt_offsets.iter().any(|&o| (o - first).abs() > 0.001);
        assert!(has_different);
    }

    #[test]
    fn transition_captures_old_buffer() {
        let old = test_buffer(10, 5, 'Z');
        let t = Transition::new(TransitionStyle::Dissolve, 800, &old);
        assert_eq!(t.old_cells[0][0].0, 'Z');
        assert_eq!(t.old_cells[4][9].0, 'Z');
    }
}
