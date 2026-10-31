//! Minimal 8051 assembly tokenizer for syntax highlighting.

use std::ops::Range;

use crate::{isa, sfr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    /// Whitespace, punctuation and identifiers without a special meaning.
    Plain,
    Comment,
    Str,
    /// A label definition (`name:` at the start of a line).
    Label,
    Mnemonic,
    /// Directives and expression operators (`ORG`, `DB`, `HIGH`, `MOD`, …).
    Directive,
    Register,
    /// Predefined SFR and bit names.
    Sfr,
    Number,
}

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

fn word_kind(w: &str, first_word: bool) -> Tok {
    let u = w.to_ascii_uppercase();
    let reg = matches!(u.as_str(), "A" | "AB" | "C" | "DPTR" | "PC")
        || (u.len() == 2
            && u.starts_with('R')
            && u.as_bytes()[1].is_ascii_digit()
            && u.as_bytes()[1] < b'8');
    if reg {
        Tok::Register
    } else if is_directive(&u) {
        Tok::Directive
    } else if first_word && isa::is_mnemonic(&u) {
        Tok::Mnemonic
    } else if sfr::SFRS.iter().chain(sfr::BITS).any(|(n, _)| *n == u) {
        Tok::Sfr
    } else if matches!(
        u.as_str(),
        "HIGH" | "LOW" | "MOD" | "AND" | "OR" | "XOR" | "NOT" | "SHL" | "SHR"
    ) {
        Tok::Directive
    } else {
        Tok::Plain
    }
}

/// Splits a source line into classified byte ranges that together cover
/// the whole line.
pub fn tokens(line: &str) -> Vec<(Range<usize>, Tok)> {
    let b = line.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut seen_word = false;
    while i < n {
        let c = b[i];
        let start = i;
        let tok = if c == b';' {
            i = n;
            Tok::Comment
        } else if c == b'\'' || c == b'"' {
            i += 1;
            while i < n && b[i] != c {
                i += 1;
            }
            i = (i + 1).min(n);
            Tok::Str
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'?' {
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'?') {
                i += 1;
            }
            if !seen_word && b.get(i) == Some(&b':') {
                Tok::Label
            } else {
                let t = word_kind(&line[start..i], !seen_word);
                seen_word = true;
                t
            }
        } else if c.is_ascii_digit() || c == b'#' || c == b'$' {
            i += 1;
            while i < n && b[i].is_ascii_alphanumeric() {
                i += 1;
            }
            Tok::Number
        } else {
            // Non-ASCII bytes land here too, so ranges stay on char boundaries.
            while i < n && !(b[i].is_ascii_alphanumeric() || b"_?;'\"#$".contains(&b[i])) {
                i += 1;
            }
            Tok::Plain
        };
        out.push((start..i, tok));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Tok, tokens};

    fn kinds(line: &str) -> Vec<(&str, Tok)> {
        tokens(line)
            .into_iter()
            .filter(|(_, t)| *t != Tok::Plain)
            .map(|(r, t)| (&line[r], t))
            .collect()
    }

    #[test]
    fn classifies_an_instruction_line() {
        assert_eq!(
            kinds("main:   MOV TMOD,#20h   ; timer 1"),
            vec![
                ("main", Tok::Label),
                ("MOV", Tok::Mnemonic),
                ("TMOD", Tok::Sfr),
                ("#20h", Tok::Number),
                ("; timer 1", Tok::Comment),
            ]
        );
        assert_eq!(
            kinds("  mov a,r7"),
            vec![
                ("mov", Tok::Mnemonic),
                ("a", Tok::Register),
                ("r7", Tok::Register)
            ]
        );
    }

    #[test]
    fn directives_strings_and_operators() {
        assert_eq!(
            kinds("msg: DB \"Hi; there\", HIGH(x) ; c"),
            vec![
                ("msg", Tok::Label),
                ("DB", Tok::Directive),
                ("\"Hi; there\"", Tok::Str),
                ("HIGH", Tok::Directive),
                ("; c", Tok::Comment),
            ]
        );
    }

    #[test]
    fn ranges_cover_the_line_even_with_unicode() {
        for line in [
            "",
            "   ",
            "x DB 'é', 1 ; µs → ok",
            "\"unterminated",
            "€€ MOV",
        ] {
            let t = tokens(line);
            let mut pos = 0;
            for (r, _) in &t {
                assert_eq!(r.start, pos, "{line:?}");
                assert!(line.is_char_boundary(r.end));
                pos = r.end;
            }
            assert_eq!(pos, line.len());
        }
    }
}
