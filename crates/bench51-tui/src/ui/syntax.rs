//! 8051 assembly syntax highlighting for the TUI.

use bench51_core::syntax::{Tok, tokens};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

fn style(t: Tok) -> Style {
    match t {
        Tok::Plain => Style::new(),
        Tok::Comment => Style::new().fg(Color::DarkGray),
        Tok::Label => Style::new().fg(Color::Yellow),
        Tok::Mnemonic => Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        Tok::Directive => Style::new().fg(Color::Magenta),
        Tok::Register => Style::new().fg(Color::LightBlue),
        Tok::Number => Style::new().fg(Color::LightGreen),
        Tok::Str => Style::new().fg(Color::Green),
        Tok::Sfr => Style::new().fg(Color::LightMagenta),
    }
}

/// Splits a source line into styled spans, applying `base` underneath.
pub fn highlight(line: &str, base: Style) -> Vec<Span<'static>> {
    tokens(line)
        .into_iter()
        .map(|(r, t)| Span::styled(line[r].to_string(), base.patch(style(t))))
        .collect()
}
