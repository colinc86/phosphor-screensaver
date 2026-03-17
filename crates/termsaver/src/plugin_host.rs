//! Plugin host: manages subprocess plugins and provides the PluginRenderer trait.

use std::io::{self, BufReader, BufWriter};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use termsaver_plugin::buffer::{PluginBuffer, PluginColor};
use termsaver_plugin::protocol::{
    self as proto, HostMessage, KeyCode as ProtoKeyCode, KeyInput, MouseInput,
    MouseKind as ProtoMouseKind, PluginMessage,
};

use crate::screensaver::EventResponse;

/// Unified interface for subprocess screensavers.
pub trait PluginRenderer {
    fn init(&mut self, w: u16, h: u16) -> io::Result<()>;
    fn update(&mut self, dt: Duration) -> io::Result<()>;

    /// Render into an owned buffer and return a reference.
    fn render_to_buffer(&mut self) -> io::Result<&Buffer>;

    /// Render directly into a ratatui Frame (copies from internal buffer).
    fn render_frame(&mut self, frame: &mut ratatui::Frame) -> io::Result<()> {
        let buf = self.render_to_buffer()?;
        copy_buffer(buf, frame.buffer_mut());
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> io::Result<EventResponse>;
    fn name(&self) -> &str;
}

/// Subprocess plugin: communicates with an external binary over stdin/stdout.
pub struct SubprocessPlugin {
    name: String,
    child: Child,
    stdin: BufWriter<std::process::ChildStdin>,
    stdout: BufReader<std::process::ChildStdout>,
    buffer: Buffer,
    plugin_buf: PluginBuffer,
    width: u16,
    height: u16,
}

impl SubprocessPlugin {
    /// Spawn a subprocess plugin from the given binary path.
    pub fn spawn(path: &str, name: &str, config_json: &str) -> io::Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = BufWriter::new(
            child
                .stdin
                .take()
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no stdin"))?,
        );
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no stdout"))?,
        );

        let mut plugin = Self {
            name: name.to_string(),
            child,
            stdin,
            stdout,
            buffer: Buffer::empty(Rect::new(0, 0, 80, 24)),
            plugin_buf: PluginBuffer::new(80, 24),
            width: 80,
            height: 24,
        };

        // Send config
        if !config_json.is_empty() && config_json != "{}" {
            HostMessage::Config(config_json.to_string()).write_to(&mut plugin.stdin)?;
        }

        Ok(plugin)
    }

    /// Convert the plugin buffer to ratatui Buffer.
    fn sync_buffer(&mut self) {
        self.buffer = Buffer::empty(Rect::new(0, 0, self.width, self.height));
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(cell) = self.plugin_buf.get(x, y) {
                    self.buffer.cell_mut((x, y)).map(|dst| {
                        dst.set_char(cell.ch);
                        dst.set_style(
                            Style::default()
                                .fg(plugin_color_to_ratatui(cell.fg))
                                .bg(plugin_color_to_ratatui(cell.bg)),
                        );
                    });
                }
            }
        }
    }
}

impl Drop for SubprocessPlugin {
    fn drop(&mut self) {
        let _ = HostMessage::Shutdown.write_to(&mut self.stdin);
        let _ = self.child.wait();
    }
}

impl PluginRenderer for SubprocessPlugin {
    fn init(&mut self, w: u16, h: u16) -> io::Result<()> {
        self.width = w;
        self.height = h;
        self.plugin_buf.resize(w, h);
        self.buffer = Buffer::empty(Rect::new(0, 0, w, h));
        HostMessage::Init {
            width: w,
            height: h,
        }
        .write_to(&mut self.stdin)
    }

    fn update(&mut self, dt: Duration) -> io::Result<()> {
        HostMessage::Update {
            dt_secs: dt.as_secs_f64(),
        }
        .write_to(&mut self.stdin)
    }

    fn render_to_buffer(&mut self) -> io::Result<&Buffer> {
        HostMessage::Render.write_to(&mut self.stdin)?;
        let msg = PluginMessage::read_from(&mut self.stdout)?;
        if let PluginMessage::Frame(data) = msg {
            self.plugin_buf.decode_frame(&data);
            self.sync_buffer();
        }
        Ok(&self.buffer)
    }

    fn handle_event(&mut self, event: &Event) -> io::Result<EventResponse> {
        match event {
            Event::Key(key) => {
                let proto_key = crossterm_key_to_proto(key);
                HostMessage::KeyEvent(proto_key).write_to(&mut self.stdin)?;
            }
            Event::Mouse(mouse) => {
                let proto_mouse = crossterm_mouse_to_proto(mouse);
                HostMessage::MouseEvent(proto_mouse).write_to(&mut self.stdin)?;
            }
            _ => return Ok(EventResponse::Ignored),
        }
        let msg = PluginMessage::read_from(&mut self.stdout)?;
        match msg {
            PluginMessage::Event(resp) => Ok(proto_event_to_host(resp)),
            _ => Ok(EventResponse::Ignored),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

fn plugin_color_to_ratatui(c: PluginColor) -> Color {
    match c {
        PluginColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
        PluginColor::Reset => Color::Reset,
    }
}

fn proto_event_to_host(resp: proto::EventResponse) -> EventResponse {
    match resp {
        proto::EventResponse::Continue => EventResponse::Continue,
        proto::EventResponse::Exit => EventResponse::Exit,
        proto::EventResponse::Ignored => EventResponse::Ignored,
    }
}

fn crossterm_key_to_proto(key: &KeyEvent) -> KeyInput {
    let code = match key.code {
        KeyCode::Char(' ') => ProtoKeyCode::Space,
        KeyCode::Char(c) => ProtoKeyCode::Char(c),
        KeyCode::Enter => ProtoKeyCode::Enter,
        KeyCode::Esc => ProtoKeyCode::Esc,
        KeyCode::Backspace => ProtoKeyCode::Backspace,
        KeyCode::Tab => ProtoKeyCode::Tab,
        KeyCode::Up => ProtoKeyCode::Up,
        KeyCode::Down => ProtoKeyCode::Down,
        KeyCode::Left => ProtoKeyCode::Left,
        KeyCode::Right => ProtoKeyCode::Right,
        _ => ProtoKeyCode::Other(0),
    };
    let modifiers = key.modifiers.bits() as u8;
    KeyInput { code, modifiers }
}

fn crossterm_mouse_to_proto(mouse: &MouseEvent) -> MouseInput {
    let kind = match mouse.kind {
        MouseEventKind::Down(_) => ProtoMouseKind::Down,
        MouseEventKind::Up(_) => ProtoMouseKind::Up,
        MouseEventKind::Drag(_) => ProtoMouseKind::Drag,
        MouseEventKind::Moved => ProtoMouseKind::Move,
        MouseEventKind::ScrollUp => ProtoMouseKind::ScrollUp,
        MouseEventKind::ScrollDown => ProtoMouseKind::ScrollDown,
        _ => ProtoMouseKind::Move,
    };
    MouseInput {
        kind,
        x: mouse.column,
        y: mouse.row,
    }
}

/// Copy all cells from src to dest buffer (same-size copy).
fn copy_buffer(src: &Buffer, dst: &mut Buffer) {
    for y in 0..src.area.height.min(dst.area.height) {
        for x in 0..src.area.width.min(dst.area.width) {
            if let Some(src_cell) = src.cell((x, y)) {
                dst.cell_mut((x, y)).map(|dst_cell| {
                    let ch: char = src_cell.symbol().chars().next().unwrap_or(' ');
                    dst_cell.set_char(ch);
                    dst_cell.set_style(src_cell.style());
                });
            }
        }
    }
}

/// Copy a buffer into a sub-region of the destination buffer.
pub fn copy_buffer_to_region(src: &Buffer, dst: &mut Buffer, area: Rect) {
    for y in 0..area.height.min(src.area.height) {
        for x in 0..area.width.min(src.area.width) {
            if let Some(src_cell) = src.cell((x, y)) {
                let dst_x = area.x + x;
                let dst_y = area.y + y;
                dst.cell_mut((dst_x, dst_y)).map(|dst_cell| {
                    let ch: char = src_cell.symbol().chars().next().unwrap_or(' ');
                    dst_cell.set_char(ch);
                    dst_cell.set_style(src_cell.style());
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_buffer_to_region_basic() {
        let src_area = Rect::new(0, 0, 5, 3);
        let mut src = Buffer::empty(src_area);
        src.cell_mut((2, 1)).map(|c| {
            c.set_char('X');
        });

        let dst_area = Rect::new(0, 0, 20, 10);
        let mut dst = Buffer::empty(dst_area);

        copy_buffer_to_region(&src, &mut dst, Rect::new(5, 2, 5, 3));
        let cell = dst.cell((7, 3)).unwrap();
        assert_eq!(cell.symbol().chars().next().unwrap(), 'X');
    }
}
