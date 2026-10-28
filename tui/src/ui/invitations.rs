use chrono::{DateTime, Duration, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::state::App;
use crate::i18n::{t, tf};
use crate::ipc::protocol::short;
use crate::ui::{markup, theme};

pub fn draw(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    let block = Block::bordered()
        .border_style(theme::border_style(focused))
        .title(Span::styled(
            t("ui.invitations_title"),
            Style::default().fg(theme::PRIMARY),
        ))
        .style(Style::default().bg(theme::BG_SURFACE));

    if app.invitations.is_empty() {
        frame.render_widget(
            Paragraph::new(markup::text(
                "ui.invitations_empty",
                Style::default().fg(theme::TEXT_DIM),
            ))
            .wrap(Wrap { trim: true })
            .block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .invitations
        .iter()
        .enumerate()
        .map(|(index, invitation)| {
            let incoming = invitation.direction == 1;
            let counterpart = if incoming {
                invitation.inviter_user_id
            } else {
                invitation.invitee_user_id
            };

            let text = tf(
                "ui.invitations_entry",
                &[
                    ("id", &short(&invitation.id)),
                    (
                        "direction",
                        &t(if incoming {
                            "ui.invitation_direction_in"
                        } else {
                            "ui.invitation_direction_out"
                        }),
                    ),
                    ("counterpart", &short(&counterpart)),
                    ("status", &status_label(invitation.status)),
                    ("age", &age_label(app.now, invitation.created_at)),
                ],
            );

            let style = if index == app.invitation_cursor {
                Style::default()
                    .fg(theme::TEXT_ON_ACCENT)
                    .bg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_MAIN)
            };

            ListItem::new(Line::styled(text, style))
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(app.invitation_cursor));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

fn status_label(status: u8) -> String {
    match status {
        0 => t("ui.invitation_status_pending"),
        1 => t("ui.invitation_status_accepted"),
        2 => t("ui.invitation_status_rejected"),
        3 => t("ui.invitation_status_cancelled"),
        other => other.to_string(),
    }
}

fn age_label(now: DateTime<Utc>, created_at: DateTime<Utc>) -> String {
    let minutes = (now - created_at).max(Duration::zero()).num_minutes();
    tf("ui.invitation_age", &[("minutes", &minutes.to_string())])
}
