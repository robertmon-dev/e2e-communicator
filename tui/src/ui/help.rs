use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::command::{HELP, HELP_KEYS};
use crate::i18n::t;
use crate::ui::{markup, theme};

const SPELLING_COLUMN: usize = 23;
const SINGLE_LINE_WIDTH: u16 = 78;
pub const CENTRE_WIDTH: u16 = SINGLE_LINE_WIDTH + 2;
pub const DOCK_WIDTH: u16 = 34;

pub fn lines(width: u16) -> Vec<Line<'static>> {
    let spelling = Style::default()
        .fg(theme::HIGHLIGHT)
        .add_modifier(Modifier::BOLD);
    let description = Style::default().fg(theme::TEXT_MAIN);
    let heading = Style::default().fg(theme::PRIMARY);

    let dim = Style::default().fg(theme::TEXT_DIM);
    let columns = width as usize;
    let mut out = Vec::new();

    for (command, key) in HELP {
        if width >= SINGLE_LINE_WIDTH {
            let mut line = vec![Span::styled(
                format!("{command:<SPELLING_COLUMN$}  "),
                spelling,
            )];
            line.extend(markup::spans(key, description));
            out.extend(wrap(line, columns, SPELLING_COLUMN + 2));
        } else {
            out.push(Line::styled(command.to_string(), spelling));
            let mut line = vec![Span::styled("    ", description)];
            line.extend(markup::spans(key, description));
            out.extend(wrap(line, columns, 4));
        }
    }

    out.push(Line::raw(""));
    out.push(Line::styled(t("help.keys_title"), heading));
    for key in HELP_KEYS {
        let mut line = vec![Span::styled("  ", dim)];
        line.extend(markup::spans(key, dim));
        out.extend(wrap(line, columns, 4));
    }

    out
}

fn wrap(spans: Vec<Span<'static>>, width: usize, indent: usize) -> Vec<Line<'static>> {
    if width <= indent + 1 {
        return vec![Line::from(spans)];
    }

    let mut words: Vec<Span<'static>> = Vec::new();
    for span in spans {
        for word in span.content.split_inclusive(' ') {
            words.push(Span::styled(word.to_string(), span.style));
        }
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut used = 0;

    for word in words {
        let length = word.content.chars().count();
        let trimmed = word.content.trim_end().chars().count();

        if used + trimmed > width && !current.is_empty() {
            lines.push(Line::from(std::mem::take(&mut current)));
            current.push(Span::styled(" ".repeat(indent), word.style));
            used = indent;
        }

        used += length;
        current.push(word);
    }

    if !current.is_empty() {
        lines.push(Line::from(current));
    }

    lines
}

pub fn draw_docked(frame: &mut Frame, area: Rect) {
    let block = block();
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines(inner.width))
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

pub fn draw_centred(frame: &mut Frame, area: Rect) {
    let block = block();
    let width = CENTRE_WIDTH.min(area.width);
    let rows = lines(width.saturating_sub(2)).len() as u16;
    let height = (rows + 2).min(area.height);

    let window = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    frame.render_widget(Clear, window);
    frame.render_widget(
        Paragraph::new(lines(window.width.saturating_sub(2)))
            .wrap(Wrap { trim: false })
            .block(block),
        window,
    );
}

fn block() -> Block<'static> {
    Block::bordered()
        .border_style(theme::border_style(true))
        .title(Span::styled(
            t("ui.help_title"),
            Style::default().fg(theme::PRIMARY),
        ))
        .style(Style::default().bg(theme::BG_SURFACE))
}
