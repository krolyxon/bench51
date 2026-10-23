use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui_textarea::{CursorMove, TextArea};

use crate::asm::{self, AsmOutput};
use crate::sim::{Sim, Stop, disasm};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Edit,
    Debug,
}

/// Right-hand pane in edit mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutTab {
    Listing,
    Hex,
    Errors,
}

/// Focusable panels in debug mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Source,
    Memory,
    Ports,
    Serial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemTab {
    Iram,
    Sfr,
    Xram,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Open,
    SaveAs,
    /// `addr value` poke into the memory shown in the memory panel.
    Poke,
}

pub struct Prompt {
    pub kind: PromptKind,
    pub input: String,
}

/// Result of the last successful assembly, which the debugger runs.
pub struct Build {
    pub source: Vec<String>,
    pub out: AsmOutput,
    pub addr_to_line: HashMap<u16, usize>,
}

impl Build {
    /// First address emitted by `line`, if any.
    pub fn line_addr(&self, line: usize) -> Option<u16> {
        self.out
            .lines
            .get(line)
            .filter(|l| !l.bytes.is_empty())
            .and_then(|l| l.addr)
    }
}

/// Instructions executed per frame (~30 fps) for each speed setting.
pub const SPEEDS: [u32; 8] = [1, 5, 25, 100, 500, 2_000, 10_000, 40_000];

pub struct App {
    pub path: Option<PathBuf>,
    pub editor: TextArea<'static>,
    saved_text: String,
    pub mode: Mode,
    pub out_tab: OutTab,
    pub focus: Focus,
    pub mem_tab: MemTab,
    pub mem_scroll: usize,
    /// Last assembler result, even if it had errors (for diagnostics).
    pub last_asm: Option<(String, AsmOutput)>,
    /// Last error-free build, loaded into the simulator.
    pub build: Option<Build>,
    pub sim: Sim,
    /// Breakpoints by source line.
    pub breakpoints: HashSet<usize>,
    pub running: bool,
    /// The next run chunk starts a fresh run and may leave a breakpoint.
    resume: bool,
    pub speed: usize,
    pub clock_mhz: f64,
    pub status: String,
    /// Selected line in the debug source view.
    pub src_cursor: usize,
    pub src_scroll: usize,
    pub out_scroll: usize,
    pub port_sel: (usize, u8),
    pub serial_input: String,
    pub prev_iram: [u8; 256],
    pub prev_sfr: [u8; 128],
    pub show_help: bool,
    pub prompt: Option<Prompt>,
    pub quit: bool,
    quit_armed: bool,
    /// Set by Ctrl-G; the event loop suspends the TUI and calls
    /// `edit_externally`.
    pub external_edit: bool,
}

impl App {
    pub fn new(path: Option<PathBuf>, clock_mhz: f64) -> Self {
        let mut app = Self {
            path: None,
            editor: TextArea::default(),
            saved_text: String::new(),
            mode: Mode::Edit,
            out_tab: OutTab::Listing,
            focus: Focus::Source,
            mem_tab: MemTab::Iram,
            mem_scroll: 0,
            last_asm: None,
            build: None,
            sim: Sim::new(),
            breakpoints: HashSet::new(),
            running: false,
            resume: false,
            speed: 5,
            clock_mhz,
            status: "F1 help · F7 assemble · F5 run · F11 step".into(),
            src_cursor: 0,
            src_scroll: 0,
            out_scroll: 0,
            port_sel: (1, 0),
            serial_input: String::new(),
            prev_iram: [0; 256],
            prev_sfr: [0; 128],
            show_help: false,
            prompt: None,
            quit: false,
            quit_armed: false,
            external_edit: false,
        };
        match path {
            Some(p) => app.open(&p),
            None => app.set_text(TEMPLATE),
        }
        app
    }

    fn set_text(&mut self, text: &str) {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        self.editor = TextArea::new(if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        });
        crate::ui::draw::style_editor(&mut self.editor);
        self.saved_text = self.text();
        self.breakpoints.clear();
        self.build = None;
        self.last_asm = None;
        self.focus = Focus::Source;
        self.serial_input.clear();
    }

    pub fn text(&self) -> String {
        self.editor.lines().join("\n")
    }

    pub fn dirty(&self) -> bool {
        self.text() != self.saved_text
    }

    /// Source differs from what the simulator is running.
    pub fn stale(&self) -> bool {
        self.build
            .as_ref()
            .is_none_or(|b| b.source != self.editor.lines())
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .map_or("untitled.asm".to_string(), |p| p.display().to_string());
        format!("{name}{}", if self.dirty() { " [+]" } else { "" })
    }

    // ---- Files ----

    fn open(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("cannot open {}: {e}", path.display());
                return;
            }
        };
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("hex") || e.eq_ignore_ascii_case("ihx"))
        {
            match asm::hex::read(&text) {
                Ok(bytes) => {
                    let src = disassemble_image(&bytes, path);
                    self.set_text(&src);
                    self.saved_text.clear();
                    self.path = None;
                    self.assemble();
                    self.status = format!(
                        "loaded {} ({} bytes) and disassembled it",
                        path.display(),
                        bytes.len()
                    );
                }
                Err(e) => self.status = format!("invalid HEX file: {e}"),
            }
        } else {
            self.set_text(&text);
            self.path = Some(path.to_path_buf());
            self.mode = Mode::Edit;
            self.status = format!("opened {}", path.display());
        }
    }

    fn save(&mut self, path: Option<PathBuf>) {
        let Some(path) = path.or_else(|| self.path.clone()) else {
            self.prompt = Some(Prompt {
                kind: PromptKind::SaveAs,
                input: String::new(),
            });
            return;
        };
        let mut text = self.text();
        text.push('\n');
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.saved_text = self.text();
                self.status = format!("saved {}", path.display());
                self.path = Some(path);
            }
            Err(e) => self.status = format!("cannot save {}: {e}", path.display()),
        }
    }

    /// Opens the source in `$EDITOR` and loads the result back. The
    /// caller must have suspended the TUI. A saved file is edited in place
    /// (unsaved changes are saved first); an untitled buffer goes through
    /// a temporary file.
    pub fn edit_externally(&mut self) {
        let (path, temp) = match self.path.clone() {
            Some(p) => {
                if self.dirty() {
                    self.save(None);
                    if self.dirty() {
                        return; // save failed; `save` set the status
                    }
                }
                (p, false)
            }
            None => {
                let p = std::env::temp_dir().join(format!("bench51-{}.asm", std::process::id()));
                if let Err(e) = std::fs::write(&p, self.text() + "\n") {
                    self.status = format!("cannot write {}: {e}", p.display());
                    return;
                }
                (p, true)
            }
        };

        let line = self.editor.cursor().0 + 1;
        let result = editor_command(&path, line).status();
        let text = std::fs::read_to_string(&path);
        if temp {
            let _ = std::fs::remove_file(&path);
        }
        match (result, text) {
            (Err(e), _) => self.status = format!("cannot start editor: {e}"),
            (Ok(s), _) if !s.success() => {
                self.status = format!("editor exited with {s}; changes not loaded")
            }
            (Ok(_), Err(e)) => self.status = format!("cannot read {}: {e}", path.display()),
            (Ok(_), Ok(text)) => self.reload(&text, !temp),
        }
    }

    /// Replaces the buffer with externally edited text, keeping the cursor
    /// and breakpoints, and reassembles if anything changed.
    fn reload(&mut self, text: &str, from_saved_file: bool) {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        if lines == self.editor.lines() {
            self.status = "no changes from external editor".into();
            return;
        }
        let cursor = self.editor.cursor();
        let (row, col) = (cursor.0, cursor.1);
        self.editor = TextArea::new(lines);
        crate::ui::draw::style_editor(&mut self.editor);
        self.editor
            .move_cursor(CursorMove::Jump(row as u16, col as u16));
        if from_saved_file {
            self.saved_text = self.text();
        }
        let n = self.editor.lines().len();
        self.breakpoints.retain(|&l| l < n);
        if self.assemble() {
            self.status = format!("reloaded from external editor; {}", self.status);
        }
    }

    // ---- Build & run ----

    /// Assembles the editor contents. On success the program is loaded
    /// into a freshly reset simulator.
    pub fn assemble(&mut self) -> bool {
        let source = self.text();
        let out = asm::assemble(&source);
        let ok = out.ok();
        if ok {
            self.sim.load_code(out.bytes());
            self.sim.reset();
            self.running = false;
            self.status = format!("assembled: {} bytes, 0 errors", out.code_size());
            self.write_outputs(&out, &source);
            self.build = Some(Build {
                source: self.editor.lines().to_vec(),
                addr_to_line: out.addr_to_line(),
                out: out.clone(),
            });
            self.snapshot();
            self.follow_pc();
        } else {
            let first = &out.diagnostics[0];
            self.status = format!(
                "{} error(s); line {}: {}",
                out.diagnostics.len(),
                first.line + 1,
                first.msg
            );
            self.editor
                .move_cursor(CursorMove::Jump(first.line as u16, first.col as u16));
            self.mode = Mode::Edit;
            self.out_tab = OutTab::Errors;
        }
        self.out_scroll = 0;
        self.last_asm = Some((source, out));
        ok
    }

    /// Writes `.lst` and `.hex` next to a saved source file.
    fn write_outputs(&mut self, out: &AsmOutput, source: &str) {
        let Some(path) = &self.path else { return };
        let res = std::fs::write(path.with_extension("lst"), out.listing(source))
            .and_then(|_| std::fs::write(path.with_extension("hex"), out.intel_hex()));
        if let Err(e) = res {
            self.status = format!("assembled, but writing .lst/.hex failed: {e}");
        }
    }

    /// Makes sure an up-to-date build is loaded; returns false on errors.
    fn ensure_built(&mut self) -> bool {
        if self.stale() && !self.assemble() {
            return false;
        }
        if self.mode == Mode::Edit {
            self.mode = Mode::Debug;
            self.focus = Focus::Source;
        }
        true
    }

    pub fn bp_addrs(&self) -> HashSet<u16> {
        let Some(b) = &self.build else {
            return HashSet::new();
        };
        self.breakpoints
            .iter()
            .filter_map(|&l| b.line_addr(l))
            .collect()
    }

    pub fn snapshot(&mut self) {
        self.prev_iram = self.sim.iram;
        self.prev_sfr = self.sim.sfr;
    }

    pub fn pc_line(&self) -> Option<usize> {
        self.build.as_ref()?.addr_to_line.get(&self.sim.pc).copied()
    }

    /// Scrolls the source view so the PC line is visible.
    pub fn follow_pc(&mut self) {
        if let Some(l) = self.pc_line() {
            self.src_cursor = l;
        }
    }

    fn report(&mut self, stop: Stop) {
        self.running = false;
        self.status = match stop {
            Stop::Breakpoint(pc) => format!("breakpoint at {pc:04X}h"),
            Stop::ReservedOpcode { pc, opcode } => {
                format!("reserved opcode {opcode:02X}h at {pc:04X}h — stopped")
            }
            Stop::PowerDown => "CPU in power-down mode (PCON.PD) — reset to continue".into(),
            Stop::Target | Stop::Budget => format!("stopped at {:04X}h", self.sim.pc),
        };
        self.follow_pc();
    }

    fn step_cmd(&mut self, f: impl FnOnce(&mut Sim, &HashSet<u16>) -> Stop) {
        if !self.ensure_built() {
            return;
        }
        self.running = false;
        self.snapshot();
        let bps = self.bp_addrs();
        let stop = f(&mut self.sim, &bps);
        match stop {
            Stop::Target => {
                self.status = format!("PC = {:04X}h", self.sim.pc);
                self.follow_pc();
            }
            Stop::Budget => self.report(Stop::Budget),
            s => self.report(s),
        }
    }

    fn toggle_run(&mut self) {
        if self.running {
            self.running = false;
            self.status = format!("paused at {:04X}h", self.sim.pc);
            self.follow_pc();
        } else if self.ensure_built() {
            self.running = true;
            self.resume = true;
            self.status = "running…".into();
        }
    }

    fn run_to_cursor(&mut self) {
        if !self.ensure_built() {
            return;
        }
        let line = if self.mode == Mode::Debug {
            self.src_cursor
        } else {
            self.editor.cursor().0
        };
        let Some(target) = self.build.as_ref().and_then(|b| b.line_addr(line)) else {
            self.status = "no code on that line".into();
            return;
        };
        self.step_cmd(|s, bps| {
            let mut bps = bps.clone();
            bps.insert(target);
            match s.run(5_000_000, &bps, true) {
                Stop::Breakpoint(pc) if pc == target => Stop::Target,
                other => other,
            }
        });
    }

    fn reset(&mut self) {
        self.sim.reset();
        self.running = false;
        self.snapshot();
        self.follow_pc();
        self.status = "reset".into();
    }

    fn toggle_breakpoint(&mut self) {
        let line = if self.mode == Mode::Debug {
            self.src_cursor
        } else {
            self.editor.cursor().0
        };
        if !self.breakpoints.remove(&line) {
            self.breakpoints.insert(line);
        }
    }

    /// Called once per frame.
    pub fn tick(&mut self) {
        if !self.running {
            return;
        }
        self.snapshot();
        let bps = self.bp_addrs();
        let resume = std::mem::take(&mut self.resume);
        match self.sim.run(SPEEDS[self.speed], &bps, resume) {
            Stop::Budget => {
                if let Some(l) = self.pc_line() {
                    self.src_cursor = l;
                }
            }
            s => self.report(s),
        }
    }

    // ---- Input ----

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let delta: isize = match m.kind {
            MouseEventKind::ScrollDown => 3,
            MouseEventKind::ScrollUp => -3,
            _ => return,
        };
        match self.mode {
            Mode::Edit => self.out_scroll = self.out_scroll.saturating_add_signed(delta),
            Mode::Debug => self.src_cursor = self.src_cursor.saturating_add_signed(delta),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if !(ctrl && key.code == KeyCode::Char('q')) {
            self.quit_armed = false;
        }
        if self.prompt.is_some() {
            self.on_prompt_key(key);
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }

        // Global keys.
        match (key.code, ctrl) {
            (KeyCode::Char('q'), true) => {
                if self.dirty() && !self.quit_armed {
                    self.quit_armed = true;
                    self.status = "unsaved changes — press Ctrl-Q again to quit".into();
                } else {
                    self.quit = true;
                }
            }
            (KeyCode::F(1), _) => self.show_help = true,
            (KeyCode::F(2), _) | (KeyCode::Char('s'), true) => self.save(None),
            (KeyCode::Char('g'), true) => self.external_edit = true,
            (KeyCode::F(3), _) | (KeyCode::Char('o'), true) => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Open,
                    input: String::new(),
                })
            }
            (KeyCode::F(4), _) => match self.mode {
                // ensure_built switches to debug mode on success.
                Mode::Edit => {
                    self.ensure_built();
                }
                Mode::Debug => self.mode = Mode::Edit,
            },
            (KeyCode::F(5), _) => self.toggle_run(),
            (KeyCode::F(6), _) => self.reset(),
            (KeyCode::F(7), _) => {
                if self.assemble() && self.mode == Mode::Edit {
                    self.out_tab = OutTab::Listing;
                }
            }
            (KeyCode::F(8), _) => self.run_to_cursor(),
            (KeyCode::F(9), _) => self.toggle_breakpoint(),
            (KeyCode::F(10), _) => self.step_cmd(|s, b| s.step_over(5_000_000, b)),
            (KeyCode::F(11), _) => self.step_cmd(|s, _| match s.step() {
                Ok(()) => Stop::Target,
                Err(e) => e,
            }),
            (KeyCode::F(12), _) => self.step_cmd(|s, b| s.step_out(5_000_000, b)),
            _ => match self.mode {
                Mode::Edit => self.on_edit_key(key),
                Mode::Debug => self.on_debug_key(key),
            },
        }
    }

    fn on_edit_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            // Alt-1/2/3 switch the output tab; Alt-Up/Down scroll it.
            KeyCode::Char('1') if alt => self.out_tab = OutTab::Listing,
            KeyCode::Char('2') if alt => self.out_tab = OutTab::Hex,
            KeyCode::Char('3') if alt => self.out_tab = OutTab::Errors,
            KeyCode::Up if alt => self.out_scroll = self.out_scroll.saturating_sub(1),
            KeyCode::Down if alt => self.out_scroll += 1,
            KeyCode::PageUp if alt => self.out_scroll = self.out_scroll.saturating_sub(20),
            KeyCode::PageDown if alt => self.out_scroll += 20,
            KeyCode::Esc if self.running => self.toggle_run(),
            _ => {
                self.editor.input(key);
            }
        }
    }

    fn on_debug_key(&mut self, key: KeyEvent) {
        if self.focus == Focus::Serial {
            match key.code {
                KeyCode::Enter => {
                    let mut bytes = std::mem::take(&mut self.serial_input).into_bytes();
                    bytes.push(b'\r');
                    self.sim.uart_send(&bytes);
                    return;
                }
                KeyCode::Backspace => {
                    self.serial_input.pop();
                    return;
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.serial_input.push(c);
                    return;
                }
                _ => {}
            }
        }

        let page = 16usize;
        match key.code {
            KeyCode::Tab => self.cycle_focus(true),
            KeyCode::BackTab => self.cycle_focus(false),
            KeyCode::Esc => {
                if self.running {
                    self.toggle_run();
                } else if self.focus == Focus::Serial {
                    self.focus = Focus::Source;
                } else {
                    self.mode = Mode::Edit;
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.speed = (self.speed + 1).min(SPEEDS.len() - 1)
            }
            KeyCode::Char('-') => self.speed = self.speed.saturating_sub(1),
            KeyCode::Char('?') => self.show_help = true,
            // Letter aliases for terminals that swallow function keys.
            KeyCode::Char('r') => self.toggle_run(),
            KeyCode::Char('s') => self.on_key(KeyEvent::from(KeyCode::F(11))),
            KeyCode::Char('n') => self.on_key(KeyEvent::from(KeyCode::F(10))),
            KeyCode::Char('o') => self.on_key(KeyEvent::from(KeyCode::F(12))),
            KeyCode::Char('g') => self.run_to_cursor(),
            KeyCode::Char('b') => self.toggle_breakpoint(),
            KeyCode::Char('x') => self.reset(),
            KeyCode::Char('e') => self.mode = Mode::Edit,
            KeyCode::Char('i') => self.set_mem_tab(MemTab::Iram),
            KeyCode::Char('f') => self.set_mem_tab(MemTab::Sfr),
            KeyCode::Char('d') => self.set_mem_tab(MemTab::Xram),
            KeyCode::Char('c') => self.set_mem_tab(MemTab::Code),
            KeyCode::Char('p') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Poke,
                    input: String::new(),
                });
            }
            _ => match self.focus {
                Focus::Source => self.move_src(key.code, page),
                Focus::Memory => self.move_mem(key.code, page),
                Focus::Ports => self.move_ports(key.code),
                Focus::Serial => {}
            },
        }
    }

    fn set_mem_tab(&mut self, tab: MemTab) {
        self.mem_tab = tab;
        self.mem_scroll = 0;
        self.focus = Focus::Memory;
    }

    fn cycle_focus(&mut self, fwd: bool) {
        const ORDER: [Focus; 4] = [Focus::Source, Focus::Memory, Focus::Ports, Focus::Serial];
        let i = ORDER.iter().position(|&f| f == self.focus).unwrap_or(0);
        let n = ORDER.len();
        self.focus = ORDER[if fwd { (i + 1) % n } else { (i + n - 1) % n }];
    }

    fn move_src(&mut self, code: KeyCode, page: usize) {
        let max = self
            .build
            .as_ref()
            .map_or(0, |b| b.source.len().saturating_sub(1));
        self.src_cursor = match code {
            KeyCode::Up => self.src_cursor.saturating_sub(1),
            KeyCode::Down => self.src_cursor + 1,
            KeyCode::PageUp => self.src_cursor.saturating_sub(page),
            KeyCode::PageDown => self.src_cursor + page,
            KeyCode::Home => 0,
            KeyCode::End => max,
            KeyCode::Char('.') => self.pc_line().unwrap_or(self.src_cursor),
            _ => self.src_cursor,
        }
        .min(max);
    }

    fn move_mem(&mut self, code: KeyCode, page: usize) {
        let rows = match self.mem_tab {
            MemTab::Iram => 16,
            MemTab::Sfr => 8,
            MemTab::Xram | MemTab::Code => 4096,
        };
        self.mem_scroll = match code {
            KeyCode::Up => self.mem_scroll.saturating_sub(1),
            KeyCode::Down => self.mem_scroll + 1,
            KeyCode::PageUp => self.mem_scroll.saturating_sub(page),
            KeyCode::PageDown => self.mem_scroll + page,
            KeyCode::Home => 0,
            KeyCode::End => rows,
            KeyCode::Left => {
                self.mem_tab = match self.mem_tab {
                    MemTab::Iram => MemTab::Code,
                    MemTab::Sfr => MemTab::Iram,
                    MemTab::Xram => MemTab::Sfr,
                    MemTab::Code => MemTab::Xram,
                };
                0
            }
            KeyCode::Right => {
                self.mem_tab = match self.mem_tab {
                    MemTab::Iram => MemTab::Sfr,
                    MemTab::Sfr => MemTab::Xram,
                    MemTab::Xram => MemTab::Code,
                    MemTab::Code => MemTab::Iram,
                };
                0
            }
            _ => self.mem_scroll,
        }
        .min(rows.saturating_sub(1));
    }

    fn move_ports(&mut self, code: KeyCode) {
        let (port, bit) = &mut self.port_sel;
        match code {
            KeyCode::Up => *port = port.saturating_sub(1),
            KeyCode::Down => *port = (*port + 1).min(3),
            // Bits are drawn MSB first, so Left moves towards bit 7.
            KeyCode::Left => *bit = (*bit + 1).min(7),
            KeyCode::Right => *bit = bit.saturating_sub(1),
            KeyCode::Char(' ') | KeyCode::Enter => {
                self.sim.pins[*port] ^= 1 << *bit;
            }
            _ => {}
        }
    }

    fn on_prompt_key(&mut self, key: KeyEvent) {
        let Some(p) = &mut self.prompt else { return };
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Backspace => {
                p.input.pop();
            }
            KeyCode::Char(c) => p.input.push(c),
            KeyCode::Enter => {
                let p = self.prompt.take().unwrap();
                let input = p.input.trim().to_string();
                if input.is_empty() {
                    return;
                }
                match p.kind {
                    PromptKind::Open => self.open(Path::new(&input)),
                    PromptKind::SaveAs => self.save(Some(PathBuf::from(input))),
                    PromptKind::Poke => self.poke(&input),
                }
            }
            _ => {}
        }
    }

    /// `addr value` → write `value` at `addr` in the current memory tab.
    fn poke(&mut self, input: &str) {
        let parse = |s: &str| {
            let s = s.trim_start_matches("0x").trim_end_matches(['h', 'H']);
            u32::from_str_radix(s, 16).ok()
        };
        let parts: Vec<&str> = input.split_whitespace().collect();
        let (Some(addr), Some(val)) = (
            parts.first().and_then(|s| parse(s)),
            parts.get(1).and_then(|s| parse(s)),
        ) else {
            self.status = "usage: <addr> <value> (hex), e.g. 30 FF".into();
            return;
        };
        if val > 0xFF {
            self.status = "value must be a byte".into();
            return;
        }
        let v = val as u8;
        let ok = match self.mem_tab {
            MemTab::Iram if addr <= 0xFF => {
                self.sim.iram[addr as usize] = v;
                true
            }
            MemTab::Sfr if (0x80..=0xFF).contains(&addr) => {
                self.sim.set_sfr(addr as u8, v);
                true
            }
            MemTab::Xram if addr <= 0xFFFF => {
                self.sim.xram[addr as usize] = v;
                true
            }
            MemTab::Code if addr <= 0xFFFF => {
                self.sim.code[addr as usize] = v;
                true
            }
            _ => false,
        };
        self.status = if ok {
            format!("wrote {v:02X}h at {addr:X}h")
        } else {
            "address out of range".into()
        };
    }
}

/// Command that opens `path` at `line` in the user's editor: `$EDITOR`,
/// then `$VISUAL`, then `vi`. Run through `sh` so values such as
/// `code --wait` work as they do for git.
fn editor_command(path: &Path, line: usize) -> Command {
    let editor = ["EDITOR", "VISUAL"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "vi".into());
    // Editors known to accept `+LINE` before the file name.
    let program = editor.split_whitespace().next().unwrap_or_default();
    let name = Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let goto = matches!(
        name,
        "vi" | "vim" | "nvim" | "nano" | "emacs" | "micro" | "kak" | "ne"
    )
    .then(|| format!("+{line}"));

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(format!("{editor} \"$@\"")).arg("sh");
    cmd.args(goto).arg(path);
    cmd
}

/// Builds an assembler source from a HEX image by disassembling each
/// contiguous run of bytes. Reassembling it reproduces the image exactly.
fn disassemble_image(bytes: &[(u16, u8)], path: &Path) -> String {
    let mut code = Box::new([0u8; 0x10000]);
    let mut used = vec![false; 0x10000];
    for &(a, b) in bytes {
        code[a as usize] = b;
        used[a as usize] = true;
    }
    let mut src = format!("; disassembled from {}\n", path.display());
    let mut addr = 0usize;
    while addr < 0x10000 {
        if !used[addr] {
            addr += 1;
            continue;
        }
        src.push_str(&format!(
            "\n        ORG {}\n",
            crate::sfr::hex16(addr as u16)
        ));
        while addr < 0x10000 && used[addr] {
            let d = disasm::disassemble(&code, addr as u16);
            let size = d.size as usize;
            if (addr..addr + size).all(|a| a < 0x10000 && used[a]) {
                src.push_str(&format!("        {:<24}; {:04X}\n", d.text, addr));
                addr += size;
            } else {
                src.push_str(&format!(
                    "        DB {:<21}; {:04X}\n",
                    crate::sfr::hex8(code[addr]),
                    addr
                ));
                addr += 1;
            }
        }
    }
    src.push_str("        END\n");
    src
}

const TEMPLATE: &str = "\
; New 8051 program. F7 assembles, F5 runs, F11 steps, F1 shows all keys.
        ORG 0000h
        LJMP main

        ORG 0030h
main:   MOV A,#01h          ; walking LED pattern on P1
loop:   MOV P1,A
        RL A
        SJMP loop

        END
";
