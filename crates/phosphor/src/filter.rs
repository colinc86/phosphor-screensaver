use ratatui::buffer::Buffer;
use ratatui::style::Color;

use crate::config::{ColorFilterMode, FilterConfig};

pub struct FilterState {
    width: u16,
    height: u16,
    vignette_map: Vec<f32>,
}

impl FilterState {
    pub fn new(w: u16, h: u16) -> Self {
        let vignette_map = compute_vignette(w, h);
        Self {
            width: w,
            height: h,
            vignette_map,
        }
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        if w == self.width && h == self.height {
            return;
        }
        self.width = w;
        self.height = h;
        self.vignette_map = compute_vignette(w, h);
    }

    pub fn apply(&mut self, buf: &mut Buffer, config: &FilterConfig) {
        let area = buf.area;
        let w = area.width;
        let h = area.height;
        self.resize(w, h);

        // Early return if nothing is active
        let crt = config.crt_enabled;
        let color = config.color_mode != ColorFilterMode::Normal;
        let noise = config.noise_enabled;
        let bloom = config.bloom_enabled;
        if !crt && !color && !noise && !bloom {
            return;
        }

        let mut rng = FastRng::new();

        for y in 0..h {
            for x in 0..w {
                let Some(cell) = buf.cell_mut((x + area.x, y + area.y)) else {
                    continue;
                };

                let (mut fg_r, mut fg_g, mut fg_b) = extract_rgb(cell.fg);
                let (mut bg_r, mut bg_g, mut bg_b) = extract_rgb(cell.bg);

                // CRT effects
                if crt {
                    // Scanlines: dim every other row
                    if config.crt_scanlines && y % 2 == 1 {
                        let factor = 1.0 - config.crt_scanline_dim as f32;
                        fg_r = (fg_r as f32 * factor) as u8;
                        fg_g = (fg_g as f32 * factor) as u8;
                        fg_b = (fg_b as f32 * factor) as u8;
                        bg_r = (bg_r as f32 * factor) as u8;
                        bg_g = (bg_g as f32 * factor) as u8;
                        bg_b = (bg_b as f32 * factor) as u8;
                    }

                    // Vignette: darken edges
                    if config.crt_vignette {
                        let idx = (y as usize) * (w as usize) + (x as usize);
                        if let Some(&v) = self.vignette_map.get(idx) {
                            let strength = config.crt_vignette_strength as f32;
                            let factor = 1.0 - (1.0 - v) * strength;
                            fg_r = (fg_r as f32 * factor) as u8;
                            fg_g = (fg_g as f32 * factor) as u8;
                            fg_b = (fg_b as f32 * factor) as u8;
                            bg_r = (bg_r as f32 * factor) as u8;
                            bg_g = (bg_g as f32 * factor) as u8;
                            bg_b = (bg_b as f32 * factor) as u8;
                        }
                    }

                    // Flicker: random per-cell brightness jitter
                    if config.crt_flicker && rng.next_f32() < config.crt_flicker_rate as f32 {
                        let jitter = 0.85 + rng.next_f32() * 0.3; // 0.85..1.15
                        fg_r = (fg_r as f32 * jitter).min(255.0) as u8;
                        fg_g = (fg_g as f32 * jitter).min(255.0) as u8;
                        fg_b = (fg_b as f32 * jitter).min(255.0) as u8;
                    }

                    // Curvature: darken outermost rows/cols
                    if config.crt_curvature {
                        let factor = edge_distance(x, y, w, h);
                        if factor < 1.0 {
                            fg_r = (fg_r as f32 * factor) as u8;
                            fg_g = (fg_g as f32 * factor) as u8;
                            fg_b = (fg_b as f32 * factor) as u8;
                            bg_r = (bg_r as f32 * factor) as u8;
                            bg_g = (bg_g as f32 * factor) as u8;
                            bg_b = (bg_b as f32 * factor) as u8;
                        }
                    }
                }

                // Color filter
                if color {
                    let (r, g, b) = apply_color_filter(fg_r, fg_g, fg_b, config.color_mode);
                    fg_r = r;
                    fg_g = g;
                    fg_b = b;
                    let (r, g, b) = apply_color_filter(bg_r, bg_g, bg_b, config.color_mode);
                    bg_r = r;
                    bg_g = g;
                    bg_b = b;
                }

                // Noise
                if noise && rng.next_f32() < config.noise_intensity as f32 {
                    let offset = (rng.next_f32() * 80.0 - 40.0) as i16; // ±40
                    fg_r = (fg_r as i16 + offset).clamp(0, 255) as u8;
                    fg_g = (fg_g as i16 + offset).clamp(0, 255) as u8;
                    fg_b = (fg_b as i16 + offset).clamp(0, 255) as u8;
                }

                // Bloom
                if bloom && luminance(fg_r, fg_g, fg_b) > config.bloom_threshold {
                    let boost = 1.0 + config.bloom_boost as f32;
                    fg_r = (fg_r as f32 * boost).min(255.0) as u8;
                    fg_g = (fg_g as f32 * boost).min(255.0) as u8;
                    fg_b = (fg_b as f32 * boost).min(255.0) as u8;
                }

                cell.set_fg(Color::Rgb(fg_r, fg_g, fg_b));
                cell.set_bg(Color::Rgb(bg_r, bg_g, bg_b));
            }
        }
    }
}

fn compute_vignette(w: u16, h: u16) -> Vec<f32> {
    if w == 0 || h == 0 {
        return vec![];
    }
    let mut map = vec![1.0f32; w as usize * h as usize];
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    // Terminal cells are roughly 2:1 aspect ratio, so scale y accordingly
    let aspect = 2.0f32;

    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - cx) / cx;
            let dy = (y as f32 + 0.5 - cy) / cy * aspect;
            let dist = (dx * dx + dy * dy).sqrt() / aspect.sqrt();
            // Smooth falloff starting at dist=0.6
            let factor = if dist <= 0.6 {
                1.0
            } else {
                (1.0 - (dist - 0.6) / 0.4).max(0.0)
            };
            map[y as usize * w as usize + x as usize] = factor;
        }
    }
    map
}

fn extract_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Reset | Color::Black => (0, 0, 0),
        Color::White => (255, 255, 255),
        Color::Red => (205, 0, 0),
        Color::Green => (0, 205, 0),
        Color::Yellow => (205, 205, 0),
        Color::Blue => (0, 0, 238),
        Color::Magenta => (205, 0, 205),
        Color::Cyan => (0, 205, 205),
        Color::Gray => (229, 229, 229),
        Color::DarkGray => (127, 127, 127),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (92, 92, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::Indexed(idx) => indexed_to_rgb(idx),
    }
}

fn indexed_to_rgb(idx: u8) -> (u8, u8, u8) {
    match idx {
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            let n = idx - 16;
            let b = n % 6;
            let g = (n / 6) % 6;
            let r = n / 36;
            let to_val = |c: u8| if c == 0 { 0 } else { 55 + 40 * c };
            (to_val(r), to_val(g), to_val(b))
        }
        232..=255 => {
            let v = 8 + 10 * (idx - 232);
            (v, v, v)
        }
    }
}

fn edge_distance(x: u16, y: u16, w: u16, h: u16) -> f32 {
    let dx = x.min(w.saturating_sub(1) - x);
    let dy = y.min(h.saturating_sub(1) - y);
    match dx.min(dy) {
        0 => 0.3,
        1 => 0.7,
        _ => 1.0,
    }
}

fn luminance(r: u8, g: u8, b: u8) -> u8 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32).min(255.0) as u8
}

fn apply_color_filter(r: u8, g: u8, b: u8, mode: ColorFilterMode) -> (u8, u8, u8) {
    match mode {
        ColorFilterMode::Normal => (r, g, b),
        ColorFilterMode::MonochromeGreen => {
            let lum = luminance(r, g, b);
            (0, lum, (lum as f32 * 0.15) as u8)
        }
        ColorFilterMode::MonochromeAmber => {
            let lum = luminance(r, g, b);
            (lum, (lum as f32 * 0.69) as u8, 0)
        }
        ColorFilterMode::Sepia => {
            let rf = r as f32;
            let gf = g as f32;
            let bf = b as f32;
            let sr = (0.393 * rf + 0.769 * gf + 0.189 * bf).min(255.0) as u8;
            let sg = (0.349 * rf + 0.686 * gf + 0.168 * bf).min(255.0) as u8;
            let sb = (0.272 * rf + 0.534 * gf + 0.131 * bf).min(255.0) as u8;
            (sr, sg, sb)
        }
        ColorFilterMode::Inverted => (255 - r, 255 - g, 255 - b),
    }
}

/// Minimal fast RNG (xorshift32) — good enough for visual effects, no external deps.
struct FastRng {
    state: u32,
}

impl FastRng {
    fn new() -> Self {
        // Seed from a cheap source — the pointer to a stack variable
        let mut seed = 0x12345678u32;
        seed ^= (&seed as *const u32 as u32).wrapping_add(0xdeadbeef);
        if seed == 0 {
            seed = 1;
        }
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    fn test_buffer(w: u16, h: u16, fg: Color, bg: Color) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let style = Style::default().fg(fg).bg(bg);
        for y in 0..h {
            for x in 0..w {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                    cell.set_style(style);
                }
            }
        }
        buf
    }

    #[test]
    fn no_filters_is_noop() {
        let config = FilterConfig::default(); // all filters off
        let mut state = FilterState::new(10, 5);
        let mut buf = test_buffer(10, 5, Color::Rgb(100, 150, 200), Color::Rgb(10, 20, 30));
        state.apply(&mut buf, &config);
        // All cells should still have original colors
        for y in 0..5u16 {
            for x in 0..10u16 {
                let c = buf.cell((x, y)).unwrap();
                assert_eq!(c.fg, Color::Rgb(100, 150, 200));
                assert_eq!(c.bg, Color::Rgb(10, 20, 30));
            }
        }
    }

    #[test]
    fn scanlines_dim_odd_rows() {
        let mut config = FilterConfig::default();
        config.crt_enabled = true;
        config.crt_scanlines = true;
        config.crt_scanline_dim = 0.5;
        config.crt_vignette = false;
        config.crt_flicker = false;
        config.crt_curvature = false;

        let mut state = FilterState::new(4, 4);
        let mut buf = test_buffer(4, 4, Color::Rgb(200, 200, 200), Color::Rgb(100, 100, 100));
        state.apply(&mut buf, &config);

        // Even row (y=0) should be unchanged
        let cell_even = buf.cell((2, 0)).unwrap();
        if let Color::Rgb(r, _, _) = cell_even.fg {
            assert_eq!(r, 200);
        }

        // Odd row (y=1) should be dimmed
        let cell_odd = buf.cell((2, 1)).unwrap();
        if let Color::Rgb(r, _, _) = cell_odd.fg {
            assert!(r < 200, "odd row fg should be dimmed, got {r}");
        }
    }

    #[test]
    fn monochrome_green_removes_rb() {
        let mut config = FilterConfig::default();
        config.color_mode = ColorFilterMode::MonochromeGreen;

        let mut state = FilterState::new(2, 2);
        let mut buf = test_buffer(2, 2, Color::Rgb(200, 100, 50), Color::Rgb(0, 0, 0));
        state.apply(&mut buf, &config);

        let cell = buf.cell((0, 0)).unwrap();
        if let Color::Rgb(r, g, b) = cell.fg {
            assert_eq!(r, 0, "red channel should be 0 for monochrome green");
            assert!(g > 0, "green channel should be > 0");
            assert!(b < g, "blue should be much less than green");
        }
    }

    #[test]
    fn inverted_is_complement() {
        let (r, g, b) = apply_color_filter(100, 150, 200, ColorFilterMode::Inverted);
        assert_eq!((r, g, b), (155, 105, 55));
    }

    #[test]
    fn luminance_bounds() {
        assert_eq!(luminance(0, 0, 0), 0);
        assert_eq!(luminance(255, 255, 255), 255);
        // Mid-range
        let lum = luminance(128, 128, 128);
        assert!(lum > 100 && lum < 150);
    }

    #[test]
    fn edge_distance_corners_and_center() {
        let f_corner = edge_distance(0, 0, 20, 10);
        assert!((f_corner - 0.3).abs() < 0.01);

        let f_center = edge_distance(10, 5, 20, 10);
        assert!((f_center - 1.0).abs() < 0.01);
    }

    #[test]
    fn vignette_center_brighter_than_corner() {
        let map = compute_vignette(20, 10);
        let center = map[5 * 20 + 10];
        let corner = map[0];
        assert!(
            center > corner,
            "center ({center}) should be > corner ({corner})"
        );
    }

    #[test]
    fn resize_recomputes() {
        let mut state = FilterState::new(10, 5);
        assert_eq!(state.vignette_map.len(), 50);
        state.resize(20, 10);
        assert_eq!(state.width, 20);
        assert_eq!(state.height, 10);
        assert_eq!(state.vignette_map.len(), 200);
    }

    #[test]
    fn resize_noop_same_size() {
        let mut state = FilterState::new(10, 5);
        let ptr_before = state.vignette_map.as_ptr();
        state.resize(10, 5);
        let ptr_after = state.vignette_map.as_ptr();
        assert_eq!(ptr_before, ptr_after);
    }

    #[test]
    fn extract_rgb_named_colors() {
        assert_eq!(extract_rgb(Color::Black), (0, 0, 0));
        assert_eq!(extract_rgb(Color::White), (255, 255, 255));
        assert_eq!(extract_rgb(Color::Rgb(42, 84, 126)), (42, 84, 126));
    }

    #[test]
    fn fast_rng_range() {
        let mut rng = FastRng::new();
        for _ in 0..100 {
            let f = rng.next_f32();
            assert!((0.0..1.0).contains(&f), "rng out of range: {f}");
        }
    }
}
