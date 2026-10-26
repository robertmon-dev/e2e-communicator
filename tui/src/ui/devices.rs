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
            t("ui.devices_title"),
            Style::default().fg(theme::PRIMARY),
        ))
        .style(Style::default().bg(theme::BG_SURFACE));

    if app.devices.is_empty() {
        frame.render_widget(
            Paragraph::new(markup::text(
                "ui.devices_empty",
                Style::default().fg(theme::TEXT_DIM),
            ))
            .wrap(Wrap { trim: true })
            .block(block),
            area,
        );
        return;
    }

    let mut items: Vec<ListItem> = app
        .devices
        .iter()
        .enumerate()
        .map(|(index, device)| {
            let own = app.identity.device_id == Some(device.device_id);
            let marker = if own {
                t("ui.devices_this_one")
            } else {
                String::new()
            };
            let text = tf(
                "ui.devices_entry",
                &[
                    ("id", &device.short_id()),
                    ("time", &device.registered_at.format("%H:%M").to_string()),
                    ("marker", &marker),
                ],
            );

            let style = if index == app.device_cursor {
                Style::default()
                    .fg(theme::TEXT_ON_ACCENT)
                    .bg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else if own {
                Style::default().fg(theme::HIGHLIGHT)
            } else {
                Style::default().fg(theme::TEXT_MAIN)
            };

            ListItem::new(Line::styled(text, style))
        })
        .collect();

    let hint = Style::default().fg(theme::TEXT_MUTED);
    items.push(ListItem::new(Line::raw("")));
    items.push(ListItem::new(markup::line("ui.devices_key_link", hint)));
    items.push(ListItem::new(markup::line("ui.devices_key_revoke", hint)));

    let mut state = ListState::default().with_selected(Some(app.device_cursor));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}
