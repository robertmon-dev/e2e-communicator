use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use crate::i18n::{Segment, segments_of};
use crate::ui::theme;

pub fn token_style() -> Style {
    Style::default()
        .fg(theme::TOKEN_KEY)
        .add_modifier(Modifier::BOLD)
}

pub fn spans(key: &str, base: Style) -> Vec<Span<'static>> {
    spans_with(key, &[], base)
}

pub fn spans_with(key: &str, args: &[(&str, &str)], base: Style) -> Vec<Span<'static>> {
    segments_of(key, args)
        .into_iter()
        .map(|segment| match segment {
            Segment::Plain(text) => Span::styled(text, base),
            Segment::Token(text) => Span::styled(text, token_style()),
        })
        .collect()
}

pub fn line(key: &str, base: Style) -> Line<'static> {
    Line::from(spans(key, base))
}

pub fn text(key: &str, base: Style) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = vec![Line::default()];

    for span in spans(key, base) {
        let mut parts = span.content.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                lines
                    .last_mut()
                    .expect("there is always a current line")
                    .push_span(Span::styled(part.to_string(), span.style));
            }
            if parts.peek().is_some() {
                lines.push(Line::default());
            }
        }
    }

    Text::from(lines)
}
