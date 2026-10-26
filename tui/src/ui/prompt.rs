use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::state::{App, Focus, InputMode};
use crate::i18n::t;
use crate::ui::theme;

pub const HEIGHT: u16 = 3;
const MASK: &str = "\u{2022}";

pub fn is_visible(app: &App) -> bool {
    app.confirm.is_some() || app.input != InputMode::Idle
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Command;

    if let Some(confirm) = &app.confirm {
        let block = block(t("ui.confirm_title"), true);
        frame.render_widget(
            Paragraph::new(confirm.prompt.clone())
                .style(
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                )
                .wrap(Wrap { trim: true })
                .block(block),
            area,
        );
        return;
    }

    let (title, prompt, buffer, colour) = match &app.input {
        InputMode::Idle => return,
        InputMode::Command(buffer) => {
            (t("ui.command_title"), ":", buffer.clone(), theme::HIGHLIGHT)
        }
        InputMode::Message(buffer) => (t("ui.message_title"), "> ", buffer.clone(), theme::SUCCESS),
        InputMode::Pin(buffer) => (
            t("ui.unlock_title"),
            "PIN ",
            MASK.repeat(buffer.chars().count()),
            theme::WARNING,
        ),
    };

    let mut spans = vec![
        Span::styled(prompt, Style::default().fg(colour)),
        Span::styled(buffer, Style::default().fg(colour)),
    ];

    if focused {
        spans.push(Span::styled(
            "▏",
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(block(title, focused)),
        area,
    );
}

fn block(title: String, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_style(theme::border_style(focused))
        .title(Span::styled(title, Style::default().fg(theme::PRIMARY)))
        .style(Style::default().bg(theme::BG_INPUT))
}
