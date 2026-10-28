use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::state::App;
use crate::i18n::{t, tf};
use crate::ui::{markup, theme};

pub fn draw(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    let block = Block::bordered()
        .border_style(theme::border_style(focused))
        .title(Span::styled(
            t("ui.chats_title"),
            Style::default().fg(theme::PRIMARY),
        ))
        .style(Style::default().bg(theme::BG_SIDEBAR));

    if app.chats.is_empty() {
        let key = if app.identity.is_registered() {
            "ui.chats_empty"
        } else {
            "ui.chats_no_identity"
        };

        frame.render_widget(
            Paragraph::new(markup::text(key, Style::default().fg(theme::TEXT_DIM)))
                .wrap(Wrap { trim: true })
                .block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .chats
        .iter()
        .enumerate()
        .map(|(index, chat)| {
            let style = if index == app.chat_cursor {
                Style::default()
                    .fg(theme::TEXT_ON_ACCENT)
                    .bg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_MAIN)
            };

            let text = match app.key_exchange.get(&chat.id) {
                Some((known, total)) => format!(
                    "{}  {}",
                    chat.title(),
                    tf(
                        "ui.key_exchange_progress",
                        &[("known", &known.to_string()), ("total", &total.to_string())],
                    )
                ),
                None => chat.title(),
            };

            ListItem::new(Line::styled(text, style))
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(app.chat_cursor));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}
