use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Helper: create a test terminal and render a single frame.
fn render_frame(width: u16, height: u16, render_fn: impl FnOnce(&mut ratatui::Frame)) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_fn(frame);
        })
        .unwrap();
    // Convert buffer to string representation
    let buf = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn snapshot_empty_frame() {
    let output = render_frame(20, 5, |_frame| {});
    insta::assert_snapshot!(output);
}
