use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::state::App;
use crate::i18n::{t, tf};
use crate::ipc::protocol::LinkTokenView;
use crate::ui::{markup, theme};

pub fn draw(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    let block = Block::bordered()
        .border_style(theme::border_style(focused))
        .title(Span::styled(
            t("ui.qr_title"),
            Style::default().fg(theme::PRIMARY),
        ))
        .style(Style::default().bg(theme::BG_SURFACE));

    let Some(token) = &app.link_token else {
        frame.render_widget(
            Paragraph::new(markup::text(
                "ui.qr_no_token",
                Style::default().fg(theme::TEXT_DIM),
            ))
            .wrap(Wrap { trim: true })
            .block(block),
            area,
        );
        return;
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let details = details(app, token);

    let budget = inner.height.saturating_sub(details.len() as u16);
    let qr = qr_lines(token, inner.width, budget);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(qr.len() as u16),
            Constraint::Length(details.len() as u16),
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(qr), rows[0]);
    frame.render_widget(Paragraph::new(details).wrap(Wrap { trim: false }), rows[1]);
}

fn qr_lines(token: &LinkTokenView, width: u16, height: u16) -> Vec<Line<'static>> {
    let code = match QrCode::new(token.qr_payload().as_bytes()) {
        Ok(code) => code,
        Err(err) => {
            return vec![Line::styled(
                tf("ui.qr_build_failed", &[("error", &err.to_string())]),
                Style::default().fg(theme::DANGER),
            )];
        }
    };

    let render = |quiet_zone: bool| -> Vec<Line<'static>> {
        code.render::<Dense1x2>()
            .quiet_zone(quiet_zone)
            .module_dimensions(1, 1)
            .build()
            .lines()
            .map(|row| {
                Line::styled(
                    row.to_string(),
                    Style::default().fg(theme::BG_APP).bg(theme::BEIGE),
                )
            })
            .collect()
    };

    let fits = |lines: &[Line<'static>]| {
        lines.len() <= height as usize && lines.iter().all(|row| row.width() <= width as usize)
    };

    let with_quiet_zone = render(true);
    if fits(&with_quiet_zone) {
        return with_quiet_zone;
    }

    let dense = render(false);
    if fits(&dense) {
        return dense;
    }

    let widest = dense.iter().map(|row| row.width()).max().unwrap_or(0);
    let note = if widest > width as usize {
        t("ui.qr_too_narrow")
    } else {
        tf(
            "ui.qr_too_short",
            &[(
                "missing",
                &dense.len().saturating_sub(height as usize).to_string(),
            )],
        )
    };

    vec![Line::styled(note, Style::default().fg(theme::TEXT_MUTED))]
}

fn details(app: &App, token: &LinkTokenView) -> Vec<Line<'static>> {
    let countdown = match app.link_token_remaining() {
        Some(left) if left.num_seconds() > 0 => tf(
            "ui.qr_expires_in",
            &[
                ("minutes", &(left.num_seconds() / 60).to_string()),
                ("seconds", &format!("{:02}", left.num_seconds() % 60)),
            ],
        ),
        _ => String::new(),
    };

    let label = Style::default().fg(theme::TEXT_MUTED);

    vec![
        Line::from(vec![
            Span::styled(t("ui.qr_label_user_id"), label),
            Span::styled(
                token.user_id.to_string(),
                Style::default().fg(theme::TEXT_MAIN),
            ),
        ]),
        Line::from(vec![
            Span::styled(t("ui.qr_label_token"), label),
            Span::styled(token.token.clone(), Style::default().fg(theme::TEXT_LINK)),
        ]),
        match countdown.is_empty() {
            true => markup::line("ui.qr_expired", Style::default().fg(theme::INFO)),
            false => Line::styled(countdown, Style::default().fg(theme::INFO)),
        },
        Line::raw(""),
        markup::line("ui.qr_other_device", label),
    ]
}
