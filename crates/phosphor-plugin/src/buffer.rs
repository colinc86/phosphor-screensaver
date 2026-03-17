/// Color representation for plugin rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginColor {
    Rgb(u8, u8, u8),
    Reset,
}

impl PluginColor {
    /// Encode as 4 bytes: [tag, r, g, b]. Tag: 0 = Reset, 1 = Rgb.
    pub fn encode(&self) -> [u8; 4] {
        match self {
            PluginColor::Rgb(r, g, b) => [1, *r, *g, *b],
            PluginColor::Reset => [0, 0, 0, 0],
        }
    }

    /// Decode from 4 bytes.
    pub fn decode(bytes: [u8; 4]) -> Self {
        match bytes[0] {
            1 => PluginColor::Rgb(bytes[1], bytes[2], bytes[3]),
            _ => PluginColor::Reset,
        }
    }
}

/// A single cell in the plugin buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: PluginColor,
    pub bg: PluginColor,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: PluginColor::Reset,
            bg: PluginColor::Reset,
        }
    }
}

/// A 2D buffer of cells that plugins render into.
pub struct PluginBuffer {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl PluginBuffer {
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![Cell::default(); size],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let size = width as usize * height as usize;
        self.cells.resize(size, Cell::default());
        self.cells.fill(Cell::default());
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    pub fn set(&mut self, x: u16, y: u16, cell: Cell) {
        if x < self.width && y < self.height {
            let idx = y as usize * self.width as usize + x as usize;
            self.cells[idx] = cell;
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            let idx = y as usize * self.width as usize + x as usize;
            Some(&self.cells[idx])
        } else {
            None
        }
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Encode all cells into binary frame data.
    /// Per cell: 4 bytes UTF-8 char + 4 bytes fg + 4 bytes bg = 12 bytes.
    pub fn encode_frame(&self) -> Vec<u8> {
        let cell_count = self.cells.len();
        let mut data = Vec::with_capacity(cell_count * 12);
        for cell in &self.cells {
            // Encode char as UTF-8, padded to 4 bytes
            let mut char_buf = [0u8; 4];
            let encoded = cell.ch.encode_utf8(&mut char_buf);
            let len = encoded.len();
            data.extend_from_slice(&char_buf[..4.min(len)]);
            // Pad if less than 4 bytes
            data.extend(std::iter::repeat_n(0u8, 4 - len));
            data.extend_from_slice(&cell.fg.encode());
            data.extend_from_slice(&cell.bg.encode());
        }
        data
    }

    /// Decode frame data into cells. Returns number of cells decoded.
    pub fn decode_frame(&mut self, data: &[u8]) -> usize {
        let cell_size = 12; // 4 char + 4 fg + 4 bg
        let count = data.len() / cell_size;
        let count = count.min(self.cells.len());

        for i in 0..count {
            let offset = i * cell_size;
            // Decode char
            let char_bytes = &data[offset..offset + 4];
            let ch = decode_char_from_bytes(char_bytes);
            // Decode fg
            let fg = PluginColor::decode([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            // Decode bg
            let bg = PluginColor::decode([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);
            self.cells[i] = Cell { ch, fg, bg };
        }
        count
    }
}

fn decode_char_from_bytes(bytes: &[u8]) -> char {
    // Find the actual UTF-8 length (strip trailing zeros)
    let len = if bytes[0] == 0 {
        0
    } else if bytes[0] & 0x80 == 0 {
        1
    } else if bytes[0] & 0xE0 == 0xC0 {
        2
    } else if bytes[0] & 0xF0 == 0xE0 {
        3
    } else {
        4
    };
    if len == 0 {
        return ' ';
    }
    std::str::from_utf8(&bytes[..len])
        .ok()
        .and_then(|s| s.chars().next())
        .unwrap_or(' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_basics() {
        let mut buf = PluginBuffer::new(10, 5);
        assert_eq!(buf.width(), 10);
        assert_eq!(buf.height(), 5);

        let cell = Cell {
            ch: '*',
            fg: PluginColor::Rgb(255, 0, 0),
            bg: PluginColor::Reset,
        };
        buf.set(3, 2, cell);
        assert_eq!(buf.get(3, 2), Some(&cell));
        assert_eq!(buf.get(0, 0).unwrap().ch, ' ');
    }

    #[test]
    fn buffer_out_of_bounds() {
        let mut buf = PluginBuffer::new(10, 5);
        buf.set(100, 100, Cell::default()); // should not panic
        assert_eq!(buf.get(100, 100), None);
    }

    #[test]
    fn buffer_clear() {
        let mut buf = PluginBuffer::new(5, 5);
        buf.set(
            2,
            2,
            Cell {
                ch: 'X',
                fg: PluginColor::Rgb(0, 255, 0),
                bg: PluginColor::Reset,
            },
        );
        buf.clear();
        assert_eq!(buf.get(2, 2).unwrap().ch, ' ');
    }

    #[test]
    fn buffer_resize() {
        let mut buf = PluginBuffer::new(5, 5);
        buf.set(
            2,
            2,
            Cell {
                ch: 'A',
                fg: PluginColor::Reset,
                bg: PluginColor::Reset,
            },
        );
        buf.resize(10, 10);
        assert_eq!(buf.width(), 10);
        assert_eq!(buf.height(), 10);
        // After resize, cells should be cleared
        assert_eq!(buf.get(2, 2).unwrap().ch, ' ');
    }

    #[test]
    fn encode_decode_round_trip() {
        let mut buf = PluginBuffer::new(3, 2);
        buf.set(
            0,
            0,
            Cell {
                ch: 'A',
                fg: PluginColor::Rgb(255, 128, 0),
                bg: PluginColor::Rgb(0, 0, 0),
            },
        );
        buf.set(
            1,
            0,
            Cell {
                ch: '\u{2588}',
                fg: PluginColor::Reset,
                bg: PluginColor::Rgb(0, 255, 0),
            },
        );
        buf.set(
            2,
            1,
            Cell {
                ch: '\u{2800}',
                fg: PluginColor::Rgb(100, 100, 100),
                bg: PluginColor::Reset,
            },
        );

        let encoded = buf.encode_frame();
        let mut buf2 = PluginBuffer::new(3, 2);
        buf2.decode_frame(&encoded);

        assert_eq!(buf2.get(0, 0).unwrap().ch, 'A');
        assert_eq!(buf2.get(0, 0).unwrap().fg, PluginColor::Rgb(255, 128, 0));
        assert_eq!(buf2.get(1, 0).unwrap().ch, '\u{2588}');
        assert_eq!(buf2.get(2, 1).unwrap().ch, '\u{2800}');
    }

    #[test]
    fn color_encode_decode() {
        let colors = [
            PluginColor::Reset,
            PluginColor::Rgb(0, 0, 0),
            PluginColor::Rgb(255, 255, 255),
            PluginColor::Rgb(128, 64, 32),
        ];
        for color in &colors {
            let encoded = color.encode();
            let decoded = PluginColor::decode(encoded);
            assert_eq!(*color, decoded);
        }
    }
}
