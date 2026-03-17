//! Wire protocol for host <-> plugin communication over stdin/stdout pipes.
//!
//! Format: [u8 type_tag] [u32 payload_len (little-endian)] [payload...]
//!
//! Host -> Plugin messages:
//!   Init(w, h)       tag=1  payload: u16 width + u16 height (4 bytes)
//!   Update(dt)       tag=2  payload: f64 delta_secs (8 bytes)
//!   Render           tag=3  payload: empty
//!   KeyEvent(data)   tag=4  payload: KeyInput encoded
//!   MouseEvent(data) tag=5  payload: MouseInput encoded
//!   Config(json)     tag=6  payload: JSON bytes
//!   Shutdown         tag=7  payload: empty
//!
//! Plugin -> Host messages:
//!   Frame(cells)     tag=128  payload: encoded cell data
//!   Event(response)  tag=129  payload: u8 EventResponse

use std::io::{self, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

/// Response from a screensaver handling an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResponse {
    /// Continue running the screensaver.
    Continue = 0,
    /// Exit the screensaver.
    Exit = 1,
    /// The screensaver didn't handle this event.
    Ignored = 2,
}

impl EventResponse {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => EventResponse::Continue,
            1 => EventResponse::Exit,
            _ => EventResponse::Ignored,
        }
    }
}

/// Key input forwarded from the host.
#[derive(Debug, Clone)]
pub struct KeyInput {
    pub code: KeyCode,
    pub modifiers: u8,
}

/// Simplified key codes for the plugin protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Space,
    Other(u16),
}

/// Mouse input forwarded from the host.
#[derive(Debug, Clone)]
pub struct MouseInput {
    pub kind: MouseKind,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseKind {
    Down,
    Up,
    Drag,
    Move,
    ScrollUp,
    ScrollDown,
}

/// Messages sent from host to plugin.
#[derive(Debug)]
pub enum HostMessage {
    Init { width: u16, height: u16 },
    Update { dt_secs: f64 },
    Render,
    KeyEvent(KeyInput),
    MouseEvent(MouseInput),
    Config(String),
    Shutdown,
}

/// Messages sent from plugin to host.
#[derive(Debug)]
pub enum PluginMessage {
    Frame(Vec<u8>),
    Event(EventResponse),
}

// --- Encoding / Decoding ---

/// Write a message header (tag + payload length).
fn write_header(w: &mut impl Write, tag: u8, payload_len: u32) -> io::Result<()> {
    w.write_u8(tag)?;
    w.write_u32::<LittleEndian>(payload_len)?;
    Ok(())
}

/// Read a message header, returns (tag, payload_len).
fn read_header(r: &mut impl Read) -> io::Result<(u8, u32)> {
    let tag = r.read_u8()?;
    let len = r.read_u32::<LittleEndian>()?;
    Ok((tag, len))
}

impl HostMessage {
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        match self {
            HostMessage::Init { width, height } => {
                write_header(w, 1, 4)?;
                w.write_u16::<LittleEndian>(*width)?;
                w.write_u16::<LittleEndian>(*height)?;
            }
            HostMessage::Update { dt_secs } => {
                write_header(w, 2, 8)?;
                w.write_f64::<LittleEndian>(*dt_secs)?;
            }
            HostMessage::Render => {
                write_header(w, 3, 0)?;
            }
            HostMessage::KeyEvent(key) => {
                let payload = encode_key(key);
                write_header(w, 4, payload.len() as u32)?;
                w.write_all(&payload)?;
            }
            HostMessage::MouseEvent(mouse) => {
                let payload = encode_mouse(mouse);
                write_header(w, 5, payload.len() as u32)?;
                w.write_all(&payload)?;
            }
            HostMessage::Config(json) => {
                let bytes = json.as_bytes();
                write_header(w, 6, bytes.len() as u32)?;
                w.write_all(bytes)?;
            }
            HostMessage::Shutdown => {
                write_header(w, 7, 0)?;
            }
        }
        w.flush()?;
        Ok(())
    }

    pub fn read_from(r: &mut impl Read) -> io::Result<Self> {
        let (tag, len) = read_header(r)?;
        match tag {
            1 => {
                let width = r.read_u16::<LittleEndian>()?;
                let height = r.read_u16::<LittleEndian>()?;
                Ok(HostMessage::Init { width, height })
            }
            2 => {
                let dt_secs = r.read_f64::<LittleEndian>()?;
                Ok(HostMessage::Update { dt_secs })
            }
            3 => Ok(HostMessage::Render),
            4 => {
                let mut payload = vec![0u8; len as usize];
                r.read_exact(&mut payload)?;
                Ok(HostMessage::KeyEvent(decode_key(&payload)))
            }
            5 => {
                let mut payload = vec![0u8; len as usize];
                r.read_exact(&mut payload)?;
                Ok(HostMessage::MouseEvent(decode_mouse(&payload)))
            }
            6 => {
                let mut payload = vec![0u8; len as usize];
                r.read_exact(&mut payload)?;
                let json = String::from_utf8_lossy(&payload).into_owned();
                Ok(HostMessage::Config(json))
            }
            7 => Ok(HostMessage::Shutdown),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown host message tag: {tag}"),
            )),
        }
    }
}

impl PluginMessage {
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        match self {
            PluginMessage::Frame(data) => {
                write_header(w, 128, data.len() as u32)?;
                w.write_all(data)?;
            }
            PluginMessage::Event(resp) => {
                write_header(w, 129, 1)?;
                w.write_u8(*resp as u8)?;
            }
        }
        w.flush()?;
        Ok(())
    }

    pub fn read_from(r: &mut impl Read) -> io::Result<Self> {
        let (tag, len) = read_header(r)?;
        match tag {
            128 => {
                let mut data = vec![0u8; len as usize];
                r.read_exact(&mut data)?;
                Ok(PluginMessage::Frame(data))
            }
            129 => {
                let resp = r.read_u8()?;
                Ok(PluginMessage::Event(EventResponse::from_u8(resp)))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown plugin message tag: {tag}"),
            )),
        }
    }
}

// --- Key/Mouse encoding helpers ---

fn encode_key(key: &KeyInput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    buf.push(key.modifiers);
    match &key.code {
        KeyCode::Char(c) => {
            buf.push(0);
            let mut char_buf = [0u8; 4];
            let len = c.encode_utf8(&mut char_buf).len();
            buf.push(len as u8);
            buf.extend_from_slice(&char_buf[..len]);
        }
        KeyCode::Enter => buf.push(1),
        KeyCode::Esc => buf.push(2),
        KeyCode::Backspace => buf.push(3),
        KeyCode::Tab => buf.push(4),
        KeyCode::Up => buf.push(5),
        KeyCode::Down => buf.push(6),
        KeyCode::Left => buf.push(7),
        KeyCode::Right => buf.push(8),
        KeyCode::Space => buf.push(9),
        KeyCode::Other(v) => {
            buf.push(255);
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    buf
}

fn decode_key(data: &[u8]) -> KeyInput {
    if data.is_empty() {
        return KeyInput {
            code: KeyCode::Other(0),
            modifiers: 0,
        };
    }
    let modifiers = data[0];
    let code = if data.len() < 2 {
        KeyCode::Other(0)
    } else {
        match data[1] {
            0 => {
                // Char
                if data.len() >= 3 {
                    let char_len = data[2] as usize;
                    if data.len() >= 3 + char_len {
                        let s = std::str::from_utf8(&data[3..3 + char_len])
                            .ok()
                            .and_then(|s| s.chars().next())
                            .unwrap_or('\0');
                        KeyCode::Char(s)
                    } else {
                        KeyCode::Char('\0')
                    }
                } else {
                    KeyCode::Char('\0')
                }
            }
            1 => KeyCode::Enter,
            2 => KeyCode::Esc,
            3 => KeyCode::Backspace,
            4 => KeyCode::Tab,
            5 => KeyCode::Up,
            6 => KeyCode::Down,
            7 => KeyCode::Left,
            8 => KeyCode::Right,
            9 => KeyCode::Space,
            255 => {
                if data.len() >= 4 {
                    let v = u16::from_le_bytes([data[2], data[3]]);
                    KeyCode::Other(v)
                } else {
                    KeyCode::Other(0)
                }
            }
            _ => KeyCode::Other(data[1] as u16),
        }
    };
    KeyInput { code, modifiers }
}

fn encode_mouse(mouse: &MouseInput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5);
    buf.push(match mouse.kind {
        MouseKind::Down => 0,
        MouseKind::Up => 1,
        MouseKind::Drag => 2,
        MouseKind::Move => 3,
        MouseKind::ScrollUp => 4,
        MouseKind::ScrollDown => 5,
    });
    buf.extend_from_slice(&mouse.x.to_le_bytes());
    buf.extend_from_slice(&mouse.y.to_le_bytes());
    buf
}

fn decode_mouse(data: &[u8]) -> MouseInput {
    if data.len() < 5 {
        return MouseInput {
            kind: MouseKind::Down,
            x: 0,
            y: 0,
        };
    }
    let kind = match data[0] {
        0 => MouseKind::Down,
        1 => MouseKind::Up,
        2 => MouseKind::Drag,
        3 => MouseKind::Move,
        4 => MouseKind::ScrollUp,
        5 => MouseKind::ScrollDown,
        _ => MouseKind::Down,
    };
    let x = u16::from_le_bytes([data[1], data[2]]);
    let y = u16::from_le_bytes([data[3], data[4]]);
    MouseInput { kind, x, y }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn host_message_round_trip_init() {
        let msg = HostMessage::Init {
            width: 80,
            height: 24,
        };
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = HostMessage::read_from(&mut cursor).unwrap();
        match decoded {
            HostMessage::Init { width, height } => {
                assert_eq!(width, 80);
                assert_eq!(height, 24);
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn host_message_round_trip_update() {
        let msg = HostMessage::Update { dt_secs: 0.0333 };
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = HostMessage::read_from(&mut cursor).unwrap();
        match decoded {
            HostMessage::Update { dt_secs } => {
                assert!((dt_secs - 0.0333).abs() < 1e-10);
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn host_message_round_trip_render() {
        let msg = HostMessage::Render;
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = HostMessage::read_from(&mut cursor).unwrap();
        assert!(matches!(decoded, HostMessage::Render));
    }

    #[test]
    fn host_message_round_trip_config() {
        let msg = HostMessage::Config(r#"{"speed": 1.5}"#.into());
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = HostMessage::read_from(&mut cursor).unwrap();
        match decoded {
            HostMessage::Config(json) => assert_eq!(json, r#"{"speed": 1.5}"#),
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn host_message_round_trip_shutdown() {
        let msg = HostMessage::Shutdown;
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = HostMessage::read_from(&mut cursor).unwrap();
        assert!(matches!(decoded, HostMessage::Shutdown));
    }

    #[test]
    fn plugin_message_round_trip_frame() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let msg = PluginMessage::Frame(data.clone());
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = PluginMessage::read_from(&mut cursor).unwrap();
        match decoded {
            PluginMessage::Frame(d) => assert_eq!(d, data),
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn plugin_message_round_trip_event() {
        for resp in [
            EventResponse::Continue,
            EventResponse::Exit,
            EventResponse::Ignored,
        ] {
            let msg = PluginMessage::Event(resp);
            let mut buf = Vec::new();
            msg.write_to(&mut buf).unwrap();
            let mut cursor = Cursor::new(&buf);
            let decoded = PluginMessage::read_from(&mut cursor).unwrap();
            match decoded {
                PluginMessage::Event(r) => assert_eq!(r, resp),
                _ => panic!("wrong message type"),
            }
        }
    }

    #[test]
    fn key_input_round_trip() {
        let keys = [
            KeyInput {
                code: KeyCode::Char('a'),
                modifiers: 0,
            },
            KeyInput {
                code: KeyCode::Char('\u{2588}'),
                modifiers: 1,
            },
            KeyInput {
                code: KeyCode::Enter,
                modifiers: 0,
            },
            KeyInput {
                code: KeyCode::Space,
                modifiers: 2,
            },
            KeyInput {
                code: KeyCode::Up,
                modifiers: 0,
            },
        ];
        for key in &keys {
            let encoded = encode_key(key);
            let decoded = decode_key(&encoded);
            assert_eq!(decoded.modifiers, key.modifiers);
            assert_eq!(decoded.code, key.code);
        }
    }

    #[test]
    fn mouse_input_round_trip() {
        let mouse = MouseInput {
            kind: MouseKind::Drag,
            x: 42,
            y: 13,
        };
        let encoded = encode_mouse(&mouse);
        let decoded = decode_mouse(&encoded);
        assert_eq!(decoded.kind, MouseKind::Drag);
        assert_eq!(decoded.x, 42);
        assert_eq!(decoded.y, 13);
    }

    #[test]
    fn key_event_message_round_trip() {
        let key = KeyInput {
            code: KeyCode::Char('r'),
            modifiers: 0,
        };
        let msg = HostMessage::KeyEvent(key);
        let mut buf = Vec::new();
        msg.write_to(&mut buf).unwrap();
        let mut cursor = Cursor::new(&buf);
        let decoded = HostMessage::read_from(&mut cursor).unwrap();
        match decoded {
            HostMessage::KeyEvent(k) => {
                assert_eq!(k.code, KeyCode::Char('r'));
                assert_eq!(k.modifiers, 0);
            }
            _ => panic!("wrong message type"),
        }
    }
}
