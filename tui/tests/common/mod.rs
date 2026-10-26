use ratatui::Terminal;
use ratatui::backend::{Backend, TestBackend};

pub fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let width = terminal.backend().size().unwrap().width as usize;
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width.max(1))
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}
