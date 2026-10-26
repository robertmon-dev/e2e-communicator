use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::state::{App, StreamKind};
use crate::i18n::t;
use crate::ui::theme;

pub fn draw(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    let title = if app.raw_stream {
        t("stream.title_raw")
    } else {
        t("stream.title")
    };

    let visible = area.height.saturating_sub(2) as usize;
    let skip = app.stream.len().saturating_sub(visible.max(1));

    let lines: Vec<Line> = app
        .stream
        .iter()
        .skip(skip)
        .map(|entry| {
            let colour = match entry.kind {
                StreamKind::Info => theme::TEXT_MAIN,
                StreamKind::Outgoing => theme::HIGHLIGHT,
                StreamKind::Incoming => theme::SUCCESS,
                StreamKind::Problem => theme::DANGER,
            };

            Line::from(vec![
                Span::styled(
                    format!("{} ", entry.at.format("%H:%M:%S")),
                    Style::default().fg(theme::TEXT_MUTED),
                ),
                Span::styled(entry.text.clone(), Style::default().fg(colour)),
            ])
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_style(theme::border_style(focused))
                .title(Span::styled(title, Style::default().fg(theme::PRIMARY)))
                .style(Style::default().bg(theme::BG_SURFACE)),
        ),
        area,
    );
}
