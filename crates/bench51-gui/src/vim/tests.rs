//! Engine tests. Buffers are written with `|` marking the cursor.

use super::keys::parse_keys;
use super::{Cx, Effect, History, Mode, View, Vim};

struct T {
    vim: Vim,
    text: String,
    cursor: usize,
    history: History,
    readonly: bool,
    dirty: bool,
    fx: Vec<Effect>,
}

impl T {
    fn new(marked: &str) -> Self {
        let cursor = marked.chars().position(|c| c == '|').unwrap_or(0);
        Self {
            vim: Vim::default(),
            text: marked.replace('|', ""),
            cursor,
            history: History::default(),
            readonly: false,
            dirty: false,
            fx: Vec::new(),
        }
    }

    fn keys(&mut self, keys: &str) -> &mut Self {
        for k in parse_keys(keys) {
            let mut cx = Cx {
                text: &mut self.text,
                cursor: &mut self.cursor,
                history: &mut self.history,
                readonly: self.readonly,
                dirty: self.dirty,
                view: View { top: 0, rows: 10 },
            };
            let fx = self.vim.key(k, &mut cx);
            self.fx.extend(fx);
        }
        self
    }

    fn show(&self) -> String {
        let mut s: Vec<char> = self.text.chars().collect();
        s.insert(self.cursor.min(s.len()), '|');
        s.into_iter().collect()
    }
}

fn after(start: &str, keys: &str) -> String {
    T::new(start).keys(keys).show()
}

#[test]
fn word_and_line_motions() {
    assert_eq!(after("|MOV A,#1 ; c", "w"), "MOV |A,#1 ; c");
    assert_eq!(after("|MOV A,#1 ; c", "ww"), "MOV A|,#1 ; c");
    assert_eq!(after("|MOV A,#1 ; c", "W"), "MOV |A,#1 ; c");
    assert_eq!(after("|MOV A,#1 ; c", "2W"), "MOV A,#1 |; c");
    assert_eq!(after("MOV A,#1 ; |c", "b"), "MOV A,#1 |; c");
    assert_eq!(after("|MOV A,#1", "e"), "MO|V A,#1");
    assert_eq!(after("  MOV |A", "0"), "|  MOV A");
    assert_eq!(after("  MOV |A", "^"), "  |MOV A");
    assert_eq!(after("  |MOV A", "$"), "  MOV |A");
    assert_eq!(after("foo\n\n|bar", "b"), "foo\n|\nbar");
    assert_eq!(after("|foo\n\nbar", "w"), "foo\n|\nbar");
}

#[test]
fn vertical_motions_keep_the_column() {
    assert_eq!(after("abcd|ef\nab\nabcdef", "j"), "abcdef\na|b\nabcdef");
    assert_eq!(after("abcd|ef\nab\nabcdef", "jj"), "abcdef\nab\nabcd|ef");
    assert_eq!(after("a|bc\nabcdef", "$j"), "abc\nabcde|f");
    assert_eq!(after("a\nb\nc\n|d", "gg"), "|a\nb\nc\nd");
    assert_eq!(after("|a\nb\nc\nd", "G"), "a\nb\nc\n|d");
    assert_eq!(after("|a\nb\nc\nd", "3G"), "a\nb\n|c\nd");
    assert_eq!(after("|a\nb\nc\nd", "2j"), "a\nb\n|c\nd");
    assert_eq!(after("|a\n  b", "<CR>"), "a\n  |b");
}

#[test]
fn find_and_match_motions() {
    assert_eq!(after("|a,b,c,d", "f,"), "a|,b,c,d");
    assert_eq!(after("|a,b,c,d", "f,;"), "a,b|,c,d");
    assert_eq!(after("|a,b,c,d", "f,;,"), "a|,b,c,d");
    assert_eq!(after("|a,b,c,d", "2f,"), "a,b|,c,d");
    assert_eq!(after("|a,b,c,d", "t,"), "|a,b,c,d");
    assert_eq!(after("|ab,c", "t,"), "a|b,c");
    assert_eq!(after("a,b,|c", "F,"), "a,b|,c");
    assert_eq!(after("MOV A,|#(1+(2))", "%"), "MOV A,#(1+(2)|)");
    assert_eq!(after("MOV A,#(1+(2)|)", "%"), "MOV A,#|(1+(2))");
    assert_eq!(after("|a\nb\n\nc\nd", "}"), "a\nb\n|\nc\nd");
}

#[test]
fn delete_yank_put_lines() {
    assert_eq!(after("a\n|b\nc", "dd"), "a\n|c");
    assert_eq!(after("a\nb\n|c", "dd"), "a\n|b");
    assert_eq!(after("|a\nb\nc", "2dd"), "|c");
    assert_eq!(after("|a\nb\nc", "dj"), "|c");
    assert_eq!(after("a\n|b\nc", "dk"), "|c");
    assert_eq!(after("|a\nb", "yyp"), "a\n|a\nb");
    assert_eq!(after("a\n|b", "yyp"), "a\nb\n|b");
    assert_eq!(after("a\n|b", "yyP"), "a\n|b\nb");
    assert_eq!(after("|  a\nb", "ddp"), "b\n  |a");
    assert_eq!(after("|a\nb\nc", "dG"), "|");
    assert_eq!(after("|para\none\n\nnext", "d}"), "|\nnext");
}

#[test]
fn charwise_operators() {
    assert_eq!(after("foo |bar\nbaz", "dw"), "foo| \nbaz");
    assert_eq!(after("|foo bar", "dw"), "|bar");
    assert_eq!(after("|foo bar", "d2w"), "|");
    assert_eq!(after("|MOV A", "cwMOVX<Esc>"), "MOV|X A");
    assert_eq!(after("MO|V A", "cw<Esc>"), "M|O A");
    assert_eq!(after("MOV A,B|B", "ciwR7<Esc>"), "MOV A,R|7");
    assert_eq!(after("DB \"hel|lo\", 0", "ci\"bye<Esc>"), "DB \"by|e\", 0");
    assert_eq!(after("DB \"hel|lo\", 0", "da\""), "DB |, 0");
    assert_eq!(after("MOV A,#(1+|2)", "di("), "MOV A,#(|)");
    assert_eq!(after("MOV A,#(1+|2)", "da("), "MOV A,|#");
    assert_eq!(after("a|bc", "x"), "a|c");
    assert_eq!(after("ab|c", "x"), "a|b");
    assert_eq!(after("ab|c", "X"), "a|c");
    assert_eq!(after("a|bcd", "D"), "|a");
    assert_eq!(after("a|bcd", "Cx<Esc>"), "a|x");
    assert_eq!(after("a|bcd", "sX<Esc>"), "a|Xcd");
    assert_eq!(after("|MOV A,B", "dt,"), "|,B");
    assert_eq!(after("|MOV A,B", "df,"), "|B");
    assert_eq!(after("|MOV A,B", "y$P"), "MOV A,|BMOV A,B");
}

#[test]
fn indent_join_case_replace() {
    assert_eq!(after("|MOV A,B", ">>"), "        |MOV A,B");
    assert_eq!(after("        MOV |A,B", "<<"), "|MOV A,B");
    assert_eq!(after("|a\nb", "2>>"), "        |a\n        b");
    assert_eq!(after("|a\n   b", "J"), "a| b");
    assert_eq!(after("|a\nb\nc", "3J"), "a b| c");
    assert_eq!(after("|abc", "~~"), "AB|c");
    assert_eq!(after("|abc", "rx"), "|xbc");
    assert_eq!(after("|abc", "2rx"), "x|xc");
}

#[test]
fn undo_redo_and_dot() {
    assert_eq!(after("|abc", "xxu"), "|bc");
    assert_eq!(after("|abc", "xxuu"), "|abc");
    assert_eq!(after("|abc", "xxuu<C-r>"), "|bc");
    assert_eq!(after("|abc", "ixy<Esc>u"), "|abc");
    assert_eq!(after("|a b c d", "dw."), "|c d");
    assert_eq!(after("|a b c d", "dw2."), "|d");
    assert_eq!(after("|ab\ncd", "Ax<Esc>j."), "abx\ncd|x");
    assert_eq!(after("|a b", "cwX<Esc>w."), "X |X");
    // Undoing a `.` undoes the whole repeated change.
    assert_eq!(after("|ab\ncd", "Axy<Esc>j.u"), "abxy\nc|d");
}

#[test]
fn insert_mode_and_counts() {
    assert_eq!(after("|", "3ix<Esc>"), "xx|x");
    assert_eq!(after("a|b", "ix<Esc>"), "a|xb");
    assert_eq!(after("a|b", "ax<Esc>"), "ab|x");
    assert_eq!(after("  a|b", "Ix<Esc>"), "  |xab");
    assert_eq!(after("a|b", "Ax<Esc>"), "ab|x");
    assert_eq!(after("a|b", "ix<BS>y<Esc>"), "a|yb");
    assert_eq!(after("|", "i<Tab>x<Esc>"), "        |x");
    assert_eq!(after("ab|", "a<CR>c<Esc>"), "ab\n|c");
    assert_eq!(after("|a", "2ox<Esc>"), "a\nx\n|x");
}

#[test]
fn new_lines_line_up_after_labels() {
    assert_eq!(
        after("|main:   MOV A,#1", "oNOP<Esc>"),
        "main:   MOV A,#1\n        NO|P"
    );
    assert_eq!(after("|loop:", "oNOP<Esc>"), "loop:\n        NO|P");
    assert_eq!(
        after("|        MOV A,B", "oX<Esc>"),
        "        MOV A,B\n        |X"
    );
    assert_eq!(after("|; comment", "oX<Esc>"), "; comment\n|X");
    assert_eq!(after("        |NOP", "OX<Esc>"), "        |X\n        NOP");
}

#[test]
fn visual_mode() {
    let mut t = T::new("|abcdef");
    t.keys("lvlly");
    assert_eq!(t.show(), "a|bcdef");
    assert!(t.fx.contains(&Effect::Clipboard("bcd".into())));
    t.keys("$p");
    assert_eq!(t.show(), "abcdefbc|d");
    assert_eq!(after("a\n|b\nc\nd", "Vjd"), "a\n|d");
    assert_eq!(after("|abcdef", "vld"), "|cdef");
    assert_eq!(after("|abcdef", "vlcX<Esc>"), "|Xcdef");
    assert_eq!(after("|a\nb", "Vj>"), "        |a\n        b");
    assert_eq!(after("|abc", "vl~"), "|ABc");
    assert_eq!(after("|ab cd", "yiwwviwp"), "ab |ab");
    let mut t = T::new("|abc");
    t.keys("v");
    assert_eq!(t.vim.selection(&t.text, t.cursor), Some((0, 1)));
    t.keys("l");
    assert_eq!(t.vim.selection(&t.text, t.cursor), Some((0, 2)));
    t.keys("<Esc>");
    assert_eq!(t.vim.mode, Mode::Normal);
}

#[test]
fn registers_and_clipboard() {
    let mut t = T::new("|one\ntwo");
    t.keys("\"ayyj\"byy\"ap");
    assert_eq!(t.show(), "one\ntwo\n|one");
    assert!(t.fx.contains(&Effect::Clipboard("one\n".into())));
    // The black hole register doesn't touch the clipboard or `"`.
    let mut t = T::new("|ab");
    t.keys("yl\"_xp");
    assert_eq!(t.show(), "b|a");
    assert_eq!(t.fx, vec![Effect::Clipboard("a".into())]);
    // Ctrl+V pastes the system clipboard before the cursor.
    let mut t = T::new("a|b");
    t.vim.key(
        super::VimKey::Paste("XY".into()),
        &mut Cx {
            text: &mut t.text,
            cursor: &mut t.cursor,
            history: &mut t.history,
            readonly: false,
            dirty: false,
            view: View::default(),
        },
    );
    assert_eq!(t.show(), "aX|Yb");
}

#[test]
fn search() {
    assert_eq!(after("|foo bar\nbaz bar", "/bar<CR>"), "foo |bar\nbaz bar");
    assert_eq!(after("|foo bar\nbaz bar", "/bar<CR>n"), "foo bar\nbaz |bar");
    assert_eq!(
        after("|foo bar\nbaz bar", "/bar<CR>nn"),
        "foo |bar\nbaz bar"
    );
    assert_eq!(after("|foo bar\nbaz bar", "/bar<CR>N"), "foo bar\nbaz |bar");
    assert_eq!(after("|foo bar\nbaz bar", "?baz<CR>"), "foo bar\n|baz bar");
    assert_eq!(after("|foo Bar bar", "/bar<CR>"), "foo |Bar bar");
    assert_eq!(after("|foo Bar bar", "/Bar<CR>n"), "foo |Bar bar");
    assert_eq!(after("|loop loops loop", "*"), "loop loops |loop");
    assert_eq!(after("|foo bar", "d/bar<CR>"), "|bar");
    assert_eq!(after("|foo bar", "d/zzz<CR>"), "|foo bar");
    assert_eq!(after("|foo bar", "d/<Esc>x"), "|oo bar");
    let mut t = T::new("|foo bar bar");
    t.keys("/bar<CR>");
    assert_eq!(t.vim.search_ranges(&t.text), vec![(4, 7), (8, 11)]);
    t.keys(":noh<CR>");
    assert!(t.vim.search_ranges(&t.text).is_empty());
    let mut t = T::new("|foo");
    t.keys("/zzz<CR>");
    assert_eq!(t.show(), "|foo");
    assert!(t.vim.message.as_ref().is_some_and(|m| m.1));
}

#[test]
fn macros() {
    assert_eq!(after("|1\n2\n3", "qaA;<Esc>jq@a"), "1;\n2;\n|3");
    assert_eq!(after("|1\n2\n3\n4", "qaA;<Esc>jq2@a"), "1;\n2;\n3;\n|4");
    assert_eq!(after("|1\n2\n3", "qaA;<Esc>jq@a@@"), "1;\n2;\n3|;");
    let mut t = T::new("|x");
    t.keys("qa");
    assert_eq!(t.vim.recording(), Some('a'));
    t.keys("q");
    assert_eq!(t.vim.recording(), None);
}

#[test]
fn ex_commands() {
    let mut t = T::new("|a\nb\nc");
    t.keys(":w<CR>:wq<CR>:qa!<CR>:make<CR>:bn<CR>");
    assert_eq!(
        t.fx,
        vec![
            Effect::Save,
            Effect::Save,
            Effect::Close { force: false },
            Effect::Quit { force: true },
            Effect::Make,
            Effect::NextDoc,
        ]
    );
    assert_eq!(after("|a\nb\n  c", ":3<CR>"), "a\nb\n  |c");
    assert_eq!(after("|a\nb\n  c", ":$<CR>"), "a\nb\n  |c");
    let mut t = T::new("|a");
    t.dirty = true;
    t.keys(":q<CR>");
    assert!(t.fx.is_empty());
    assert!(
        t.vim
            .message
            .as_ref()
            .is_some_and(|m| m.0.starts_with("E37"))
    );
    t.keys(":q!<CR>");
    assert_eq!(t.fx, vec![Effect::Close { force: true }]);
    let mut t = T::new("|a");
    t.keys(":wat<CR>");
    assert!(
        t.vim
            .message
            .as_ref()
            .is_some_and(|m| m.0.starts_with("E492"))
    );
    assert_eq!(t.vim.mode, Mode::Normal);
}

#[test]
fn read_only_allows_moving_and_yanking() {
    let mut t = T::new("|abc\ndef");
    t.readonly = true;
    t.keys("x");
    assert_eq!(t.show(), "|abc\ndef");
    assert!(t.vim.message.as_ref().is_some_and(|m| m.1));
    t.keys("ix");
    assert_eq!(t.vim.mode, Mode::Normal);
    t.keys("jlyl");
    assert_eq!(t.show(), "abc\nd|ef");
    assert!(t.fx.contains(&Effect::Clipboard("e".into())));
}

#[test]
fn goto_definition_and_tabs() {
    assert_eq!(
        after("  SJMP |loop\nloop:  NOP", "gd"),
        "  SJMP loop\n|loop:  NOP"
    );
    assert_eq!(
        after("MOV A,#|RELOAD\nRELOAD EQU 5", "gd"),
        "MOV A,#RELOAD\n|RELOAD EQU 5"
    );
    let mut t = T::new("|a");
    t.keys("gtgT");
    assert_eq!(t.fx, vec![Effect::NextDoc, Effect::PrevDoc]);
}

#[test]
fn escape_and_pending() {
    let mut t = T::new("|abc");
    t.keys("2d");
    assert_eq!(t.vim.pending_label(), "2d");
    t.keys("<Esc>");
    assert_eq!(t.vim.pending_label(), "");
    assert_eq!(t.show(), "|abc");
    assert_eq!(after("ab|c", "i<Esc>"), "a|bc");
    assert_eq!(after("|abc", "i<Esc>"), "|abc");
}
