//! Vim keybindings for the code editor: modes, motions with counts,
//! operators and text objects, registers, undo, `.` repeat, macros, search
//! and `:` commands. The engine edits a `String` through a char-index
//! cursor and knows nothing about egui, so it is tested on plain text.

mod ex;
mod keys;
#[cfg(test)]
mod tests;
mod text;

use std::collections::HashMap;

pub use keys::{VimKey, from_egui};
use text::{Buf, Kind, auto_indent, is_blank, is_keyword};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    /// Typing a `:` command or a `/` `?` search.
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Top,
    Center,
    Bottom,
}

/// What the editor or the app has to do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Yanked or deleted text, mirrored to the system clipboard.
    Clipboard(String),
    Save,
    SaveAs(String),
    Close {
        force: bool,
    },
    Quit {
        force: bool,
    },
    Edit(String),
    Make,
    NextDoc,
    PrevDoc,
    /// Scroll so the cursor line is at the top, centre or bottom.
    Scroll(Align),
    /// Scroll the view by this many lines (Ctrl-D/U/F/B).
    ScrollLines(isize),
}

/// The lines currently visible, for `H M L` and paging.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub top: usize,
    pub rows: usize,
}

impl Default for View {
    fn default() -> Self {
        Self { top: 0, rows: 30 }
    }
}

/// Undo and redo snapshots of one document.
#[derive(Default)]
pub struct History {
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
}

const HISTORY_LIMIT: usize = 1000;

/// The document a key acts on.
pub struct Cx<'a> {
    pub text: &'a mut String,
    /// Char index into `text`.
    pub cursor: &'a mut usize,
    pub history: &'a mut History,
    pub readonly: bool,
    /// Unsaved changes, for `:q`.
    pub dirty: bool,
    pub view: View,
}

#[derive(Clone, Debug)]
struct Reg {
    text: String,
    linewise: bool,
}

/// The last search. It stays highlighted until `:noh`.
#[derive(Clone, Debug)]
pub struct Search {
    pub pattern: String,
    forward: bool,
    whole_word: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
    Yank,
    Indent,
    Dedent,
    Case,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    /// `+` / Enter and `-`: first non-blank of the next or previous line.
    NextLine,
    PrevLine,
    WordFwd(bool),
    WordBack(bool),
    WordEnd(bool),
    LineStart,
    FirstNonBlank,
    LineEnd,
    FileStart,
    FileEnd,
    Find(char, char),
    RepeatFind(bool),
    MatchPair,
    ParaFwd,
    ParaBack,
    ScreenTop,
    ScreenMid,
    ScreenBottom,
    HalfDown,
    HalfUp,
    PageDown,
    PageUp,
    SearchNext(bool),
    SearchWord(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Motion(Motion),
    /// Text object: its character and whether it's an "a" object.
    Obj(char, bool),
    /// `dd`, `yy`, `cc`, `>>`: whole lines.
    Line,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Cmd {
    Move(Motion, Option<usize>),
    Op {
        op: Op,
        reg: Option<char>,
        count: Option<usize>,
        target: Target,
    },
    VisualOp(Op, Option<char>),
    /// `viw`, `vi(`…: select a text object.
    VisualObj(char, bool),
    VisualJoin,
    VisualPaste(Option<char>),
    SwapAnchor,
    /// `d/pattern`, `c?pattern`: an operator over a search.
    OpSearch {
        op: Op,
        reg: Option<char>,
        forward: bool,
    },
    Insert(char, usize),
    Paste {
        before: bool,
        reg: Option<char>,
        count: usize,
    },
    Undo(usize),
    Redo(usize),
    Repeat(usize),
    Join(usize),
    ToggleCase(usize),
    Replace(char, usize),
    Visual(bool),
    Prompt(char),
    MacroRecord(char),
    MacroStop,
    MacroPlay(Option<char>, usize),
    Scroll(Align),
    GotoDef,
    NextTab,
    PrevTab,
}

impl Cmd {
    /// Changes the text (so it's refused when read-only and undoable).
    fn modifies(&self) -> bool {
        match self {
            Cmd::Op { op, .. } | Cmd::VisualOp(op, _) => *op != Op::Yank,
            Cmd::VisualJoin
            | Cmd::VisualPaste(_)
            | Cmd::Insert(..)
            | Cmd::Paste { .. }
            | Cmd::Undo(_)
            | Cmd::Redo(_)
            | Cmd::Repeat(_)
            | Cmd::Join(_)
            | Cmd::ToggleCase(_)
            | Cmd::Replace(..) => true,
            _ => false,
        }
    }

    /// Recorded for `.`.
    fn repeatable(&self) -> bool {
        self.modifies() && !matches!(self, Cmd::Undo(_) | Cmd::Redo(_) | Cmd::Repeat(_))
    }

    fn enters_insert(&self) -> bool {
        matches!(self, Cmd::Insert(..) | Cmd::Op { op: Op::Change, .. })
    }
}

enum Parse {
    More,
    Bad,
    Done(Cmd),
}

/// A charwise range is `[start, end)`; a linewise one covers whole lines,
/// including the last line's `\n` when there is one.
#[derive(Clone, Copy, Debug)]
struct Range {
    start: usize,
    end: usize,
    linewise: bool,
}

fn lines_range(buf: &Buf, l0: usize, l1: usize) -> Range {
    let start = buf.start_of_line(l0);
    let end = if l1 + 1 < buf.lines() {
        buf.start_of_line(l1 + 1)
    } else {
        buf.len()
    };
    Range {
        start,
        end,
        linewise: true,
    }
}

fn is_register(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '"' || c == '_'
}

fn count_at(keys: &[VimKey], mut i: usize) -> (Option<usize>, usize) {
    let mut n: Option<usize> = None;
    while let Some(VimKey::Char(c)) = keys.get(i) {
        let Some(d) = c.to_digit(10) else { break };
        if d == 0 && n.is_none() {
            break; // `0` is a motion
        }
        n = Some((n.unwrap_or(0) * 10 + d as usize).min(99_999));
        i += 1;
    }
    (n, i)
}

/// A motion starting at `keys[i]`, and the index after it.
fn parse_motion(keys: &[VimKey], i: usize) -> Result<Option<(Motion, usize)>, ()> {
    use Motion::*;
    let Some(k) = keys.get(i) else {
        return Ok(None);
    };
    let single = |m| Ok(Some((m, i + 1)));
    match k {
        VimKey::Left | VimKey::Backspace => single(Left),
        VimKey::Right => single(Right),
        VimKey::Up => single(Up),
        VimKey::Down => single(Down),
        VimKey::Enter => single(NextLine),
        VimKey::Home => single(LineStart),
        VimKey::End => single(LineEnd),
        VimKey::PageUp => single(PageUp),
        VimKey::PageDown => single(PageDown),
        VimKey::Ctrl('d') => single(HalfDown),
        VimKey::Ctrl('u') => single(HalfUp),
        VimKey::Ctrl('f') => single(PageDown),
        VimKey::Ctrl('b') => single(PageUp),
        VimKey::Ctrl('n') | VimKey::Ctrl('j') => single(Down),
        VimKey::Ctrl('p') => single(Up),
        VimKey::Char(c) => match c {
            'h' => single(Left),
            'l' | ' ' => single(Right),
            'j' => single(Down),
            'k' => single(Up),
            '+' => single(NextLine),
            '-' => single(PrevLine),
            'w' => single(WordFwd(false)),
            'W' => single(WordFwd(true)),
            'b' => single(WordBack(false)),
            'B' => single(WordBack(true)),
            'e' => single(WordEnd(false)),
            'E' => single(WordEnd(true)),
            '0' => single(LineStart),
            '^' => single(FirstNonBlank),
            '$' => single(LineEnd),
            'G' => single(FileEnd),
            '%' => single(MatchPair),
            '}' => single(ParaFwd),
            '{' => single(ParaBack),
            'H' => single(ScreenTop),
            'M' => single(ScreenMid),
            'L' => single(ScreenBottom),
            'n' => single(SearchNext(false)),
            'N' => single(SearchNext(true)),
            '*' => single(SearchWord(true)),
            '#' => single(SearchWord(false)),
            ';' => single(RepeatFind(false)),
            ',' => single(RepeatFind(true)),
            'g' => match keys.get(i + 1) {
                None => Ok(None),
                Some(VimKey::Char('g')) => Ok(Some((FileStart, i + 2))),
                _ => Err(()),
            },
            'f' | 'F' | 't' | 'T' => match keys.get(i + 1) {
                None => Ok(None),
                Some(VimKey::Char(ch)) => Ok(Some((Find(*c, *ch), i + 2))),
                _ => Err(()),
            },
            _ => Err(()),
        },
        _ => Err(()),
    }
}

fn parse(keys: &[VimKey], visual: bool, recording: bool) -> Parse {
    use VimKey::Char;
    let mut i = 0;
    let mut reg = None;
    if keys.first() == Some(&Char('"')) {
        match keys.get(1) {
            None => return Parse::More,
            Some(Char(c)) if is_register(*c) => {
                reg = Some(*c);
                i = 2;
            }
            _ => return Parse::Bad,
        }
    }
    let (count, j) = count_at(keys, i);
    i = j;
    let Some(k) = keys.get(i) else {
        return Parse::More;
    };
    let n = count.unwrap_or(1);
    let op = |op, target| {
        Parse::Done(Cmd::Op {
            op,
            reg,
            count,
            target,
        })
    };
    let next_char = |f: &dyn Fn(char) -> Cmd| match keys.get(i + 1) {
        None => Parse::More,
        Some(Char(c)) => Parse::Done(f(*c)),
        _ => Parse::Bad,
    };

    if visual {
        let done = |cmd| Parse::Done(cmd);
        match k {
            Char('d' | 'x' | 'X' | 'D') | VimKey::Delete => {
                return done(Cmd::VisualOp(Op::Delete, reg));
            }
            Char('c' | 's' | 'C' | 'S' | 'R') => return done(Cmd::VisualOp(Op::Change, reg)),
            Char('y' | 'Y') => return done(Cmd::VisualOp(Op::Yank, reg)),
            Char('>') => return done(Cmd::VisualOp(Op::Indent, reg)),
            Char('<') => return done(Cmd::VisualOp(Op::Dedent, reg)),
            Char('~' | 'u' | 'U') => return done(Cmd::VisualOp(Op::Case, reg)),
            Char('J') => return done(Cmd::VisualJoin),
            Char('p' | 'P') => return done(Cmd::VisualPaste(reg)),
            Char('o' | 'O') => return done(Cmd::SwapAnchor),
            Char('v') => return done(Cmd::Visual(false)),
            Char('V') => return done(Cmd::Visual(true)),
            Char(':') => return done(Cmd::Prompt(':')),
            Char(ia @ ('i' | 'a')) => {
                return match keys.get(i + 1) {
                    None => Parse::More,
                    Some(Char(obj)) => done(Cmd::VisualObj(*obj, *ia == 'a')),
                    _ => Parse::Bad,
                };
            }
            _ => {}
        }
    }

    match k {
        Char(o @ ('d' | 'c' | 'y' | '>' | '<')) => {
            let o = *o;
            let kind = match o {
                'd' => Op::Delete,
                'c' => Op::Change,
                'y' => Op::Yank,
                '>' => Op::Indent,
                _ => Op::Dedent,
            };
            let (count2, j) = count_at(keys, i + 1);
            let count = match (count, count2) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
            };
            let op = |target| {
                Parse::Done(Cmd::Op {
                    op: kind,
                    reg,
                    count,
                    target,
                })
            };
            match keys.get(j) {
                None => Parse::More,
                Some(Char(c)) if *c == o => op(Target::Line),
                Some(Char(p @ ('/' | '?'))) => Parse::Done(Cmd::OpSearch {
                    op: kind,
                    reg,
                    forward: *p == '/',
                }),
                Some(Char(ia @ ('i' | 'a'))) => match keys.get(j + 1) {
                    None => Parse::More,
                    Some(Char(obj)) if "wWsp\"'`()b[]{}B<>".contains(*obj) => {
                        op(Target::Obj(*obj, *ia == 'a'))
                    }
                    _ => Parse::Bad,
                },
                _ => match parse_motion(keys, j) {
                    Ok(None) => Parse::More,
                    Err(()) => Parse::Bad,
                    Ok(Some((m, _))) => op(Target::Motion(m)),
                },
            }
        }
        Char('x') | VimKey::Delete => op(Op::Delete, Target::Motion(Motion::Right)),
        Char('X') => op(Op::Delete, Target::Motion(Motion::Left)),
        Char('D') => op(Op::Delete, Target::Motion(Motion::LineEnd)),
        Char('C') => op(Op::Change, Target::Motion(Motion::LineEnd)),
        Char('s') => op(Op::Change, Target::Motion(Motion::Right)),
        Char('S') => op(Op::Change, Target::Line),
        Char('Y') => op(Op::Yank, Target::Line),
        Char(c @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')) => Parse::Done(Cmd::Insert(*c, n)),
        Char(c @ ('p' | 'P')) => Parse::Done(Cmd::Paste {
            before: *c == 'P',
            reg,
            count: n,
        }),
        Char('u') => Parse::Done(Cmd::Undo(n)),
        VimKey::Ctrl('r') => Parse::Done(Cmd::Redo(n)),
        Char('.') => Parse::Done(Cmd::Repeat(n)),
        Char('J') => Parse::Done(Cmd::Join(n)),
        Char('~') => Parse::Done(Cmd::ToggleCase(n)),
        Char('r') => next_char(&|c| Cmd::Replace(c, n)),
        Char('v') => Parse::Done(Cmd::Visual(false)),
        Char('V') => Parse::Done(Cmd::Visual(true)),
        Char(c @ (':' | '/' | '?')) => Parse::Done(Cmd::Prompt(*c)),
        Char('q') if recording => Parse::Done(Cmd::MacroStop),
        Char('q') => match keys.get(i + 1) {
            None => Parse::More,
            Some(Char(c)) if c.is_ascii_lowercase() => Parse::Done(Cmd::MacroRecord(*c)),
            _ => Parse::Bad,
        },
        Char('@') => match keys.get(i + 1) {
            None => Parse::More,
            Some(Char('@')) => Parse::Done(Cmd::MacroPlay(None, n)),
            Some(Char(c)) if c.is_ascii_lowercase() => Parse::Done(Cmd::MacroPlay(Some(*c), n)),
            _ => Parse::Bad,
        },
        Char('z') => match keys.get(i + 1) {
            None => Parse::More,
            Some(Char('z' | '.')) => Parse::Done(Cmd::Scroll(Align::Center)),
            Some(Char('t') | VimKey::Enter) => Parse::Done(Cmd::Scroll(Align::Top)),
            Some(Char('b' | '-')) => Parse::Done(Cmd::Scroll(Align::Bottom)),
            _ => Parse::Bad,
        },
        Char('g') => match keys.get(i + 1) {
            None => Parse::More,
            Some(Char('d')) => Parse::Done(Cmd::GotoDef),
            Some(Char('t')) => Parse::Done(Cmd::NextTab),
            Some(Char('T')) => Parse::Done(Cmd::PrevTab),
            Some(Char('g')) => Parse::Done(Cmd::Move(Motion::FileStart, count)),
            _ => Parse::Bad,
        },
        _ => match parse_motion(keys, i) {
            Ok(None) => Parse::More,
            Err(()) => Parse::Bad,
            Ok(Some((m, _))) => Parse::Done(Cmd::Move(m, count)),
        },
    }
}

pub struct Vim {
    pub mode: Mode,
    pending: Vec<VimKey>,
    anchor: usize,
    /// Column `j`/`k` aim for; `usize::MAX` after `$`.
    want_col: Option<usize>,
    registers: HashMap<char, Reg>,
    last_find: Option<(char, char)>,
    pub search: Option<Search>,
    pub highlight: bool,
    /// The `:`, `/` or `?` prompt and what's been typed after it.
    pub cmdline: Option<(char, String)>,
    /// Status message, and whether it's an error.
    pub message: Option<(String, bool)>,
    /// Text and cursor before the change in progress, for undo.
    change_start: Option<(String, usize)>,
    /// Operator waiting for the `/` or `?` search that ends its range.
    search_op: Option<(Op, Option<char>)>,
    dot_rec: Option<Vec<VimKey>>,
    last_change: Vec<VimKey>,
    dot_replay: bool,
    macro_rec: Option<(char, Vec<VimKey>)>,
    macros: HashMap<char, Vec<VimKey>>,
    last_macro: Option<char>,
    macro_depth: u8,
    /// Count and command of the insert in progress (`3ix`, `2o`).
    insert_repeat: (usize, char),
    insert_keys: Vec<VimKey>,
}

impl Default for Vim {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            pending: Vec::new(),
            anchor: 0,
            want_col: None,
            registers: HashMap::new(),
            last_find: None,
            search: None,
            highlight: false,
            cmdline: None,
            message: None,
            change_start: None,
            search_op: None,
            dot_rec: None,
            last_change: Vec::new(),
            dot_replay: false,
            macro_rec: None,
            macros: HashMap::new(),
            last_macro: None,
            macro_depth: 0,
            insert_repeat: (1, 'i'),
            insert_keys: Vec::new(),
        }
    }
}

impl Vim {
    pub fn mode_label(&self) -> &'static str {
        match self.mode {
            Mode::Normal => "normal",
            Mode::Insert => "insert",
            Mode::Visual => "visual",
            Mode::VisualLine => "v-line",
            Mode::Command => "command",
        }
    }

    /// Keys of a command still being typed, e.g. `2d`.
    pub fn pending_label(&self) -> String {
        self.pending.iter().map(VimKey::label).collect()
    }

    pub fn recording(&self) -> Option<char> {
        self.macro_rec.as_ref().map(|r| r.0)
    }

    /// Normal, Visual and command-line modes show a block cursor.
    pub fn block_cursor(&self) -> bool {
        self.mode != Mode::Insert
    }

    /// Back to Normal mode without finishing anything, e.g. when another
    /// document becomes active.
    pub fn abandon(&mut self) {
        self.mode = Mode::Normal;
        self.pending.clear();
        self.cmdline = None;
        self.change_start = None;
        self.search_op = None;
        self.dot_rec = None;
        self.want_col = None;
    }

    /// The mouse put the cursor at `head`, selecting from `tail` if they
    /// differ (egui's `[min, max)`; Vim's selection includes the cursor).
    pub fn mouse(&mut self, tail: usize, head: usize, text: &str, cursor: &mut usize) {
        self.pending.clear();
        if self.mode == Mode::Command {
            self.cmdline = None;
            self.search_op = None;
            self.mode = Mode::Normal;
        }
        let buf = Buf::new(text);
        if tail != head && self.mode != Mode::Insert {
            let (a, b) = (tail.min(head), tail.max(head) - 1);
            if head > tail {
                self.anchor = a;
                *cursor = b;
            } else {
                self.anchor = b;
                *cursor = a;
            }
            if self.mode != Mode::VisualLine {
                self.mode = Mode::Visual;
            }
        } else {
            if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
                self.mode = Mode::Normal;
            }
            *cursor = if self.mode == Mode::Insert {
                head.min(buf.len())
            } else {
                buf.clamp_normal(head)
            };
        }
    }

    /// The Visual selection as a char range `[start, end)`.
    pub fn selection(&self, text: &str, cursor: usize) -> Option<(usize, usize)> {
        let (a, b) = (self.anchor.min(cursor), self.anchor.max(cursor));
        let len = text.chars().count();
        match self.mode {
            Mode::Visual => Some((a.min(len), (b + 1).min(len))),
            Mode::VisualLine => {
                let buf = Buf::new(text);
                Some((buf.line_start(a), buf.line_end(b)))
            }
            _ => None,
        }
    }

    /// Byte ranges of the highlighted search's matches in `text`.
    pub fn search_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        let Some(s) = self.search.as_ref().filter(|_| self.highlight) else {
            return Vec::new();
        };
        let buf = Buf::new(text);
        let plen = s.pattern.chars().count();
        if plen == 0 {
            return Vec::new();
        }
        let bytes: Vec<usize> = text
            .char_indices()
            .map(|(b, _)| b)
            .chain([text.len()])
            .collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i + plen <= buf.len() && out.len() < 2000 {
            if matches_at(&buf, i, s) {
                out.push((bytes[i], bytes[i + plen]));
                i += plen;
            } else {
                i += 1;
            }
        }
        out
    }

    /// Handles one key typed by the user.
    pub fn key(&mut self, key: VimKey, cx: &mut Cx) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.mode != Mode::Command {
            self.message = None;
        }
        if let Some((_, keys)) = &mut self.macro_rec {
            keys.push(key.clone());
        }
        self.handle(key, cx, &mut fx);
        fx
    }

    fn handle(&mut self, key: VimKey, cx: &mut Cx, fx: &mut Vec<Effect>) {
        match self.mode {
            Mode::Insert => self.insert_key(key, cx),
            Mode::Command => self.cmdline_key(key, cx, fx),
            _ => self.normal_key(key, cx, fx),
        }
        if matches!(self.mode, Mode::Normal | Mode::Visual | Mode::VisualLine) {
            let buf = Buf::new(cx.text);
            *cx.cursor = buf.clamp_normal(*cx.cursor);
        }
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), true));
    }

    // ---- Normal and Visual mode ----

    fn normal_key(&mut self, key: VimKey, cx: &mut Cx, fx: &mut Vec<Effect>) {
        let visual = matches!(self.mode, Mode::Visual | Mode::VisualLine);
        match &key {
            VimKey::Esc | VimKey::Ctrl('c') => {
                self.pending.clear();
                if visual {
                    self.mode = Mode::Normal;
                }
                return;
            }
            VimKey::Copy => {
                self.pending.clear();
                if visual {
                    self.exec(Cmd::VisualOp(Op::Yank, None), cx, fx);
                }
                return;
            }
            VimKey::Cut => {
                self.pending.clear();
                if visual {
                    self.exec(Cmd::VisualOp(Op::Delete, None), cx, fx);
                }
                return;
            }
            VimKey::Paste(t) => {
                self.pending.clear();
                let t = t.clone();
                self.paste_text(&t, visual, cx);
                return;
            }
            _ => {}
        }
        self.pending.push(key);
        match parse(&self.pending, visual, self.macro_rec.is_some()) {
            Parse::More => {}
            Parse::Bad => self.pending.clear(),
            Parse::Done(cmd) => {
                let keys = std::mem::take(&mut self.pending);
                if !visual && !self.dot_replay && cmd.repeatable() && !cx.readonly {
                    if cmd.enters_insert() {
                        self.dot_rec = Some(keys);
                    } else {
                        self.last_change = keys;
                    }
                }
                self.exec(cmd, cx, fx);
            }
        }
    }

    fn begin_change(&mut self, cx: &Cx) {
        if self.change_start.is_none() {
            self.change_start = Some((cx.text.clone(), *cx.cursor));
        }
    }

    fn end_change(&mut self, cx: &mut Cx) {
        if let Some((text, cursor)) = self.change_start.take()
            && text != *cx.text
        {
            cx.history.undo.push((text, cursor));
            if cx.history.undo.len() > HISTORY_LIMIT {
                cx.history.undo.remove(0);
            }
            cx.history.redo.clear();
        }
    }

    fn exec(&mut self, cmd: Cmd, cx: &mut Cx, fx: &mut Vec<Effect>) {
        if cmd.modifies() && cx.readonly {
            self.error("read-only while debugging");
            if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
                self.mode = Mode::Normal;
            }
            return;
        }
        let tracked =
            cmd.modifies() && !matches!(cmd, Cmd::Undo(_) | Cmd::Redo(_) | Cmd::Repeat(_));
        if tracked {
            self.begin_change(cx);
        }
        let mut buf = Buf::new(cx.text);
        let pos = (*cx.cursor).min(buf.len());
        match cmd {
            Cmd::Move(m, count) => {
                if let Some((p, _)) = self.motion(m, count, &buf, pos, false, cx.view, fx) {
                    *cx.cursor = p;
                }
            }
            Cmd::Op {
                op,
                reg,
                count,
                target,
            } => {
                if let Some(r) = self.op_range(op, count, target, &buf, pos, cx.view, fx) {
                    self.apply(op, reg, r, &mut buf, cx, fx);
                }
            }
            Cmd::VisualOp(op, reg) => {
                let r = self.visual_range(&buf, pos, op);
                self.mode = Mode::Normal;
                self.apply(op, reg, r, &mut buf, cx, fx);
            }
            Cmd::VisualJoin => {
                let (a, b) = (self.anchor.min(pos), self.anchor.max(pos));
                let lines = buf.line_of(b) - buf.line_of(a) + 1;
                self.mode = Mode::Normal;
                *cx.cursor = a;
                self.join(lines.max(2), &mut buf, cx);
            }
            Cmd::VisualPaste(reg) => {
                if let Some(r) = self.get_reg(reg) {
                    let range = self.visual_range(&buf, pos, Op::Delete);
                    self.mode = Mode::Normal;
                    let mut t = r.text.clone();
                    if range.linewise {
                        // Whole lines in, whole lines out.
                        if !t.ends_with('\n') {
                            t.push('\n');
                        }
                        if !buf.c[range.start..range.end].ends_with(&['\n']) {
                            t.pop();
                        }
                    } else if r.linewise {
                        // Lines into part of a line go on lines of their own.
                        t.insert(0, '\n');
                    }
                    buf.remove(range.start, range.end);
                    buf.insert(range.start, &t);
                    *cx.cursor = range.start + usize::from(!range.linewise && r.linewise);
                    *cx.text = buf.text();
                }
            }
            Cmd::VisualObj(c, around) => {
                if let Some((a, b, linewise)) = buf.object(pos, c, around) {
                    self.anchor = a;
                    if linewise {
                        self.mode = Mode::VisualLine;
                        *cx.cursor = b;
                    } else {
                        *cx.cursor = b.saturating_sub(1).max(a);
                    }
                }
            }
            Cmd::OpSearch { op, reg, forward } => {
                if op != Op::Yank && cx.readonly {
                    self.error("read-only while debugging");
                } else {
                    self.search_op = Some((op, reg));
                    self.mode = Mode::Command;
                    self.cmdline = Some((if forward { '/' } else { '?' }, String::new()));
                }
            }
            Cmd::SwapAnchor => {
                std::mem::swap(&mut self.anchor, cx.cursor);
            }
            Cmd::Insert(kind, count) => self.start_insert(kind, count, &mut buf, pos, cx),
            Cmd::Paste { before, reg, count } => {
                if let Some(r) = self.get_reg(reg) {
                    self.put(&r, before, count, &mut buf, pos, cx);
                }
            }
            Cmd::Undo(n) => self.undo(n, cx, true),
            Cmd::Redo(n) => self.undo(n, cx, false),
            Cmd::Repeat(n) => {
                let keys = self.last_change.clone();
                if !keys.is_empty() {
                    self.dot_replay = true;
                    for _ in 0..n {
                        for k in &keys {
                            self.handle(k.clone(), cx, fx);
                        }
                    }
                    self.dot_replay = false;
                }
            }
            Cmd::Join(n) => self.join(n.max(2), &mut buf, cx),
            Cmd::ToggleCase(n) => {
                let e = buf.line_end(pos);
                let end = (pos + n).min(e);
                for i in pos..end {
                    buf.c[i] = toggle_case(buf.c[i]);
                }
                *cx.text = buf.text();
                *cx.cursor = end;
            }
            Cmd::Replace(c, n) => {
                let e = buf.line_end(pos);
                if pos + n <= e {
                    for i in pos..pos + n {
                        buf.c[i] = c;
                    }
                    *cx.text = buf.text();
                    *cx.cursor = pos + n - 1;
                }
            }
            Cmd::Visual(line) => {
                let want = if line { Mode::VisualLine } else { Mode::Visual };
                self.mode = match self.mode {
                    m if m == want => Mode::Normal,
                    Mode::Normal => {
                        self.anchor = pos;
                        want
                    }
                    _ => want,
                };
            }
            Cmd::Prompt(p) => {
                self.mode = Mode::Command;
                self.cmdline = Some((p, String::new()));
            }
            Cmd::MacroRecord(c) => self.macro_rec = Some((c, Vec::new())),
            Cmd::MacroStop => {
                if let Some((c, mut keys)) = self.macro_rec.take() {
                    keys.pop(); // the `q` that stopped it
                    self.macros.insert(c, keys);
                }
            }
            Cmd::MacroPlay(reg, n) => {
                let Some(r) = reg.or(self.last_macro) else {
                    return;
                };
                self.last_macro = Some(r);
                let Some(keys) = self.macros.get(&r).cloned() else {
                    self.error(format!("register {r} is empty"));
                    return;
                };
                if self.macro_depth >= 8 {
                    return;
                }
                self.macro_depth += 1;
                for _ in 0..n {
                    for k in &keys {
                        self.handle(k.clone(), cx, fx);
                    }
                }
                self.macro_depth -= 1;
            }
            Cmd::Scroll(a) => fx.push(Effect::Scroll(a)),
            Cmd::GotoDef => match self.find_definition(&buf, pos) {
                Some(p) => *cx.cursor = p,
                None => self.error("definition not found"),
            },
            Cmd::NextTab => fx.push(Effect::NextDoc),
            Cmd::PrevTab => fx.push(Effect::PrevDoc),
        }
        if tracked && self.mode != Mode::Insert {
            self.end_change(cx);
        }
    }

    // ---- Motions ----

    #[allow(clippy::too_many_arguments)]
    fn motion(
        &mut self,
        m: Motion,
        count: Option<usize>,
        buf: &Buf,
        pos: usize,
        for_op: bool,
        view: View,
        fx: &mut Vec<Effect>,
    ) -> Option<(usize, Kind)> {
        use Kind::*;
        use Motion::*;
        let n = count.unwrap_or(1).max(1);
        let line = buf.line_of(pos);
        let last = buf.lines() - 1;
        let fnb = |l: usize| buf.first_non_blank(buf.start_of_line(l.min(last)));
        let r = match m {
            Left => (pos.saturating_sub(n).max(buf.line_start(pos)), Exclusive),
            Right => {
                let s = buf.line_start(pos);
                let e = buf.line_end(pos);
                let max = if for_op {
                    e
                } else {
                    e.saturating_sub(1).max(s)
                };
                ((pos + n).min(max), Exclusive)
            }
            Up | Down => {
                if (m == Up && line == 0) || (m == Down && line == last) {
                    return None;
                }
                let tl = if m == Up {
                    line.saturating_sub(n)
                } else {
                    (line + n).min(last)
                };
                let col = self.want_col.unwrap_or(buf.col(pos));
                self.want_col = Some(col);
                let target = if col == usize::MAX {
                    buf.clamp_normal(buf.line_end(buf.start_of_line(tl)))
                } else {
                    buf.at_col(tl, col)
                };
                return Some((target, Linewise));
            }
            NextLine => {
                if line == last {
                    return None;
                }
                (fnb(line + n), Linewise)
            }
            PrevLine => {
                if line == 0 {
                    return None;
                }
                (fnb(line.saturating_sub(n)), Linewise)
            }
            WordFwd(big) => ((0..n).fold(pos, |p, _| buf.word_fwd(p, big)), Exclusive),
            WordBack(big) => ((0..n).fold(pos, |p, _| buf.word_back(p, big)), Exclusive),
            WordEnd(big) => ((0..n).fold(pos, |p, _| buf.word_end(p, big)), Inclusive),
            LineStart => (buf.line_start(pos), Exclusive),
            FirstNonBlank => (buf.first_non_blank(pos), Exclusive),
            LineEnd => {
                let s = buf.start_of_line((line + n - 1).min(last));
                let e = buf.line_end(s);
                self.want_col = Some(usize::MAX);
                if e == s {
                    return Some((s, Exclusive));
                }
                return Some((e - 1, Inclusive));
            }
            FileStart => (fnb(count.map_or(0, |c| c - 1)), Linewise),
            FileEnd => (fnb(count.map_or(last, |c| c - 1)), Linewise),
            Find(kind, ch) => {
                self.last_find = Some((kind, ch));
                (buf.find_char(pos, kind, ch, n, false)?, find_kind(kind))
            }
            RepeatFind(rev) => {
                let (kind, ch) = self.last_find?;
                let kind = if rev {
                    match kind {
                        'f' => 'F',
                        'F' => 'f',
                        't' => 'T',
                        _ => 't',
                    }
                } else {
                    kind
                };
                (buf.find_char(pos, kind, ch, n, true)?, find_kind(kind))
            }
            MatchPair => (buf.match_pair(pos)?, Inclusive),
            ParaFwd => ((0..n).fold(pos, |p, _| buf.paragraph(p, true)), Exclusive),
            ParaBack => ((0..n).fold(pos, |p, _| buf.paragraph(p, false)), Exclusive),
            ScreenTop | ScreenMid | ScreenBottom => {
                let top = view.top.min(last);
                let bottom = (view.top + view.rows.max(1) - 1).min(last);
                let l = match m {
                    ScreenTop => (top + n - 1).min(bottom),
                    ScreenMid => (top + bottom) / 2,
                    _ => bottom.saturating_sub(n - 1).max(top),
                };
                (fnb(l), Linewise)
            }
            HalfDown | HalfUp | PageDown | PageUp => {
                let step = n * match m {
                    HalfDown | HalfUp => (view.rows / 2).max(1),
                    _ => view.rows.saturating_sub(2).max(1),
                };
                let down = matches!(m, HalfDown | PageDown);
                let tl = if down {
                    (line + step).min(last)
                } else {
                    line.saturating_sub(step)
                };
                if !for_op {
                    let delta = step as isize;
                    fx.push(Effect::ScrollLines(if down { delta } else { -delta }));
                }
                (fnb(tl), Linewise)
            }
            SearchNext(rev) => {
                let s = self.search.clone()?;
                self.highlight = true;
                let fwd = s.forward != rev;
                let mut p = pos;
                for _ in 0..n {
                    p = self.find_next(buf, p, fwd, &s)?;
                }
                (p, Exclusive)
            }
            SearchWord(fwd) => {
                let word = word_at(buf, pos)?;
                let s = Search {
                    pattern: word,
                    forward: fwd,
                    whole_word: true,
                };
                self.search = Some(s.clone());
                self.highlight = true;
                (self.find_next(buf, pos, fwd, &s)?, Exclusive)
            }
        };
        if !for_op || !matches!(m, Up | Down) {
            self.want_col = None;
        }
        Some(r)
    }

    fn find_next(&mut self, buf: &Buf, from: usize, fwd: bool, s: &Search) -> Option<usize> {
        let n = buf.len();
        if n == 0 {
            return None;
        }
        let order: Box<dyn Iterator<Item = usize>> = if fwd {
            Box::new((from + 1..n).chain(0..=from.min(n - 1)))
        } else {
            Box::new((0..from).rev().chain((from..n).rev()))
        };
        for i in order {
            if matches_at(buf, i, s) {
                if (fwd && i <= from) || (!fwd && i >= from) {
                    self.message = Some((
                        if fwd {
                            "search hit BOTTOM, continuing at TOP"
                        } else {
                            "search hit TOP, continuing at BOTTOM"
                        }
                        .into(),
                        false,
                    ));
                }
                return Some(i);
            }
        }
        self.error(format!("E486: Pattern not found: {}", s.pattern));
        None
    }

    fn find_definition(&self, buf: &Buf, pos: usize) -> Option<usize> {
        let word = word_at(buf, pos)?.to_ascii_uppercase();
        (0..buf.lines()).find_map(|l| {
            let s = buf.start_of_line(l);
            let line = buf.line_text(s);
            let t = line.trim_start();
            let rest = t.get(word.len()..)?;
            if !t[..word.len()].eq_ignore_ascii_case(&word) {
                return None;
            }
            let rest_up = rest.to_ascii_uppercase();
            let defines = rest.starts_with(':')
                || ["EQU", "SET", "BIT", "DATA", "IDATA", "XDATA", "CODE"]
                    .iter()
                    .any(|d| {
                        rest_up.trim_start().starts_with(d) && rest.starts_with(char::is_whitespace)
                    });
            defines.then(|| buf.first_non_blank(s))
        })
    }

    // ---- Operators ----

    #[allow(clippy::too_many_arguments)]
    fn op_range(
        &mut self,
        op: Op,
        count: Option<usize>,
        target: Target,
        buf: &Buf,
        pos: usize,
        view: View,
        fx: &mut Vec<Effect>,
    ) -> Option<Range> {
        let line = buf.line_of(pos);
        match target {
            Target::Line => {
                let l1 = (line + count.unwrap_or(1) - 1).min(buf.lines() - 1);
                Some(lines_range(buf, line, l1))
            }
            Target::Obj(c, around) => {
                let (a, b, linewise) = buf.object(pos, c, around)?;
                Some(if linewise {
                    lines_range(buf, buf.line_of(a), buf.line_of(b))
                } else {
                    Range {
                        start: a,
                        end: b,
                        linewise: false,
                    }
                })
            }
            Target::Motion(m) => {
                // `cw` changes to the end of the word, like `ce`.
                if op == Op::Change
                    && let Motion::WordFwd(big) = m
                    && pos < buf.len()
                    && !buf.c[pos].is_whitespace()
                {
                    let mut end = buf.run_end(pos, big);
                    for _ in 1..count.unwrap_or(1) {
                        end = buf.word_end(end, big);
                    }
                    return Some(Range {
                        start: pos,
                        end: end + 1,
                        linewise: false,
                    });
                }
                let (to, kind) = self.motion(m, count, buf, pos, true, view, fx)?;
                let (a, b) = (pos.min(to), pos.max(to));
                Some(match kind {
                    Kind::Linewise => lines_range(buf, buf.line_of(a), buf.line_of(b)),
                    Kind::Inclusive => Range {
                        start: a,
                        end: (b + 1).min(buf.len()),
                        linewise: false,
                    },
                    Kind::Exclusive => {
                        // An exclusive motion that ends at the start of a
                        // later line stops at the end of the line before
                        // (`dw` on a line's last word), or covers whole
                        // lines if it started at the indentation (`d}`).
                        if b > a && buf.col(b) == 0 && buf.line_of(b) > buf.line_of(a) {
                            if buf.col(a) <= buf.col(buf.first_non_blank(a)) {
                                return Some(lines_range(buf, buf.line_of(a), buf.line_of(b) - 1));
                            }
                            return Some(Range {
                                start: a,
                                end: b - 1,
                                linewise: false,
                            });
                        }
                        Range {
                            start: a,
                            end: b,
                            linewise: false,
                        }
                    }
                })
            }
        }
    }

    fn visual_range(&self, buf: &Buf, pos: usize, op: Op) -> Range {
        let (a, b) = (self.anchor.min(pos), self.anchor.max(pos));
        if self.mode == Mode::VisualLine || matches!(op, Op::Indent | Op::Dedent) {
            lines_range(buf, buf.line_of(a), buf.line_of(b))
        } else {
            Range {
                start: a,
                end: (b + 1).min(buf.len()),
                linewise: false,
            }
        }
    }

    fn apply(
        &mut self,
        op: Op,
        reg: Option<char>,
        r: Range,
        buf: &mut Buf,
        cx: &mut Cx,
        fx: &mut Vec<Effect>,
    ) {
        let pos = *cx.cursor;
        let mut text: String = buf.c[r.start..r.end].iter().collect();
        if r.linewise && !text.ends_with('\n') {
            text.push('\n');
        }
        match op {
            Op::Yank => {
                self.set_reg(reg, text, r.linewise, fx);
                *cx.cursor = if r.linewise {
                    if r.start < buf.line_start(pos) {
                        buf.at_col(buf.line_of(r.start), buf.col(pos))
                    } else {
                        pos
                    }
                } else {
                    r.start.min(pos)
                };
            }
            Op::Delete => {
                self.set_reg(reg, text, r.linewise, fx);
                let mut start = r.start;
                // Deleting the last lines also takes the `\n` before them.
                if r.linewise
                    && r.end == buf.len()
                    && start > 0
                    && !buf.c[r.start..r.end].ends_with(&['\n'])
                {
                    start -= 1;
                }
                buf.remove(start, r.end);
                *cx.text = buf.text();
                *cx.cursor = if r.linewise {
                    buf.first_non_blank(start.min(buf.len()))
                } else {
                    start
                };
            }
            Op::Change => {
                self.set_reg(reg, text, r.linewise, fx);
                if r.linewise {
                    let indent: String = buf.c[r.start..r.end]
                        .iter()
                        .take_while(|&&c| is_blank(c))
                        .collect();
                    let end = if buf.c[r.start..r.end].ends_with(&['\n']) {
                        r.end - 1
                    } else {
                        r.end
                    };
                    buf.remove(r.start, end);
                    let n = buf.insert(r.start, &indent);
                    *cx.cursor = r.start + n;
                } else {
                    buf.remove(r.start, r.end);
                    *cx.cursor = r.start;
                }
                *cx.text = buf.text();
                self.mode = Mode::Insert;
                self.insert_repeat = (1, 'c');
                self.insert_keys.clear();
            }
            Op::Indent | Op::Dedent => {
                let (l0, l1) = (
                    buf.line_of(r.start),
                    buf.line_of(r.end.saturating_sub(1).max(r.start)),
                );
                for l in l0..=l1 {
                    let s = buf.start_of_line(l);
                    let e = buf.line_end(s);
                    if op == Op::Indent {
                        if e > s {
                            buf.insert(s, "        ");
                        }
                    } else {
                        let n = if buf.c.get(s) == Some(&'\t') {
                            1
                        } else {
                            buf.c[s..e]
                                .iter()
                                .take(8)
                                .take_while(|&&c| c == ' ')
                                .count()
                        };
                        buf.remove(s, s + n);
                    }
                }
                *cx.text = buf.text();
                *cx.cursor = buf.first_non_blank(buf.start_of_line(l0));
            }
            Op::Case => {
                for i in r.start..r.end {
                    buf.c[i] = toggle_case(buf.c[i]);
                }
                *cx.text = buf.text();
                *cx.cursor = r.start;
            }
        }
    }

    fn set_reg(&mut self, reg: Option<char>, text: String, linewise: bool, fx: &mut Vec<Effect>) {
        match reg {
            Some('_') => return,
            Some(c) if c.is_ascii_lowercase() => {
                self.registers.insert(
                    c,
                    Reg {
                        text: text.clone(),
                        linewise,
                    },
                );
            }
            Some(c) if c.is_ascii_uppercase() => {
                let r = self.registers.entry(c.to_ascii_lowercase()).or_insert(Reg {
                    text: String::new(),
                    linewise,
                });
                r.text.push_str(&text);
            }
            _ => {}
        }
        fx.push(Effect::Clipboard(text.clone()));
        self.registers.insert('"', Reg { text, linewise });
    }

    fn get_reg(&mut self, reg: Option<char>) -> Option<Reg> {
        let c = reg.unwrap_or('"').to_ascii_lowercase();
        let r = self.registers.get(&c).cloned();
        if r.is_none() {
            self.error(format!("E353: Nothing in register {c}"));
        }
        r
    }

    fn put(&mut self, r: &Reg, before: bool, count: usize, buf: &mut Buf, pos: usize, cx: &mut Cx) {
        if r.linewise {
            let mut t = r.text.repeat(count);
            if !t.ends_with('\n') {
                t.push('\n');
            }
            let at = if before {
                buf.line_start(pos)
            } else {
                let e = buf.line_end(pos);
                if e == buf.len() {
                    // After the last line: the newline goes in front.
                    t.pop();
                    t.insert(0, '\n');
                    e
                } else {
                    e + 1
                }
            };
            buf.insert(at, &t);
            let first = if !before && at == buf.line_end(pos) && buf.c.get(at) == Some(&'\n') {
                at + 1
            } else {
                at
            };
            *cx.text = buf.text();
            *cx.cursor = buf.first_non_blank(first);
        } else {
            let t = r.text.repeat(count);
            let at = if before || buf.line_end(pos) == pos {
                pos
            } else {
                pos + 1
            };
            let n = buf.insert(at, &t);
            *cx.text = buf.text();
            *cx.cursor = at + n.saturating_sub(1);
        }
    }

    /// Ctrl+V: the system clipboard, before the cursor or over the selection.
    fn paste_text(&mut self, t: &str, visual: bool, cx: &mut Cx) {
        if cx.readonly {
            self.error("read-only while debugging");
            return;
        }
        let r = Reg {
            text: t.to_string(),
            linewise: t.ends_with('\n'),
        };
        self.begin_change(cx);
        let mut buf = Buf::new(cx.text);
        let pos = (*cx.cursor).min(buf.len());
        if visual {
            let range = self.visual_range(&buf, pos, Op::Delete);
            self.mode = Mode::Normal;
            buf.remove(range.start, range.end);
            let n = buf.insert(range.start, t);
            *cx.text = buf.text();
            *cx.cursor = range.start + n.saturating_sub(1);
        } else {
            self.put(&r, true, 1, &mut buf, pos, cx);
        }
        self.end_change(cx);
    }

    fn join(&mut self, lines: usize, buf: &mut Buf, cx: &mut Cx) {
        let mut pos = *cx.cursor;
        for _ in 1..lines {
            let e = buf.line_end(pos);
            if e >= buf.len() {
                break;
            }
            let mut f = e + 1;
            while f < buf.len() && is_blank(buf.c[f]) {
                f += 1;
            }
            buf.remove(e, f);
            let line_empty = e == buf.line_start(e);
            let ends_blank = e > 0 && is_blank(buf.c[e - 1]);
            let next_empty = e >= buf.len() || buf.c[e] == '\n' || buf.c[e] == ')';
            if !line_empty && !ends_blank && !next_empty {
                buf.insert(e, " ");
            }
            pos = e;
        }
        *cx.text = buf.text();
        *cx.cursor = pos;
    }

    fn undo(&mut self, n: usize, cx: &mut Cx, undo: bool) {
        for _ in 0..n {
            let (from, to) = if undo {
                (&mut cx.history.undo, &mut cx.history.redo)
            } else {
                (&mut cx.history.redo, &mut cx.history.undo)
            };
            let Some((text, cursor)) = from.pop() else {
                self.error(if undo {
                    "Already at oldest change"
                } else {
                    "Already at newest change"
                });
                return;
            };
            to.push((std::mem::replace(cx.text, text), *cx.cursor));
            *cx.cursor = cursor;
        }
    }

    // ---- Insert mode ----

    fn start_insert(&mut self, kind: char, count: usize, buf: &mut Buf, pos: usize, cx: &mut Cx) {
        let s = buf.line_start(pos);
        let e = buf.line_end(pos);
        *cx.cursor = match kind {
            'a' if e > pos => pos + 1,
            'I' => (s..e).find(|&i| !is_blank(buf.c[i])).unwrap_or(e),
            'A' => e,
            'o' | 'O' => {
                let indent = auto_indent(&buf.line_text(pos));
                let at = if kind == 'o' { e } else { s };
                let line = if kind == 'o' {
                    format!("\n{indent}")
                } else {
                    format!("{indent}\n")
                };
                buf.insert(at, &line);
                *cx.text = buf.text();
                at + indent.chars().count() + usize::from(kind == 'o')
            }
            _ => pos,
        };
        self.mode = Mode::Insert;
        self.insert_repeat = (count, kind);
        self.insert_keys.clear();
    }

    fn insert_key(&mut self, key: VimKey, cx: &mut Cx) {
        if let Some(rec) = &mut self.dot_rec
            && !self.dot_replay
        {
            rec.push(key.clone());
        }
        if cx.readonly || matches!(key, VimKey::Esc | VimKey::Copy | VimKey::Ctrl('c')) {
            self.leave_insert(cx);
            return;
        }
        if matches!(
            key,
            VimKey::Char(_) | VimKey::Enter | VimKey::Tab | VimKey::Backspace | VimKey::Paste(_)
        ) {
            self.insert_keys.push(key.clone());
        }
        self.type_key(&key, cx);
    }

    /// One key's effect on the text in Insert mode.
    fn type_key(&mut self, key: &VimKey, cx: &mut Cx) {
        let mut buf = Buf::new(cx.text);
        let pos = (*cx.cursor).min(buf.len());
        let s = buf.line_start(pos);
        let mut changed = true;
        match key {
            VimKey::Char(c) => *cx.cursor = pos + buf.insert(pos, &c.to_string()),
            VimKey::Paste(t) => *cx.cursor = pos + buf.insert(pos, t),
            VimKey::Enter => {
                let indent = auto_indent(&buf.c[s..pos].iter().collect::<String>());
                *cx.cursor = pos + buf.insert(pos, &format!("\n{indent}"));
            }
            VimKey::Tab => {
                let n = 8 - (pos - s) % 8;
                *cx.cursor = pos + buf.insert(pos, &" ".repeat(n));
            }
            VimKey::Backspace if pos > 0 => {
                buf.remove(pos - 1, pos);
                *cx.cursor = pos - 1;
            }
            VimKey::Delete if pos < buf.len() => {
                buf.remove(pos, pos + 1);
            }
            VimKey::Ctrl('w') if pos > s => {
                let mut a = pos;
                while a > s && is_blank(buf.c[a - 1]) {
                    a -= 1;
                }
                let kw = a > s && is_keyword(buf.c[a - 1]);
                while a > s && !is_blank(buf.c[a - 1]) && is_keyword(buf.c[a - 1]) == kw {
                    a -= 1;
                }
                buf.remove(a, pos);
                *cx.cursor = a;
            }
            VimKey::Ctrl('u') if pos > s => {
                buf.remove(s, pos);
                *cx.cursor = s;
            }
            other => {
                changed = false;
                let e = buf.line_end(pos);
                let line = buf.line_of(pos);
                *cx.cursor = match other {
                    VimKey::Left => pos.saturating_sub(1).max(s),
                    VimKey::Right => (pos + 1).min(e),
                    VimKey::Home => s,
                    VimKey::End => e,
                    VimKey::Up if line > 0 => {
                        let s2 = buf.start_of_line(line - 1);
                        (s2 + (pos - s)).min(buf.line_end(s2))
                    }
                    VimKey::Down if line + 1 < buf.lines() => {
                        let s2 = buf.start_of_line(line + 1);
                        (s2 + (pos - s)).min(buf.line_end(s2))
                    }
                    _ => pos,
                };
            }
        }
        if changed {
            *cx.text = buf.text();
        }
    }

    fn leave_insert(&mut self, cx: &mut Cx) {
        let (count, kind) = self.insert_repeat;
        if count > 1 && !self.insert_keys.is_empty() && !cx.readonly {
            let keys = std::mem::take(&mut self.insert_keys);
            for _ in 1..count {
                if matches!(kind, 'o' | 'O') {
                    let buf = Buf::new(cx.text);
                    let pos = (*cx.cursor).min(buf.len());
                    let indent = auto_indent(&buf.line_text(pos));
                    let mut buf = buf;
                    let e = buf.line_end(pos);
                    *cx.cursor = e + buf.insert(e, &format!("\n{indent}"));
                    *cx.text = buf.text();
                }
                for k in &keys {
                    self.type_key(k, cx);
                }
            }
        }
        self.insert_keys.clear();
        self.mode = Mode::Normal;
        let buf = Buf::new(cx.text);
        if *cx.cursor > buf.line_start(*cx.cursor) {
            *cx.cursor -= 1;
        }
        self.end_change(cx);
        if let Some(rec) = self.dot_rec.take() {
            self.last_change = rec;
        }
    }

    // ---- Command line ----

    fn cmdline_key(&mut self, key: VimKey, cx: &mut Cx, fx: &mut Vec<Effect>) {
        let Some((prompt, input)) = &mut self.cmdline else {
            self.mode = Mode::Normal;
            return;
        };
        match key {
            VimKey::Esc | VimKey::Copy | VimKey::Ctrl('c') => {
                self.cmdline = None;
                self.search_op = None;
                self.mode = Mode::Normal;
            }
            VimKey::Backspace => {
                if input.pop().is_none() {
                    self.cmdline = None;
                    self.search_op = None;
                    self.mode = Mode::Normal;
                }
            }
            VimKey::Char(c) => input.push(c),
            VimKey::Paste(t) => input.push_str(t.lines().next().unwrap_or("")),
            VimKey::Enter => {
                let (prompt, input) = (*prompt, std::mem::take(input));
                self.cmdline = None;
                self.mode = Mode::Normal;
                if prompt == ':' {
                    self.run_ex(&input, cx, fx);
                } else {
                    self.run_search(prompt == '/', input, cx, fx);
                }
            }
            _ => {}
        }
    }

    fn run_search(&mut self, forward: bool, input: String, cx: &mut Cx, fx: &mut Vec<Effect>) {
        let op = self.search_op.take();
        let pattern = if input.is_empty() {
            match &self.search {
                Some(s) => s.pattern.clone(),
                None => return,
            }
        } else {
            input
        };
        let s = Search {
            pattern,
            forward,
            whole_word: false,
        };
        self.search = Some(s.clone());
        self.highlight = true;
        let mut buf = Buf::new(cx.text);
        let pos = *cx.cursor;
        let Some(p) = self.find_next(&buf, pos, forward, &s) else {
            return;
        };
        let Some((op, reg)) = op else {
            *cx.cursor = p;
            return;
        };
        let r = Range {
            start: pos.min(p),
            end: pos.max(p),
            linewise: false,
        };
        if op != Op::Yank {
            self.begin_change(cx);
        }
        self.apply(op, reg, r, &mut buf, cx, fx);
        if op != Op::Yank && self.mode != Mode::Insert {
            self.end_change(cx);
        }
    }

    fn run_ex(&mut self, input: &str, cx: &mut Cx, fx: &mut Vec<Effect>) {
        use ex::Ex;
        let buf = Buf::new(cx.text);
        match ex::parse(input) {
            Ex::Write(None) => fx.push(Effect::Save),
            Ex::Write(Some(p)) => fx.push(Effect::SaveAs(p)),
            Ex::Quit { force } => {
                if !force && cx.dirty {
                    self.error("E37: No write since last change (add ! to override)");
                } else {
                    fx.push(Effect::Close { force });
                }
            }
            Ex::WriteQuit => fx.extend([Effect::Save, Effect::Close { force: false }]),
            Ex::Exit => {
                if cx.dirty {
                    fx.push(Effect::Save);
                }
                fx.push(Effect::Close { force: false });
            }
            Ex::QuitAll { force } => fx.push(Effect::Quit { force }),
            Ex::Edit(p) => fx.push(Effect::Edit(p)),
            Ex::Line(n) => *cx.cursor = buf.first_non_blank(buf.start_of_line(n.saturating_sub(1))),
            Ex::LastLine => *cx.cursor = buf.first_non_blank(buf.start_of_line(buf.lines() - 1)),
            Ex::NoHighlight => self.highlight = false,
            Ex::Make => fx.push(Effect::Make),
            Ex::Next => fx.push(Effect::NextDoc),
            Ex::Prev => fx.push(Effect::PrevDoc),
            Ex::Unknown(s) => self.error(format!("E492: Not an editor command: {s}")),
        }
    }
}

fn find_kind(kind: char) -> Kind {
    if matches!(kind, 'f' | 't') {
        Kind::Inclusive
    } else {
        Kind::Exclusive
    }
}

fn toggle_case(c: char) -> char {
    if c.is_lowercase() {
        c.to_uppercase().next().unwrap_or(c)
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// The keyword under (or after, on the same line) the cursor.
fn word_at(buf: &Buf, pos: usize) -> Option<String> {
    let e = buf.line_end(pos);
    let start = (pos..e).find(|&i| is_keyword(buf.c[i]))?;
    let mut a = start;
    while a > buf.line_start(pos) && is_keyword(buf.c[a - 1]) {
        a -= 1;
    }
    let mut b = start;
    while b < e && is_keyword(buf.c[b]) {
        b += 1;
    }
    Some(buf.c[a..b].iter().collect())
}

/// Smartcase: case-insensitive unless the pattern has capitals.
fn matches_at(buf: &Buf, i: usize, s: &Search) -> bool {
    let pat: Vec<char> = s.pattern.chars().collect();
    if pat.is_empty() || i + pat.len() > buf.len() {
        return false;
    }
    let exact = pat.iter().any(|c| c.is_uppercase());
    let eq = |a: char, b: char| {
        if exact {
            a == b
        } else {
            a.to_lowercase().eq(b.to_lowercase())
        }
    };
    if !pat.iter().enumerate().all(|(k, &p)| eq(buf.c[i + k], p)) {
        return false;
    }
    !s.whole_word
        || ((i == 0 || !is_keyword(buf.c[i - 1]))
            && (i + pat.len() == buf.len() || !is_keyword(buf.c[i + pat.len()])))
}
