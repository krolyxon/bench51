//! Keys as the Vim engine sees them, and their conversion from egui events.

use eframe::egui::{Event, Key};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VimKey {
    Char(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// Ctrl plus a lowercase letter.
    Ctrl(char),
    /// Ctrl+V: the system clipboard's text.
    Paste(String),
    /// Ctrl+C.
    Copy,
    /// Ctrl+X.
    Cut,
}

impl VimKey {
    /// How the key is shown in the mode line while a command is pending.
    pub fn label(&self) -> String {
        match self {
            VimKey::Char(c) => c.to_string(),
            VimKey::Ctrl(c) => format!("^{}", c.to_ascii_uppercase()),
            VimKey::Esc => "<Esc>".into(),
            VimKey::Enter => "<CR>".into(),
            other => format!("<{other:?}>"),
        }
    }
}

/// The keys in a keyboard event; text events can carry several chars.
pub fn from_egui(event: &Event) -> Vec<VimKey> {
    match event {
        Event::Text(t) => t.chars().map(VimKey::Char).collect(),
        Event::Paste(t) => vec![VimKey::Paste(t.clone())],
        Event::Copy => vec![VimKey::Copy],
        Event::Cut => vec![VimKey::Cut],
        Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => {
            if modifiers.ctrl && !modifiers.alt {
                let name = key.name();
                if *key == Key::OpenBracket {
                    return vec![VimKey::Esc];
                }
                if name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic() {
                    return vec![VimKey::Ctrl(
                        name.to_ascii_lowercase().chars().next().unwrap(),
                    )];
                }
                return Vec::new();
            }
            let k = match key {
                Key::Escape => VimKey::Esc,
                Key::Enter => VimKey::Enter,
                Key::Backspace => VimKey::Backspace,
                Key::Delete => VimKey::Delete,
                Key::Tab if !modifiers.shift => VimKey::Tab,
                Key::ArrowUp => VimKey::Up,
                Key::ArrowDown => VimKey::Down,
                Key::ArrowLeft => VimKey::Left,
                Key::ArrowRight => VimKey::Right,
                Key::Home => VimKey::Home,
                Key::End => VimKey::End,
                Key::PageUp => VimKey::PageUp,
                Key::PageDown => VimKey::PageDown,
                // Printable keys arrive as `Event::Text`.
                _ => return Vec::new(),
            };
            vec![k]
        }
        _ => Vec::new(),
    }
}

/// Keys from a string, for tests and replays: `<Esc>`, `<CR>`, `<BS>`,
/// `<C-r>` style names, everything else literally.
#[cfg(test)]
pub fn parse_keys(s: &str) -> Vec<VimKey> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            let name = &rest[1..end];
            let k = match name {
                "Esc" => Some(VimKey::Esc),
                "CR" => Some(VimKey::Enter),
                "BS" => Some(VimKey::Backspace),
                "Del" => Some(VimKey::Delete),
                "Tab" => Some(VimKey::Tab),
                "Up" => Some(VimKey::Up),
                "Down" => Some(VimKey::Down),
                "Left" => Some(VimKey::Left),
                "Right" => Some(VimKey::Right),
                _ if name.starts_with("C-") && name.len() == 3 => {
                    Some(VimKey::Ctrl(name.chars().nth(2).unwrap()))
                }
                _ => None,
            };
            if let Some(k) = k {
                out.push(k);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push(VimKey::Char(c));
        rest = &rest[c.len_utf8()..];
    }
    out
}
