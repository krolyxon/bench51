#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Num(i64),
    Str(Vec<u8>),
    Punct(char),
    /// `<<` and `>>`
    Shl,
    Shr,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub col: usize,
}

impl Token {
    pub fn ident(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_punct(&self, c: char) -> bool {
        self.tok == Tok::Punct(c)
    }
}

/// Splits one source line into tokens, stopping at a `;` comment.
/// Identifiers are upper-cased (the assembler is case-insensitive).
pub fn lex(line: &str) -> Result<Vec<Token>, (usize, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let col = i;
        if c.is_whitespace() {
            i += 1;
        } else if c == ';' {
            break;
        } else if c.is_ascii_alphabetic() || c == '_' || c == '?' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '?')
            {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push(Token {
                tok: Tok::Ident(s.to_ascii_uppercase()),
                col,
            });
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let n = parse_number(&s).ok_or((col, format!("invalid number '{s}'")))?;
            out.push(Token {
                tok: Tok::Num(n),
                col,
            });
        } else if c == '\'' || c == '"' {
            i += 1;
            let mut bytes = Vec::new();
            loop {
                match chars.get(i) {
                    None => return Err((col, "unterminated string".into())),
                    // A doubled quote inside a string stands for the quote itself.
                    Some(&q) if q == c && chars.get(i + 1) == Some(&c) => {
                        bytes.push(c as u8);
                        i += 2;
                    }
                    Some(&q) if q == c => {
                        i += 1;
                        break;
                    }
                    Some(&ch) => {
                        let mut buf = [0u8; 4];
                        bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        i += 1;
                    }
                }
            }
            out.push(Token {
                tok: Tok::Str(bytes),
                col,
            });
        } else if c == '<' && chars.get(i + 1) == Some(&'<') {
            out.push(Token { tok: Tok::Shl, col });
            i += 2;
        } else if c == '>' && chars.get(i + 1) == Some(&'>') {
            out.push(Token { tok: Tok::Shr, col });
            i += 2;
        } else if "+-*/%()#@.,:$~&|^".contains(c) {
            out.push(Token {
                tok: Tok::Punct(c),
                col,
            });
            i += 1;
        } else {
            return Err((col, format!("unexpected character '{c}'")));
        }
    }
    Ok(out)
}

/// Number literals: `123`, `123d`, `0x1F`, `1Fh`, `0b101`, `101b`, `17o`, `17q`.
pub fn parse_number(s: &str) -> Option<i64> {
    let l = s.to_ascii_lowercase();
    let (digits, radix) = if let Some(r) = l.strip_prefix("0x") {
        (r, 16)
    } else if let Some(r) = l.strip_suffix('h') {
        (r, 16)
    } else if let Some(r) = l
        .strip_prefix("0b")
        .filter(|r| !r.is_empty() && r.chars().all(|c| c == '0' || c == '1'))
    {
        (r, 2)
    } else if let Some(r) = l
        .strip_suffix('b')
        .filter(|r| r.chars().all(|c| c == '0' || c == '1'))
    {
        (r, 2)
    } else if let Some(r) = l.strip_suffix('o').or_else(|| l.strip_suffix('q')) {
        (r, 8)
    } else if let Some(r) = l.strip_suffix('d') {
        (r, 10)
    } else {
        (l.as_str(), 10)
    };
    if digits.is_empty() {
        return None;
    }
    i64::from_str_radix(digits, radix).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("0FFh"), Some(255));
        assert_eq!(parse_number("0bh"), Some(11));
        assert_eq!(parse_number("0b101"), Some(5));
        assert_eq!(parse_number("1010b"), Some(10));
        assert_eq!(parse_number("0x1f"), Some(31));
        assert_eq!(parse_number("17o"), Some(15));
        assert_eq!(parse_number("42"), Some(42));
        assert_eq!(parse_number("12x"), None);
    }

    #[test]
    fn lexes_line() {
        let t = lex("loop: mov a, #'x' ; comment").unwrap();
        let toks: Vec<_> = t.into_iter().map(|t| t.tok).collect();
        assert_eq!(
            toks,
            vec![
                Tok::Ident("LOOP".into()),
                Tok::Punct(':'),
                Tok::Ident("MOV".into()),
                Tok::Ident("A".into()),
                Tok::Punct(','),
                Tok::Punct('#'),
                Tok::Str(vec![b'x']),
            ]
        );
    }
}
