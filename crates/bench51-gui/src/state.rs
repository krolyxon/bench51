//! Application state shared by every pane, and the actions panes ask for.

use std::path::{Path, PathBuf};

use bench51_core::session::{MemSpace, Session};
use serde::{Deserialize, Serialize};

use crate::layout::Pane;

/// Keil's two window layouts: building and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Edit,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum ThemePref {
    System,
    Dark,
    Light,
}

/// Settings that survive restarts.
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemePref,
    pub recent: Vec<PathBuf>,
    pub speed: usize,
    pub clock_mhz: f64,
    pub zoom: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: ThemePref::System,
            recent: Vec::new(),
            speed: 5,
            clock_mhz: 11.0592,
            zoom: 1.0,
        }
    }
}

impl Prefs {
    pub fn add_recent(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path);
        self.recent.truncate(10);
    }
}

/// An open source file and the simulator running it.
pub struct Document {
    /// Stable id for egui widget state.
    pub id: u64,
    pub path: Option<PathBuf>,
    /// Name shown for documents without a path (e.g. a disassembled HEX file).
    pub label: Option<String>,
    pub text: String,
    pub saved: String,
    pub session: Session,
    /// Cursor position as (line, column), updated by the editor.
    pub cursor: (usize, usize),
    /// Cursor position as a char index into `text`.
    pub cursor_char: usize,
    /// Move the cursor here and scroll to it on the next frame.
    pub goto: Option<(usize, usize)>,
    /// Select this char range and scroll to it on the next frame, keeping
    /// keyboard focus where it is (the find bar).
    pub select: Option<(usize, usize)>,
    /// Byte range of the current find match.
    pub find_hit: Option<(usize, usize)>,
    /// Scroll the editor and disassembly to the PC on the next frame.
    pub follow_pc: bool,
}

impl Document {
    pub fn new(id: u64, path: Option<PathBuf>, text: &str, clock_mhz: f64) -> Self {
        let text = text.replace("\r\n", "\n");
        Self {
            id,
            path,
            label: None,
            saved: text.clone(),
            text,
            session: Session::new(clock_mhz),
            cursor: (0, 0),
            cursor_char: 0,
            goto: None,
            select: None,
            find_hit: None,
            follow_pc: false,
        }
    }

    pub fn name(&self) -> String {
        if let Some(name) = self.path.as_ref().and_then(|p| p.file_name()) {
            return name.to_string_lossy().into_owned();
        }
        self.label.clone().unwrap_or_else(|| "untitled.asm".into())
    }

    /// File name without extension, Keil's "target" name.
    pub fn target(&self) -> String {
        let name = self.name();
        match name.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem.to_string(),
            _ => name,
        }
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// The editor text differs from the program in the simulator.
    pub fn stale(&self) -> bool {
        self.session.is_stale(self.text.split('\n'))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Ok,
    Error,
}

/// A line in the Build Output messages.
pub struct LogLine {
    pub text: String,
    pub kind: LogKind,
    /// Clicking the line jumps here: (document id, line, column).
    pub target: Option<(u64, usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTab {
    Messages,
    Listing,
    Hex,
}

/// Everything the menus, toolbar, shortcuts and panes can ask for. They
/// queue actions; `GuiApp::apply` runs them once the frame's UI is done.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    New,
    Open,
    OpenPath(PathBuf),
    Save,
    SaveAs,
    /// Close a document, asking first if it has unsaved changes.
    Close(usize),
    /// Close without asking.
    ForceClose(usize),
    Activate(usize),
    Quit,
    /// Quit even with unsaved changes.
    ForceQuit,
    Build,
    ToggleDebug,
    RunPause,
    StepInto,
    StepOver,
    StepOut,
    RunToCursor,
    Reset,
    ToggleBreakpoint,
    ClearBreakpoints,
    SetSpeed(usize),
    SetClock(f64),
    Goto {
        doc: u64,
        line: usize,
        col: usize,
    },
    ShowPane(Pane),
    ResetLayout,
    SetTheme(ThemePref),
    Zoom(f32),
    ZoomReset,
    Find,
    Modal(Modal),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    /// Unsaved changes in this document.
    ConfirmClose(usize),
    ConfirmQuit,
    Shortcuts,
    /// Options for Target: the oscillator frequency, being edited.
    Options(String),
    About,
}

pub struct MemView {
    pub space: MemSpace,
    pub goto: String,
    /// Scroll so this address is visible on the next frame.
    pub scroll_to: Option<u32>,
    /// Byte being edited inline: (address, text).
    pub edit: Option<(u32, String)>,
}

pub struct FindBar {
    pub open: bool,
    pub query: String,
    /// Give the query field keyboard focus on the next frame.
    pub focus: bool,
}

pub struct State {
    pub docs: Vec<Document>,
    pub active: usize,
    next_doc_id: u64,
    pub mode: Mode,
    pub prefs: Prefs,
    pub log: Vec<LogLine>,
    pub actions: Vec<Action>,
    pub modal: Option<Modal>,
    pub mem: MemView,
    pub build_tab: BuildTab,
    pub serial_input: String,
    pub find: FindBar,
}

impl State {
    pub fn new(prefs: Prefs) -> Self {
        Self {
            docs: Vec::new(),
            active: 0,
            next_doc_id: 1,
            mode: Mode::Edit,
            prefs,
            log: Vec::new(),
            actions: Vec::new(),
            modal: None,
            mem: MemView {
                space: MemSpace::Iram,
                goto: String::new(),
                scroll_to: None,
                edit: None,
            },
            build_tab: BuildTab::Messages,
            serial_input: String::new(),
            find: FindBar {
                open: false,
                query: String::new(),
                focus: false,
            },
        }
    }

    pub fn doc(&self) -> Option<&Document> {
        self.docs.get(self.active)
    }

    pub fn doc_mut(&mut self) -> Option<&mut Document> {
        self.docs.get_mut(self.active)
    }

    pub fn add_doc(&mut self, path: Option<PathBuf>, text: &str) -> &mut Document {
        let id = self.next_doc_id;
        self.next_doc_id += 1;
        let mut doc = Document::new(id, path, text, self.prefs.clock_mhz);
        doc.session.speed = self.prefs.speed;
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
        self.docs.last_mut().unwrap()
    }

    pub fn log(&mut self, kind: LogKind, text: impl Into<String>) {
        self.log.push(LogLine {
            text: text.into(),
            kind,
            target: None,
        });
        if self.log.len() > 2000 {
            self.log.drain(..500);
        }
    }

    /// The active document's simulator is free running.
    pub fn running(&self) -> bool {
        self.doc().is_some_and(|d| d.session.running)
    }
}
