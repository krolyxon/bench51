//! `:` commands.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ex {
    Write(Option<String>),
    Quit {
        force: bool,
    },
    WriteQuit,
    /// `:x`: write if changed, then quit.
    Exit,
    QuitAll {
        force: bool,
    },
    Edit(String),
    /// 1-based line number.
    Line(usize),
    LastLine,
    NoHighlight,
    Make,
    Next,
    Prev,
    Unknown(String),
}

pub fn parse(input: &str) -> Ex {
    let s = input.trim();
    if let Ok(n) = s.parse::<usize>() {
        return Ex::Line(n);
    }
    if s == "$" {
        return Ex::LastLine;
    }
    let (cmd, arg) = match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    };
    let (cmd, bang) = match cmd.strip_suffix('!') {
        Some(c) => (c, true),
        None => (cmd, false),
    };
    let arg = (!arg.is_empty()).then(|| arg.to_string());
    match cmd {
        "w" | "write" => Ex::Write(arg),
        "q" | "quit" | "clo" | "close" => Ex::Quit { force: bang },
        "wq" => Ex::WriteQuit,
        "x" | "xit" | "exi" | "exit" => Ex::Exit,
        "qa" | "qall" | "quita" | "quitall" => Ex::QuitAll { force: bang },
        "e" | "edit" => match arg {
            Some(a) => Ex::Edit(a),
            None => Ex::Unknown(s.to_string()),
        },
        "noh" | "nohl" | "nohlsearch" => Ex::NoHighlight,
        "mak" | "make" => Ex::Make,
        "bn" | "bnext" | "tabn" | "tabnext" => Ex::Next,
        "bp" | "bprevious" | "bN" | "bNext" | "tabp" | "tabprevious" | "tabN" => Ex::Prev,
        _ => Ex::Unknown(s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Ex, parse};

    #[test]
    fn parses_common_commands() {
        assert_eq!(parse("w"), Ex::Write(None));
        assert_eq!(parse("w  out.asm "), Ex::Write(Some("out.asm".into())));
        assert_eq!(parse("q!"), Ex::Quit { force: true });
        assert_eq!(parse("wq"), Ex::WriteQuit);
        assert_eq!(parse("x"), Ex::Exit);
        assert_eq!(parse("qa!"), Ex::QuitAll { force: true });
        assert_eq!(parse("42"), Ex::Line(42));
        assert_eq!(parse("$"), Ex::LastLine);
        assert_eq!(
            parse("e examples/blink.asm"),
            Ex::Edit("examples/blink.asm".into())
        );
        assert_eq!(parse("noh"), Ex::NoHighlight);
        assert_eq!(parse("make"), Ex::Make);
        assert_eq!(parse("bn"), Ex::Next);
        assert_eq!(parse("frobnicate"), Ex::Unknown("frobnicate".into()));
    }
}
