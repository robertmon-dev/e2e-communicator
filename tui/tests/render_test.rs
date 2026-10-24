use ratatui::Terminal;
use ratatui::backend::TestBackend;

pub fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(terminal.backend().size().unwrap().width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

#[test]
fn the_placeholder_screen_renders() {
    let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();

    terminal.draw(tui::ui::draw_placeholder).unwrap();

    let text = rows(&terminal).join("\n");
    assert!(
        text.contains("e2e-communicator"),
        "expected the title in the buffer, got:\n{text}"
    );
}
