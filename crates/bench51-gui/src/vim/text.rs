//! Char-indexed text helpers for the Vim engine: lines, columns, word
//! classes, motions and text objects. Positions are char indices.

/// How a motion's range is taken by an operator (`:help exclusive`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Exclusive,
    Inclusive,
    Linewise,
}

pub struct Buf {
    pub c: Vec<char>,
}

pub fn is_blank(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn is_space(ch: char) -> bool {
    ch.is_whitespace()
}

pub fn is_keyword(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// 0 whitespace, 1 keyword, 2 punctuation; `big` words only tell
/// whitespace from the rest.
fn class(ch: char, big: bool) -> u8 {
    if is_space(ch) {
        0
    } else if big || is_keyword(ch) {
        1
    } else {
        2
    }
}

impl Buf {
    pub fn new(text: &str) -> Self {
        Self {
            c: text.chars().collect(),
        }
    }

    pub fn text(&self) -> String {
        self.c.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.c.len()
    }

    pub fn line_start(&self, pos: usize) -> usize {
        let mut i = pos.min(self.len());
        while i > 0 && self.c[i - 1] != '\n' {
            i -= 1;
        }
        i
    }

    /// Index of the line's `\n`, or the text length on the last line.
    pub fn line_end(&self, pos: usize) -> usize {
        let mut i = pos.min(self.len());
        while i < self.len() && self.c[i] != '\n' {
            i += 1;
        }
        i
    }

    pub fn line_of(&self, pos: usize) -> usize {
        self.c[..pos.min(self.len())]
            .iter()
            .filter(|&&ch| ch == '\n')
            .count()
    }

    pub fn lines(&self) -> usize {
        self.c.iter().filter(|&&ch| ch == '\n').count() + 1
    }

    /// Start of `line`, clamped to the last line.
    pub fn start_of_line(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        let mut seen = 0;
        let mut last_start = 0;
        for (i, &ch) in self.c.iter().enumerate() {
            if ch == '\n' {
                seen += 1;
                last_start = i + 1;
                if seen == line {
                    return i + 1;
                }
            }
        }
        last_start
    }

    pub fn col(&self, pos: usize) -> usize {
        pos - self.line_start(pos)
    }

    pub fn line_is_blank(&self, line: usize) -> bool {
        let s = self.start_of_line(line);
        let e = self.line_end(s);
        self.c[s..e].iter().all(|&ch| is_blank(ch))
    }

    pub fn line_text(&self, pos: usize) -> String {
        let s = self.line_start(pos);
        self.c[s..self.line_end(s)].iter().collect()
    }

    /// Position on `line` at `col`, clamped to the line's last char.
    pub fn at_col(&self, line: usize, col: usize) -> usize {
        let s = self.start_of_line(line);
        let e = self.line_end(s);
        if e == s { s } else { (s + col).min(e - 1) }
    }

    pub fn first_non_blank(&self, pos: usize) -> usize {
        let s = self.line_start(pos);
        let e = self.line_end(s);
        let mut i = s;
        while i < e && is_blank(self.c[i]) {
            i += 1;
        }
        if i == e && e > s { e - 1 } else { i }
    }

    /// In Normal mode the cursor sits on a character, never on the `\n`
    /// (except on an empty line).
    pub fn clamp_normal(&self, pos: usize) -> usize {
        let pos = pos.min(self.len());
        let s = self.line_start(pos);
        let e = self.line_end(pos);
        if e > s && pos >= e { e - 1 } else { pos }
    }

    pub fn insert(&mut self, at: usize, s: &str) -> usize {
        let new: Vec<char> = s.chars().collect();
        let n = new.len();
        self.c.splice(at..at, new);
        n
    }

    pub fn remove(&mut self, start: usize, end: usize) -> String {
        self.c.drain(start..end).collect()
    }

    // ---- Motions ----

    pub fn word_fwd(&self, pos: usize, big: bool) -> usize {
        let n = self.len();
        if pos >= n {
            return n;
        }
        let mut i = pos;
        let cls = class(self.c[i], big);
        if cls != 0 {
            while i < n && class(self.c[i], big) == cls {
                i += 1;
            }
        }
        while i < n && is_space(self.c[i]) {
            // An empty line counts as a word.
            if self.c[i] == '\n' && i + 1 < n && self.c[i + 1] == '\n' {
                return i + 1;
            }
            i += 1;
        }
        i
    }

    pub fn word_back(&self, pos: usize, big: bool) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut i = pos - 1;
        while is_space(self.c[i]) {
            let empty_line = self.c[i] == '\n' && (i == 0 || self.c[i - 1] == '\n');
            if empty_line && i < pos {
                return i;
            }
            if i == 0 {
                return 0;
            }
            i -= 1;
        }
        let cls = class(self.c[i], big);
        while i > 0 && class(self.c[i - 1], big) == cls {
            i -= 1;
        }
        i
    }

    pub fn word_end(&self, pos: usize, big: bool) -> usize {
        let n = self.len();
        if n == 0 {
            return 0;
        }
        let mut i = pos + 1;
        while i < n && is_space(self.c[i]) {
            i += 1;
        }
        if i >= n {
            return n - 1;
        }
        let cls = class(self.c[i], big);
        while i + 1 < n && class(self.c[i + 1], big) == cls {
            i += 1;
        }
        i
    }

    /// End (inclusive) of the run of same-class chars starting at `pos`.
    pub fn run_end(&self, pos: usize, big: bool) -> usize {
        let cls = class(self.c[pos], big);
        let mut i = pos;
        while i + 1 < self.len() && self.c[i + 1] != '\n' && class(self.c[i + 1], big) == cls {
            i += 1;
        }
        i
    }

    /// `f`/`t` (forward) or `F`/`T` (backward) to `ch` on the current line.
    /// `repeat` is for `;` and `,`, where `t` mustn't stay stuck right
    /// before the char it stopped at last time.
    pub fn find_char(
        &self,
        pos: usize,
        kind: char,
        ch: char,
        count: usize,
        repeat: bool,
    ) -> Option<usize> {
        let s = self.line_start(pos);
        let e = self.line_end(pos);
        let mut i = pos;
        for k in 0..count {
            let skip = repeat && matches!(kind, 't' | 'T') && k == 0;
            i = match kind {
                'f' | 't' => {
                    let from = i + 1 + usize::from(skip && i + 1 < e && self.c[i + 1] == ch);
                    (from..e).find(|&j| self.c[j] == ch)?
                }
                _ => {
                    let to = if skip && i > s && self.c[i - 1] == ch {
                        i - 1
                    } else {
                        i
                    };
                    (s..to).rev().find(|&j| self.c[j] == ch)?
                }
            };
        }
        Some(match kind {
            't' => i - 1,
            'T' => i + 1,
            _ => i,
        })
    }

    /// `%`: the bracket matching the first one at or after `pos` on its line.
    pub fn match_pair(&self, pos: usize) -> Option<usize> {
        let e = self.line_end(pos);
        let start = (pos..e).find(|&i| "()[]{}".contains(self.c[i]))?;
        let open = self.c[start];
        let (close, fwd) = match open {
            '(' => (')', true),
            '[' => (']', true),
            '{' => ('}', true),
            ')' => ('(', false),
            ']' => ('[', false),
            _ => ('{', false),
        };
        let mut depth = 0usize;
        if fwd {
            for i in start..self.len() {
                if self.c[i] == open {
                    depth += 1;
                } else if self.c[i] == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        } else {
            for i in (0..=start).rev() {
                if self.c[i] == open {
                    depth += 1;
                } else if self.c[i] == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    /// `}` (forward) or `{`: the next blank line past the current paragraph.
    pub fn paragraph(&self, pos: usize, fwd: bool) -> usize {
        let last = self.lines() - 1;
        let mut l = self.line_of(pos);
        if fwd {
            while l < last && self.line_is_blank(l) {
                l += 1;
            }
            while l < last && !self.line_is_blank(l) {
                l += 1;
            }
            if l == last && !self.line_is_blank(l) {
                return self.len();
            }
        } else {
            while l > 0 && self.line_is_blank(l) {
                l -= 1;
            }
            while l > 0 && !self.line_is_blank(l) {
                l -= 1;
            }
        }
        self.start_of_line(l)
    }

    /// Text object `c` (`w W " ' ( b [ { B < p`) around `pos`.
    pub fn object(&self, pos: usize, c: char, around: bool) -> Option<(usize, usize, bool)> {
        match c {
            'w' => self.obj_word(pos, false, around),
            'W' => self.obj_word(pos, true, around),
            '"' | '\'' | '`' => self.obj_quote(pos, c, around),
            '(' | ')' | 'b' => self.obj_paren(pos, '(', ')', around),
            '[' | ']' => self.obj_paren(pos, '[', ']', around),
            '{' | '}' | 'B' => self.obj_paren(pos, '{', '}', around),
            '<' | '>' => self.obj_paren(pos, '<', '>', around),
            'p' => self.obj_paragraph(pos, around),
            _ => None,
        }
    }

    // ---- Text objects ----
    //
    // Charwise objects return (start, end exclusive, false); linewise ones
    // return (a position on the first line, one on the last line, true).

    pub fn obj_word(&self, pos: usize, big: bool, around: bool) -> Option<(usize, usize, bool)> {
        if pos >= self.len() || self.c[pos] == '\n' {
            return None;
        }
        let s = self.line_start(pos);
        let e = self.line_end(pos);
        let cls = class(self.c[pos], big);
        let mut a = pos;
        while a > s && class(self.c[a - 1], big) == cls {
            a -= 1;
        }
        let mut b = pos + 1;
        while b < e && class(self.c[b], big) == cls {
            b += 1;
        }
        if around {
            let b2 = (b..e).find(|&i| !is_blank(self.c[i])).unwrap_or(e);
            if b2 > b {
                b = b2;
            } else {
                while a > s && is_blank(self.c[a - 1]) {
                    a -= 1;
                }
            }
        }
        Some((a, b, false))
    }

    pub fn obj_quote(&self, pos: usize, q: char, around: bool) -> Option<(usize, usize, bool)> {
        let s = self.line_start(pos);
        let e = self.line_end(pos);
        let quotes: Vec<usize> = (s..e).filter(|&i| self.c[i] == q).collect();
        let pair = quotes
            .chunks_exact(2)
            .find(|p| pos <= p[1])
            .map(|p| (p[0], p[1]))?;
        let (open, close) = pair;
        if around {
            let mut b = close + 1;
            while b < e && is_blank(self.c[b]) {
                b += 1;
            }
            Some((open, b, false))
        } else {
            Some((open + 1, close, false))
        }
    }

    pub fn obj_paren(
        &self,
        pos: usize,
        open: char,
        close: char,
        around: bool,
    ) -> Option<(usize, usize, bool)> {
        let mut depth = 0usize;
        let mut o = None;
        let start = if pos < self.len() && self.c[pos] == close {
            pos.checked_sub(1)?
        } else {
            pos
        };
        for i in (0..=start.min(self.len().saturating_sub(1))).rev() {
            if self.c[i] == close && i != pos {
                depth += 1;
            } else if self.c[i] == open {
                if depth == 0 {
                    o = Some(i);
                    break;
                }
                depth -= 1;
            }
        }
        let o = o?;
        let mut depth = 0usize;
        let c = (o + 1..self.len()).find(|&i| {
            if self.c[i] == open {
                depth += 1;
            } else if self.c[i] == close {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            false
        })?;
        Some(if around {
            (o, c + 1, false)
        } else {
            (o + 1, c, false)
        })
    }

    pub fn obj_paragraph(&self, pos: usize, around: bool) -> Option<(usize, usize, bool)> {
        let last = self.lines() - 1;
        let l = self.line_of(pos);
        let blank = self.line_is_blank(l);
        let mut a = l;
        while a > 0 && self.line_is_blank(a - 1) == blank {
            a -= 1;
        }
        let mut b = l;
        while b < last && self.line_is_blank(b + 1) == blank {
            b += 1;
        }
        if around {
            while b < last && self.line_is_blank(b + 1) != blank {
                b += 1;
            }
        }
        Some((self.start_of_line(a), self.start_of_line(b), true))
    }
}

/// Indentation for a new line after `line`: its leading whitespace, or for
/// a `label:` line, the column where its instruction starts (8 if bare).
pub fn auto_indent(line: &str) -> String {
    let lead: String = line.chars().take_while(|&c| is_blank(c)).collect();
    if !lead.is_empty() || line.trim().is_empty() {
        return lead;
    }
    let label_end = line
        .char_indices()
        .find(|&(_, c)| !(is_keyword(c) || c == '?'))
        .map_or(line.len(), |(i, _)| i);
    if label_end > 0 && line[label_end..].starts_with(':') {
        let rest = &line[label_end + 1..];
        if rest.trim().is_empty() || rest.trim_start().starts_with(';') {
            return " ".repeat(8);
        }
        let pad = rest.chars().take_while(|&c| is_blank(c)).count();
        return " ".repeat(line[..label_end].chars().count() + 1 + pad);
    }
    String::new()
}
