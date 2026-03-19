//! Plugin host: manages subprocess plugins and provides the PluginRenderer trait.

use std::io::{self, BufReader, BufWriter};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use phosphor_plugin::buffer::{PluginBuffer, PluginColor};
use phosphor_plugin::protocol::{
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

/// Commands sent from the host thread to the I/O thread.
enum IoCmd {
    Init { width: u16, height: u16 },
    Update { dt_secs: f64 },
    KeyEvent(KeyInput),
    MouseEvent(MouseInput),
    Shutdown,
}

/// Subprocess plugin: communicates with an external binary over stdin/stdout
/// via a background I/O thread.
pub struct SubprocessPlugin {
    name: String,
    cmd_tx: mpsc::Sender<IoCmd>,
    frame_rx: mpsc::Receiver<Buffer>,
    event_rx: mpsc::Receiver<io::Result<EventResponse>>,
    latest_frame: Buffer,
    has_frame: bool,
    // Kept so dropping the handle detaches (rather than leaks) the thread.
    #[allow(dead_code)]
    io_thread: Option<JoinHandle<()>>,
    child_id: u32,
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

        let mut stdin = BufWriter::new(
            child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("no stdin"))?,
        );
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("no stdout"))?,
        );

        // Send config before handing stdin to the I/O thread
        if !config_json.is_empty() && config_json != "{}" {
            HostMessage::Config(config_json.to_string()).write_to(&mut stdin)?;
        }

        let child_id = child.id();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let io_thread = thread::spawn(move || {
            io_thread_main(child, stdin, stdout, cmd_rx, frame_tx, event_tx);
        });

        Ok(Self {
            name: name.to_string(),
            cmd_tx,
            frame_rx,
            event_rx,
            latest_frame: Buffer::empty(Rect::new(0, 0, 80, 24)),
            has_frame: false,
            io_thread: Some(io_thread),
            child_id,
            width: 80,
            height: 24,
        })
    }
}

impl Drop for SubprocessPlugin {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(IoCmd::Shutdown);
        #[cfg(unix)]
        unsafe {
            libc::kill(self.child_id as libc::pid_t, libc::SIGKILL);
        }
    }
}

impl PluginRenderer for SubprocessPlugin {
    fn init(&mut self, w: u16, h: u16) -> io::Result<()> {
        self.width = w;
        self.height = h;
        // Drain stale frames from previous size
        while self.frame_rx.try_recv().is_ok() {}
        self.latest_frame = Buffer::empty(Rect::new(0, 0, w, h));
        self.has_frame = false;
        self.cmd_tx
            .send(IoCmd::Init {
                width: w,
                height: h,
            })
            .ok();
        Ok(())
    }

    fn update(&mut self, dt: Duration) -> io::Result<()> {
        self.cmd_tx
            .send(IoCmd::Update {
                dt_secs: dt.as_secs_f64(),
            })
            .ok();
        Ok(())
    }

    fn render_to_buffer(&mut self) -> io::Result<&Buffer> {
        // Block-wait for the first frame after init.  500ms is generous
        // but only fires once per plugin lifecycle (when `has_frame` is
        // false).  Cold subprocess startup (fork+exec+init) can exceed
        // 50ms, causing an empty (black) first frame that ruins dissolve
        // transitions.  The dissolve itself is 800ms, so 500ms is fine.
        if !self.has_frame
            && let Ok(frame) = self.frame_rx.recv_timeout(Duration::from_millis(500))
        {
            self.latest_frame = frame;
            self.has_frame = true;
        }
        // Drain all available frames, keep the latest
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.latest_frame = frame;
            self.has_frame = true;
        }
        Ok(&self.latest_frame)
    }

    fn handle_event(&mut self, event: &Event) -> io::Result<EventResponse> {
        match event {
            Event::Key(key) => {
                let proto_key = crossterm_key_to_proto(key);
                self.cmd_tx.send(IoCmd::KeyEvent(proto_key)).ok();
            }
            Event::Mouse(mouse) => {
                let proto_mouse = crossterm_mouse_to_proto(mouse);
                self.cmd_tx.send(IoCmd::MouseEvent(proto_mouse)).ok();
            }
            _ => return Ok(EventResponse::Ignored),
        }
        match self.event_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(resp)) => Ok(resp),
            // Plugin IO error (crashed/hung) — treat as ignored so the
            // caller can exit cleanly instead of propagating the error
            // and skipping terminal cleanup.
            Ok(Err(_)) | Err(_) => Ok(EventResponse::Ignored),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Background I/O thread: owns the subprocess and communicates with the host
/// via channels. Batches commands and produces at most one frame per batch.
fn io_thread_main(
    mut child: Child,
    mut stdin: BufWriter<std::process::ChildStdin>,
    mut stdout: BufReader<std::process::ChildStdout>,
    cmd_rx: mpsc::Receiver<IoCmd>,
    frame_tx: mpsc::Sender<Buffer>,
    event_tx: mpsc::Sender<io::Result<EventResponse>>,
) {
    let mut plugin_buf = PluginBuffer::new(80, 24);
    let mut width: u16 = 80;
    let mut height: u16 = 24;

    while let Ok(first) = cmd_rx.recv() {
        // Drain all pending commands into a batch
        let mut batch = vec![first];
        while let Ok(cmd) = cmd_rx.try_recv() {
            batch.push(cmd);
        }

        // Process batch
        let mut need_render = false;
        let mut broken = false;
        for cmd in batch {
            match cmd {
                IoCmd::Init {
                    width: w,
                    height: h,
                } => {
                    width = w;
                    height = h;
                    plugin_buf.resize(w, h);
                    let msg = HostMessage::Init {
                        width: w,
                        height: h,
                    };
                    if msg.write_to(&mut stdin).is_err() {
                        broken = true;
                        break;
                    }
                    need_render = true;
                }
                IoCmd::Update { dt_secs } => {
                    let msg = HostMessage::Update { dt_secs };
                    if msg.write_to(&mut stdin).is_err() {
                        broken = true;
                        break;
                    }
                    need_render = true;
                }
                IoCmd::KeyEvent(key) => {
                    let resp = send_key_event(&mut stdin, &mut stdout, key);
                    let _ = event_tx.send(resp);
                }
                IoCmd::MouseEvent(mouse) => {
                    let resp = send_mouse_event(&mut stdin, &mut stdout, mouse);
                    let _ = event_tx.send(resp);
                }
                IoCmd::Shutdown => {
                    let _ = HostMessage::Shutdown.write_to(&mut stdin);
                    let _ = child.wait();
                    return;
                }
            }
        }

        if broken {
            break;
        }

        // One render per batch (avoids redundant work for multiple queued Updates)
        if need_render {
            match do_render(&mut stdin, &mut stdout, &mut plugin_buf, width, height) {
                Ok(buf) => {
                    let _ = frame_tx.send(buf);
                }
                Err(_) => break, // Plugin died — exit IO thread
            }
        }
    }

    // Cleanup: host dropped channel or pipe broke
    let _ = HostMessage::Shutdown.write_to(&mut stdin);
    let _ = child.kill();
    let _ = child.wait();
}

/// Send Render to the plugin, read the Frame response, decode, and convert to ratatui Buffer.
fn do_render(
    stdin: &mut BufWriter<std::process::ChildStdin>,
    stdout: &mut BufReader<std::process::ChildStdout>,
    plugin_buf: &mut PluginBuffer,
    width: u16,
    height: u16,
) -> io::Result<Buffer> {
    HostMessage::Render.write_to(stdin)?;
    let msg = PluginMessage::read_from(stdout)?;
    if let PluginMessage::Frame(data) = msg {
        plugin_buf.decode_frame(&data);
    }
    Ok(sync_to_buffer(plugin_buf, width, height))
}

/// Send a key event to the plugin and read the response.
fn send_key_event(
    stdin: &mut BufWriter<std::process::ChildStdin>,
    stdout: &mut BufReader<std::process::ChildStdout>,
    key: KeyInput,
) -> io::Result<EventResponse> {
    HostMessage::KeyEvent(key).write_to(stdin)?;
    let msg = PluginMessage::read_from(stdout)?;
    match msg {
        PluginMessage::Event(resp) => Ok(proto_event_to_host(resp)),
        _ => Ok(EventResponse::Ignored),
    }
}

/// Send a mouse event to the plugin and read the response.
fn send_mouse_event(
    stdin: &mut BufWriter<std::process::ChildStdin>,
    stdout: &mut BufReader<std::process::ChildStdout>,
    mouse: MouseInput,
) -> io::Result<EventResponse> {
    HostMessage::MouseEvent(mouse).write_to(stdin)?;
    let msg = PluginMessage::read_from(stdout)?;
    match msg {
        PluginMessage::Event(resp) => Ok(proto_event_to_host(resp)),
        _ => Ok(EventResponse::Ignored),
    }
}

/// Convert a PluginBuffer into a ratatui Buffer.
///
/// Skips the bottom-right cell to prevent terminal scroll. Writing a character
/// at the last column of the last row causes the terminal cursor to wrap past
/// the screen bottom, triggering a scroll that shifts the entire display up.
fn sync_to_buffer(plugin_buf: &PluginBuffer, width: u16, height: u16) -> Buffer {
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    let last_x = width.saturating_sub(1);
    let last_y = height.saturating_sub(1);
    for y in 0..height {
        for x in 0..width {
            if x == last_x && y == last_y {
                continue; // skip bottom-right cell
            }
            if let Some(cell) = plugin_buf.get(x, y)
                && let Some(dst) = buffer.cell_mut((x, y))
            {
                dst.set_char(cell.ch);
                dst.set_style(
                    Style::default()
                        .fg(plugin_color_to_ratatui(cell.fg))
                        .bg(plugin_color_to_ratatui(cell.bg)),
                );
            }
        }
    }
    buffer
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
    let modifiers = key.modifiers.bits();
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
/// Skips the bottom-right cell of dst to prevent terminal scroll.
/// When dst is larger than src (e.g. after an unhandled resize), fills the
/// extra right/bottom edge cells with black to prevent flashing artifacts
/// from ratatui's double-buffer diff.
fn copy_buffer(src: &Buffer, dst: &mut Buffer) {
    let max_y = src.area.height.min(dst.area.height);
    let max_x = src.area.width.min(dst.area.width);
    let last_x = dst.area.width.saturating_sub(1);
    let last_y = dst.area.height.saturating_sub(1);
    for y in 0..max_y {
        for x in 0..max_x {
            if x == last_x && y == last_y {
                continue;
            }
            if let Some(src_cell) = src.cell((x, y))
                && let Some(dst_cell) = dst.cell_mut((x, y))
            {
                let ch: char = src_cell.symbol().chars().next().unwrap_or(' ');
                dst_cell.set_char(ch);
                dst_cell.set_style(src_cell.style());
            }
        }
    }
    // Fill any edge cells beyond the source buffer with black to prevent
    // stale double-buffer content from flashing at the right/bottom edges.
    let black = Style::default().bg(Color::Black);
    for y in 0..dst.area.height {
        for x in max_x..dst.area.width {
            if x == last_x && y == last_y {
                continue;
            }
            if let Some(c) = dst.cell_mut((x, y)) {
                c.set_char(' ');
                c.set_style(black);
            }
        }
    }
    for y in max_y..dst.area.height {
        for x in 0..max_x {
            if x == last_x && y == last_y {
                continue;
            }
            if let Some(c) = dst.cell_mut((x, y)) {
                c.set_char(' ');
                c.set_style(black);
            }
        }
    }
}

/// Copy a buffer into a sub-region of the destination buffer.
/// Skips the bottom-right cell of dst to prevent terminal scroll.
pub fn copy_buffer_to_region(src: &Buffer, dst: &mut Buffer, area: Rect) {
    let last_x = dst.area.width.saturating_sub(1);
    let last_y = dst.area.height.saturating_sub(1);
    for y in 0..area.height.min(src.area.height) {
        for x in 0..area.width.min(src.area.width) {
            let dst_x = area.x + x;
            let dst_y = area.y + y;
            if dst_x == last_x && dst_y == last_y {
                continue;
            }
            if let Some(src_cell) = src.cell((x, y))
                && let Some(dst_cell) = dst.cell_mut((dst_x, dst_y))
            {
                let ch: char = src_cell.symbol().chars().next().unwrap_or(' ');
                dst_cell.set_char(ch);
                dst_cell.set_style(src_cell.style());
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
        if let Some(c) = src.cell_mut((2, 1)) {
            c.set_char('X');
        }

        let dst_area = Rect::new(0, 0, 20, 10);
        let mut dst = Buffer::empty(dst_area);

        copy_buffer_to_region(&src, &mut dst, Rect::new(5, 2, 5, 3));
        let cell = dst.cell((7, 3)).unwrap();
        assert_eq!(cell.symbol().chars().next().unwrap(), 'X');
    }
}
