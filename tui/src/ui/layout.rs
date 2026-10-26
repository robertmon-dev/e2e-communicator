use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::{App, ConnectionStatus, Focus, HelpPlacement, View};
use crate::i18n::{t, tf};
use crate::ui::{chats, conversation, devices, help, markup, prompt, qr, stream, theme, unlock};

const CHAT_COLUMN_WIDTH: u16 = 22;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let prompt_height = if prompt::is_visible(app) {
        prompt::HEIGHT
    } else {
        0
    };

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(prompt_height),
            Constraint::Length(1),
        ])
        .split(area);

    draw_body(frame, app, vertical[0]);
    prompt::draw(frame, app, vertical[1]);
    draw_status_bar(frame, app, vertical[2]);
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let docked = app.help == Some(HelpPlacement::Left);

    if let View::Conversation(chat_id) = app.view
        && !docked
    {
        conversation::draw(frame, app, chat_id, area, true);
        draw_help_window(frame, app, area);
        return;
    }

    if app.view == View::Unlock && !docked {
        unlock::draw(frame, area, true);
        draw_help_window(frame, app, area);
        return;
    }

    let left_width = if docked {
        help::DOCK_WIDTH
    } else {
        CHAT_COLUMN_WIDTH
    };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width.min(area.width.saturating_sub(1))),
            Constraint::Min(1),
        ])
        .split(area);

    if docked {
        help::draw_docked(frame, columns[0]);
    } else {
        chats::draw(frame, app, columns[0], app.focus == Focus::Chats);
    }

    let main_focused = app.focus == Focus::Main;
    match app.view {
        View::Devices => devices::draw(frame, app, columns[1], main_focused),
        View::LinkToken => qr::draw(frame, app, columns[1], main_focused),
        View::Conversation(chat_id) => {
            conversation::draw(frame, app, chat_id, columns[1], main_focused)
        }
        View::Stream => stream::draw(frame, app, columns[1], main_focused),
        View::Unlock => unlock::draw(frame, columns[1], main_focused),
    }

    draw_help_window(frame, app, area);
}

fn draw_help_window(frame: &mut Frame, app: &App, area: Rect) {
    if app.help == Some(HelpPlacement::Centre) {
        help::draw_centred(frame, area);
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();

    match &app.connection {
        ConnectionStatus::Connected => {
            spans.push(Span::styled("● ", Style::default().fg(theme::SUCCESS)));
            spans.push(Span::styled(
                app.connectivity.clone(),
                Style::default().fg(theme::TEXT_MAIN),
            ));
        }
        ConnectionStatus::Disconnected { reason } => {
            spans.push(Span::styled("● ", Style::default().fg(theme::DANGER)));
            const REASON_BUDGET: usize = 24;
            let reason: String = match reason.chars().count() > REASON_BUDGET {
                true => reason
                    .chars()
                    .take(REASON_BUDGET - 1)
                    .chain(['…'])
                    .collect(),
                false => reason.clone(),
            };
            spans.push(Span::styled(
                tf("status.no_client", &[("reason", &reason)]),
                Style::default().fg(theme::TEXT_MAIN),
            ));
        }
    }

    spans.push(Span::raw("  "));
    if app.authenticated {
        spans.push(Span::styled(
            t("status.session_on"),
            Style::default().fg(theme::SUCCESS),
        ));
    } else {
        spans.push(Span::styled(
            t("status.session_off"),
            Style::default().fg(theme::TEXT_MUTED),
        ));
    }

    match app.identity.short_label() {
        Some(label) => spans.push(Span::styled(
            format!("  {label}"),
            Style::default().fg(theme::TEXT_LINK),
        )),
        None => spans.push(Span::styled(
            format!("  {}", t("status.no_identity")),
            Style::default().fg(theme::TEXT_MUTED),
        )),
    }

    let keys = match (app.view, app.identity.state.as_str()) {
        (View::Unlock, _) => "status.keys_unlock",
        (View::Conversation(_), _) => "status.keys_conversation",
        (View::Devices, _) => "status.keys_devices",
        (View::LinkToken, _) => "status.keys_link_token",
        (_, "unregistered") => "status.keys_unregistered",
        (_, "conflict") => "status.keys_conflict",
        _ => "status.keys_default",
    };
    spans.extend(markup::spans(keys, Style::default().fg(theme::TEXT_MUTED)));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_SIDEBAR)),
        area,
    );
}
