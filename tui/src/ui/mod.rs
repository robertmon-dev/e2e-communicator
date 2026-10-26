pub mod chats;
pub mod conversation;
pub mod devices;
pub mod help;
pub mod layout;
pub mod markup;
pub mod prompt;
pub mod qr;
pub mod stream;
pub mod theme;

use ratatui::Frame;
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::app::state::App;

pub fn draw(frame: &mut Frame, app: &App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BG_APP).fg(theme::TEXT_MAIN)),
        frame.area(),
    );

    layout::draw(frame, app);
}
