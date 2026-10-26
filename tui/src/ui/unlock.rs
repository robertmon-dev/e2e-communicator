use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::i18n::t;
use crate::ui::{markup, theme};

pub fn draw(frame: &mut Frame, area: Rect, focused: bool) {
    let block = Block::bordered()
        .border_style(theme::border_style(focused))
        .title(Span::styled(
            t("ui.locked_title"),
            Style::default().fg(theme::WARNING),
        ))
        .style(Style::default().bg(theme::BG_SURFACE));

    frame.render_widget(
        Paragraph::new(markup::text(
            "ui.locked_body",
            Style::default().fg(theme::TEXT_MAIN),
        ))
        .wrap(Wrap { trim: true })
        .block(block),
        area,
    );
}
