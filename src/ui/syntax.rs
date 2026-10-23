//! Minimal 8051 assembly syntax highlighting.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::{isa, sfr};

const COMMENT: Style = Style::new().fg(Color::DarkGray);
const LABEL: Style = Style::new().fg(Color::Yellow);
const MNEMONIC: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
const DIRECTIVE: Style = Style::new().fg(Color::Magenta);
const REGISTER: Style = Style::new().fg(Color::LightBlue);
const NUMBER: Style = Style::new().fg(Color::LightGreen);
const STRING: Style = Style::new().fg(Color::Green);
const SFR: Style = Style::new().fg(Color::LightMagenta);

fn is_directive(w: &str) -> bool {
    matches!(
        w,
        "ORG"
            | "DB"
            | "DW"
            | "DS"
            | "END"
            | "EQU"
            | "SET"
            | "BIT"
            | "DATA"
            | "IDATA"
            | "XDATA"
            | "CODE"
            | "CSEG"
            | "XSEG"
            | "DSEG"
            | "AT"
            | "USING"
            | "NAME"
            | "BYTE"
            | "WORD"
    )
}

fn word_style(w: &str, first_word: bool) -> Style {
    let u = w.to_ascii_uppercase();
    let reg = matches!(u.as_str(), "A" | "AB" | "C" | "DPTR" | "PC")
        || (u.len() == 2
            && u.starts_with('R')
            && u.as_bytes()[1].is_ascii_digit()
            && u.as_bytes()[1] < b'8');
    if reg {
        REGISTER
    } else if is_directive(&u) {
        DIRECTIVE
    } else if first_word && isa::is_mnemonic(&u) {
        MNEMONIC
    } else if sfr::SFRS.iter().chain(sfr::BITS).any(|(n, _)| *n == u) {
        SFR
    } else if matches!(
        u.as_str(),
        "HIGH" | "LOW" | "MOD" | "AND" | "OR" | "XOR" | "NOT" | "SHL" | "SHR"
    ) {
        DIRECTIVE
    } else {
        Style::new()
    }
}

/// Splits a source line into styled spans, applying `base` underneath.
pub fn highlight(line: &str, base: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = line.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    let mut seen_word = false;
    let push = |spans: &mut Vec<Span<'static>>, s: String, st: Style| {
        spans.push(Span::styled(s, base.patch(st)))
    };
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c == ';' {
            push(&mut spans, chars[i..].iter().collect(), COMMENT);
            break;
        } else if c == '\'' || c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            push(&mut spans, chars[start..i].iter().collect(), STRING);
        } else if c.is_ascii_alphabetic() || c == '_' || c == '?' {
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '?')
            {
                i += 1;
            }
            let w: String = chars[start..i].iter().collect();
            let is_label = !seen_word && chars.get(i) == Some(&':');
            let style = if is_label {
                LABEL
            } else {
                let st = word_style(&w, !seen_word);
                seen_word = true;
                st
            };
            push(&mut spans, w, style);
        } else if c.is_ascii_digit() || c == '#' || c == '$' {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            push(&mut spans, chars[start..i].iter().collect(), NUMBER);
        } else {
            while i < chars.len()
                && !(chars[i].is_ascii_alphanumeric() || "_?;'\"#$".contains(chars[i]))
            {
                i += 1;
            }
            push(&mut spans, chars[start..i].iter().collect(), Style::new());
        }
    }
    spans
}
