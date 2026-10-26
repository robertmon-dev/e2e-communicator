use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use uuid::Uuid;

use crate::app::state::App;
use crate::i18n::{t, tf};
use crate::ipc::protocol::short;
use crate::ui::theme;

pub fn draw(frame: &mut Frame, app: &App, chat_id: Uuid, area: Rect, focused: bool) {
    let title = app
        .chats
        .iter()
        .find(|chat| chat.id == chat_id)
        .map(|chat| chat.title())
        .unwrap_or_else(|| tf("ui.chat_fallback_title", &[("id", &short(&chat_id))]));

    let block = Block::bordered()
        .border_style(theme::border_style(focused))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme::PRIMARY),
        ))
        .style(Style::default().bg(theme::BG_SURFACE));

    if app.messages.is_empty() {
        frame.render_widget(
            Paragraph::new(t("ui.conversation_empty"))
                .style(Style::default().fg(theme::TEXT_DIM))
                .wrap(Wrap { trim: true })
                .block(block),
            area,
        );
        return;
    }

    let visible = area.height.saturating_sub(2) as usize;
    let skip = app.messages.len().saturating_sub(visible.max(1));

    let lines: Vec<Line> = app
        .messages
        .iter()
        .skip(skip)
        .map(|message| {
            let own = app.identity.user_id == Some(message.author_id);
            let author = if own {
                t("ui.conversation_me")
            } else {
                short(&message.author_id)
            };

            Line::from(vec![
                Span::styled(
                    format!("{author:<10}"),
                    Style::default().fg(if own { theme::HIGHLIGHT } else { theme::PURPLE }),
                ),
                Span::styled(
                    format!("{}  ", message.created_at.format("%H:%M")),
                    Style::default().fg(theme::TEXT_MUTED),
                ),
                Span::styled(
                    message.text().to_string(),
                    Style::default().fg(theme::TEXT_MAIN),
                ),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
