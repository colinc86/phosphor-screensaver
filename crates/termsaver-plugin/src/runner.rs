//! Plugin runner: handles the stdin/stdout message loop for a plugin binary.

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::time::Duration;

use crate::buffer::PluginBuffer;
use crate::protocol::{HostMessage, PluginMessage};
use crate::ScreensaverPlugin;

/// Run the plugin main loop. Call this from your plugin's `main()`.
///
/// If `--info` is passed as a CLI argument, prints plugin metadata as JSON and exits.
/// Otherwise, enters the message loop reading from stdin and writing to stdout.
pub fn run(mut plugin: impl ScreensaverPlugin) {
    // Handle --info flag
    if std::env::args().any(|a| a == "--info") {
        let info = plugin.info();
        println!("{}", serde_json::to_string(&info).unwrap());
        return;
    }

    // Enter message loop
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let mut buf = PluginBuffer::new(80, 24);

    if let Err(e) = message_loop(&mut plugin, &mut buf, &mut reader, &mut writer) {
        // Plugin crashed — write error to stderr (host will see process exit)
        eprintln!("termsaver plugin error: {e}");
    }
}

fn message_loop(
    plugin: &mut impl ScreensaverPlugin,
    buf: &mut PluginBuffer,
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> io::Result<()> {
    loop {
        let msg = HostMessage::read_from(reader)?;
        match msg {
            HostMessage::Init { width, height } => {
                buf.resize(width, height);
                plugin.init(width, height);
            }
            HostMessage::Update { dt_secs } => {
                let dt = Duration::from_secs_f64(dt_secs);
                plugin.update(dt);
            }
            HostMessage::Render => {
                buf.clear();
                plugin.render(buf);
                let frame_data = buf.encode_frame();
                PluginMessage::Frame(frame_data).write_to(writer)?;
            }
            HostMessage::KeyEvent(key) => {
                let resp = plugin.handle_key(key);
                PluginMessage::Event(resp).write_to(writer)?;
            }
            HostMessage::MouseEvent(mouse) => {
                let resp = plugin.handle_mouse(mouse);
                PluginMessage::Event(resp).write_to(writer)?;
            }
            HostMessage::Config(json) => {
                // Plugins can optionally handle config updates.
                // For now, we just forward it — plugins deserialize in their own code.
                // This is a no-op unless the plugin overrides something.
                let _ = json;
            }
            HostMessage::Shutdown => {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Cell;
    use crate::config_meta::{FieldKind, PluginInfo};
    use crate::protocol::EventResponse;
    use crate::{ConfigField, PluginColor, ScreensaverPlugin};
    use std::io::Cursor;

    struct TestPlugin {
        width: u16,
        height: u16,
        fill_char: char,
    }

    impl ScreensaverPlugin for TestPlugin {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "test".into(),
                description: "Test plugin".into(),
                key_bindings: vec![],
                features: vec![],
                config_fields: vec![ConfigField {
                    key: "fill".into(),
                    label: "Fill Character".into(),
                    kind: FieldKind::Text,
                    default: "X".into(),
                    choices: None,
                }],
            }
        }

        fn init(&mut self, width: u16, height: u16) {
            self.width = width;
            self.height = height;
        }

        fn update(&mut self, _dt: Duration) {}

        fn render(&self, buf: &mut PluginBuffer) {
            for y in 0..buf.height() {
                for x in 0..buf.width() {
                    buf.set(
                        x,
                        y,
                        Cell {
                            ch: self.fill_char,
                            fg: PluginColor::Rgb(255, 255, 255),
                            bg: PluginColor::Reset,
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn message_loop_init_render_shutdown() {
        let mut plugin = TestPlugin {
            width: 0,
            height: 0,
            fill_char: 'A',
        };

        // Build a sequence of host messages
        let mut input_buf = Vec::new();
        HostMessage::Init {
            width: 4,
            height: 2,
        }
        .write_to(&mut input_buf)
        .unwrap();
        HostMessage::Render.write_to(&mut input_buf).unwrap();
        HostMessage::Shutdown.write_to(&mut input_buf).unwrap();

        let mut reader = Cursor::new(input_buf);
        let mut output_buf = Vec::new();
        let mut buf = PluginBuffer::new(4, 2);

        message_loop(&mut plugin, &mut buf, &mut reader, &mut output_buf).unwrap();

        assert_eq!(plugin.width, 4);
        assert_eq!(plugin.height, 2);

        // Read back the frame response
        let mut cursor = Cursor::new(&output_buf);
        let resp = PluginMessage::read_from(&mut cursor).unwrap();
        match resp {
            PluginMessage::Frame(data) => {
                // 4*2 = 8 cells, 12 bytes each = 96 bytes
                assert_eq!(data.len(), 96);
                // First cell should be 'A'
                assert_eq!(data[0], b'A');
            }
            _ => panic!("expected Frame"),
        }
    }

    #[test]
    fn message_loop_key_event() {
        let mut plugin = TestPlugin {
            width: 0,
            height: 0,
            fill_char: 'B',
        };

        let mut input_buf = Vec::new();
        let key = crate::protocol::KeyInput {
            code: crate::protocol::KeyCode::Char('q'),
            modifiers: 0,
        };
        HostMessage::KeyEvent(key)
            .write_to(&mut input_buf)
            .unwrap();
        HostMessage::Shutdown.write_to(&mut input_buf).unwrap();

        let mut reader = Cursor::new(input_buf);
        let mut output_buf = Vec::new();
        let mut buf = PluginBuffer::new(4, 2);

        message_loop(&mut plugin, &mut buf, &mut reader, &mut output_buf).unwrap();

        // Should get an EventResponse::Ignored back
        let mut cursor = Cursor::new(&output_buf);
        let resp = PluginMessage::read_from(&mut cursor).unwrap();
        match resp {
            PluginMessage::Event(r) => assert_eq!(r, EventResponse::Ignored),
            _ => panic!("expected Event"),
        }
    }
}
