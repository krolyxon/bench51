//! The application: window chrome around a tree of docked panes, the run
//! loop, file handling, and every action the UI can ask for.

use std::path::{Path, PathBuf};

use bench51_core::files::{self, Loaded};
use eframe::egui::{
    self, CentralPanel, Context, Key, Modifiers, Panel, Shape, ThemePreference, Ui, ViewportCommand,
};
use egui_tiles::Tree;

use crate::layout::{self, Pane, PaneBehavior};
use crate::state::{Action, BuildTab, LogKind, LogLine, Modal, Mode, Prefs, State, ThemePref};
use crate::theme::{self, palette};
use crate::{chrome, modals};

/// One simulation chunk, as in the TUI's frame loop.
const TICK: f64 = 1.0 / 30.0;

const SOURCE_EXTENSIONS: [&str; 4] = ["asm", "a51", "s", "src"];
const HEX_EXTENSIONS: [&str; 2] = ["hex", "ihx"];

const TEMPLATE: &str = "\
; New 8051 program. F7 builds, Ctrl+F5 starts debugging, F11 steps.
        ORG 0000h
        LJMP main

        ORG 0030h
main:   MOV A,#01h          ; walking LED pattern on P1
loop:   MOV P1,A
        RL A
        SJMP loop

        END
";

pub struct GuiApp {
    st: State,
    edit_tree: Tree<Pane>,
    debug_tree: Tree<Pane>,
    last_time: Option<f64>,
    tick_debt: f64,
    /// The user confirmed quitting with unsaved changes.
    allow_close: bool,
    title: String,
}

fn theme_preference(t: ThemePref) -> ThemePreference {
    match t {
        ThemePref::System => ThemePreference::System,
        ThemePref::Dark => ThemePreference::Dark,
        ThemePref::Light => ThemePreference::Light,
    }
}

fn load_tree(
    cc: &eframe::CreationContext<'_>,
    key: &str,
    default: fn() -> Tree<Pane>,
) -> Tree<Pane> {
    let mut tree = cc
        .storage
        .and_then(|s| eframe::get_value::<Tree<Pane>>(s, key))
        .filter(|t| t.tiles.find_pane(&Pane::Editor).is_some())
        .unwrap_or_else(default);
    // Panes added in newer versions are simply missing; nothing to fix up.
    if tree.root.is_none() {
        tree = default();
    }
    tree
}

impl GuiApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        files: Vec<PathBuf>,
        clock: Option<f64>,
        theme: Option<ThemePref>,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        theme::install(ctx);
        let mut prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, "prefs"))
            .unwrap_or_default();
        if let Some(c) = clock {
            prefs.clock_mhz = c;
        }
        if let Some(t) = theme {
            prefs.theme = t;
        }
        prefs.speed = prefs.speed.min(bench51_core::session::SPEEDS.len() - 1);
        ctx.set_theme(theme_preference(prefs.theme));
        ctx.set_zoom_factor(prefs.zoom.clamp(0.5, 3.0));
        let mut app = Self {
            st: State::new(prefs),
            edit_tree: load_tree(cc, "edit_layout", layout::edit_tree),
            debug_tree: load_tree(cc, "debug_layout", layout::debug_tree),
            last_time: None,
            tick_debt: 0.0,
            allow_close: false,
            title: String::new(),
        };
        for f in files {
            app.open_path(&f);
        }
        app
    }

    fn tree(&mut self) -> &mut Tree<Pane> {
        match self.st.mode {
            Mode::Edit => &mut self.edit_tree,
            Mode::Debug => &mut self.debug_tree,
        }
    }

    // ---- Input ----

    fn shortcuts(&mut self, ctx: &Context) {
        if self.st.modal.is_some() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let none = Modifiers::NONE;
        let running = self.st.running();
        let active = self.st.active;
        // Most specific first: `consume_key` ignores extra Shift.
        let table: [(Modifiers, Key, Action); 25] = [
            (cmd_shift, Key::S, Action::SaveAs),
            (cmd, Key::S, Action::Save),
            (none, Key::F2, Action::Save),
            (cmd, Key::N, Action::New),
            (cmd, Key::O, Action::Open),
            (cmd, Key::W, Action::Close(active)),
            (cmd, Key::Q, Action::Quit),
            (cmd, Key::F, Action::Find),
            (none, Key::F1, Action::Modal(Modal::Shortcuts)),
            (none, Key::F7, Action::Build),
            (cmd, Key::F5, Action::ToggleDebug),
            (none, Key::F4, Action::ToggleDebug),
            (none, Key::F5, Action::RunPause),
            (cmd, Key::F11, Action::StepOut),
            (none, Key::F12, Action::StepOut),
            (none, Key::F11, Action::StepInto),
            (cmd, Key::F10, Action::RunToCursor),
            (none, Key::F10, Action::StepOver),
            (none, Key::F8, Action::RunToCursor),
            (cmd_shift, Key::F9, Action::ClearBreakpoints),
            (none, Key::F9, Action::ToggleBreakpoint),
            (none, Key::F6, Action::Reset),
            (cmd, Key::Plus, Action::Zoom(0.1)),
            (cmd, Key::Minus, Action::Zoom(-0.1)),
            (cmd, Key::Num0, Action::ZoomReset),
        ];
        ctx.input_mut(|i| {
            for (mods, key, action) in table {
                if i.consume_key(mods, key) {
                    self.st.actions.push(action);
                }
            }
            if running && i.consume_key(none, Key::Escape) {
                self.st.actions.push(Action::RunPause);
            }
        });
    }

    fn handle_drops(&mut self, ctx: &Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            self.open_path(&path);
        }
    }

    fn handle_close(&mut self, ctx: &Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.st.docs.iter().any(|d| d.dirty())
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.st.modal = Some(Modal::ConfirmQuit);
        }
    }

    /// Advances the running program in fixed 33 ms chunks of wall time.
    fn pace(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let dt = self.last_time.map_or(0.0, |t| now - t);
        self.last_time = Some(now);
        let speed = self.st.prefs.speed;
        let Some(doc) = self.st.doc_mut() else {
            return;
        };
        doc.session.speed = speed;
        if !doc.session.running {
            self.tick_debt = 0.0;
            return;
        }
        self.tick_debt = (self.tick_debt + dt).min(TICK * 3.0);
        while self.tick_debt >= TICK && doc.session.running {
            doc.session.tick();
            self.tick_debt -= TICK;
        }
        if !doc.session.running || speed <= 2 {
            doc.follow_pc = true;
        }
        if doc.session.running {
            ctx.request_repaint();
        }
    }

    // ---- Files ----

    fn open_path(&mut self, path: &Path) {
        let canon = path.canonicalize().ok();
        if let Some(i) = self.st.docs.iter().position(|d| {
            d.path.as_ref().and_then(|p| p.canonicalize().ok()) == canon && canon.is_some()
        }) {
            self.activate(i);
            return;
        }
        // An untouched new document is replaced, as editors usually do.
        if self.st.docs.len() == 1 {
            let d = &self.st.docs[0];
            if d.path.is_none() && d.label.is_none() && !d.dirty() && d.text == TEMPLATE {
                self.st.docs.clear();
            }
        }
        match files::load(path) {
            Err(e) => {
                self.st.log(LogKind::Error, e.clone());
                if let Some(d) = self.st.doc_mut() {
                    d.session.status = e;
                }
            }
            Ok(Loaded::Source(text)) => {
                self.pause_active();
                let doc = self.st.add_doc(Some(path.to_path_buf()), &text);
                doc.session.status = format!("opened {}", path.display());
                self.st.prefs.add_recent(path);
                self.st
                    .log(LogKind::Info, format!("opened {}", path.display()));
            }
            Ok(Loaded::Hex { source, bytes }) => {
                self.pause_active();
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                let doc = self.st.add_doc(None, &source);
                doc.label = name;
                // Not saved anywhere yet: Save asks for an .asm name.
                doc.saved.clear();
                self.st.prefs.add_recent(path);
                self.build();
                if let Some(d) = self.st.doc_mut() {
                    d.session.status = format!(
                        "loaded {} ({bytes} bytes) and disassembled it",
                        path.display()
                    );
                }
            }
        }
    }

    fn start_dir(&self) -> Option<PathBuf> {
        self.st
            .doc()
            .and_then(|d| d.path.as_ref())
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| {
                self.st
                    .prefs
                    .recent
                    .first()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
            })
            .or_else(|| std::env::current_dir().ok())
    }

    fn open_dialog(&mut self) {
        let mut all: Vec<&str> = SOURCE_EXTENSIONS.to_vec();
        all.extend(HEX_EXTENSIONS);
        let mut dialog = rfd::FileDialog::new()
            .set_title("Open")
            .add_filter("8051 source or Intel HEX", &all)
            .add_filter("8051 assembly", &SOURCE_EXTENSIONS)
            .add_filter("Intel HEX", &HEX_EXTENSIONS)
            .add_filter("All files", &["*"]);
        if let Some(dir) = self.start_dir() {
            dialog = dialog.set_directory(dir);
        }
        for path in dialog.pick_files().unwrap_or_default() {
            self.open_path(&path);
        }
    }

    fn save(&mut self, ask: bool) {
        let dir = self.start_dir();
        let Some(doc) = self.st.doc_mut() else {
            return;
        };
        let path = match (&doc.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let name = doc.name();
                let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
                let mut dialog = rfd::FileDialog::new()
                    .set_title("Save as")
                    .add_filter("8051 assembly", &SOURCE_EXTENSIONS)
                    .set_file_name(format!("{stem}.asm"));
                if let Some(dir) = dir {
                    dialog = dialog.set_directory(dir);
                }
                dialog.save_file()
            }
        };
        let Some(path) = path else {
            return;
        };
        let mut contents = doc.text.clone();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        match std::fs::write(&path, contents) {
            Ok(()) => {
                doc.saved = doc.text.clone();
                doc.session.status = format!("saved {}", path.display());
                doc.path = Some(path.clone());
                doc.label = None;
                self.st.prefs.add_recent(&path);
            }
            Err(e) => {
                let msg = format!("cannot save {}: {e}", path.display());
                doc.session.status = msg.clone();
                self.st.log(LogKind::Error, msg);
            }
        }
    }

    fn close(&mut self, i: usize) {
        if i >= self.st.docs.len() {
            return;
        }
        self.st.docs.remove(i);
        if self.st.active >= i && self.st.active > 0 {
            self.st.active -= 1;
        }
        if self.st.docs.is_empty() {
            self.st.mode = Mode::Edit;
            self.st.active = 0;
        }
    }

    fn pause_active(&mut self) {
        if let Some(d) = self.st.doc_mut()
            && d.session.running
        {
            d.session.pause();
        }
    }

    fn activate(&mut self, i: usize) {
        if i < self.st.docs.len() && i != self.st.active {
            self.pause_active();
            self.st.active = i;
        }
    }

    // ---- Build & debug ----

    /// Assembles the active document and reports Keil-style in the Build
    /// Output. Returns whether it built without errors.
    fn build(&mut self) -> bool {
        let Some(doc) = self.st.docs.get_mut(self.st.active) else {
            return false;
        };
        let (name, target, id) = (doc.name(), doc.target(), doc.id);
        let ok = doc.session.assemble(&doc.text, doc.path.as_deref());
        let mut lines = vec![
            (LogKind::Info, format!("Build target '{target}'"), None),
            (LogKind::Info, format!("assembling {name}..."), None),
        ];
        let out = &doc.session.last_asm.as_ref().unwrap().1;
        if ok {
            lines.push((
                LogKind::Info,
                format!("Program Size: code={} bytes", out.code_size()),
                None,
            ));
            if doc.path.is_some() {
                lines.push((
                    LogKind::Info,
                    format!("creating hex file from \"{target}\"..."),
                    None,
                ));
            }
            if doc.session.status.contains("failed") {
                lines.push((LogKind::Error, doc.session.status.clone(), None));
            }
            lines.push((LogKind::Ok, format!("\"{target}\" - 0 Error(s)"), None));
            doc.follow_pc = true;
        } else {
            for d in &out.diagnostics {
                lines.push((
                    LogKind::Error,
                    format!("{name}({}:{}): error: {}", d.line + 1, d.col + 1, d.msg),
                    Some((id, d.line, d.col)),
                ));
            }
            lines.push((
                LogKind::Error,
                format!("\"{target}\" - {} Error(s)", out.diagnostics.len()),
                None,
            ));
            let first = &out.diagnostics[0];
            doc.goto = Some((first.line, first.col));
        }
        for (kind, text, target) in lines {
            self.st.log.push(LogLine { text, kind, target });
        }
        self.st.log(LogKind::Info, "");
        if !ok {
            self.st.mode = Mode::Edit;
            self.st.build_tab = BuildTab::Messages;
            layout::show_pane(&mut self.edit_tree, Pane::Build);
        }
        ok
    }

    fn ensure_built(&mut self) -> bool {
        match self.st.doc() {
            None => false,
            Some(d) if d.stale() => self.build(),
            Some(_) => true,
        }
    }

    /// Runs a debugger command on the active document, building first and
    /// entering the debug layout, as Keil does.
    fn debug_cmd(&mut self, f: impl FnOnce(&mut bench51_core::session::Session, (usize, usize))) {
        if !self.ensure_built() {
            return;
        }
        self.st.mode = Mode::Debug;
        if let Some(doc) = self.st.doc_mut() {
            f(&mut doc.session, doc.cursor);
            doc.follow_pc = true;
        }
    }

    fn apply(&mut self, ctx: &Context, action: Action) {
        match action {
            Action::New => {
                self.pause_active();
                self.st.add_doc(None, TEMPLATE);
            }
            Action::Open => self.open_dialog(),
            Action::OpenPath(p) => self.open_path(&p),
            Action::Save => self.save(false),
            Action::SaveAs => self.save(true),
            Action::Close(i) => {
                if self.st.docs.get(i).is_some_and(|d| d.dirty()) {
                    self.st.modal = Some(Modal::ConfirmClose(i));
                } else {
                    self.close(i);
                }
            }
            Action::ForceClose(i) => self.close(i),
            Action::Activate(i) => self.activate(i),
            Action::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            Action::ForceQuit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Action::Build => {
                if self.build() && self.st.mode == Mode::Edit {
                    self.st.build_tab = BuildTab::Messages;
                }
            }
            Action::ToggleDebug => match self.st.mode {
                Mode::Edit => {
                    if self.ensure_built() {
                        self.st.mode = Mode::Debug;
                        if let Some(d) = self.st.doc_mut() {
                            d.follow_pc = true;
                        }
                    }
                }
                Mode::Debug => {
                    self.pause_active();
                    self.st.mode = Mode::Edit;
                }
            },
            Action::RunPause => {
                if self.st.running() {
                    self.pause_active();
                    if let Some(d) = self.st.doc_mut() {
                        d.follow_pc = true;
                    }
                } else {
                    self.debug_cmd(|s, _| s.start());
                }
            }
            Action::StepInto => self.debug_cmd(|s, _| s.step_into()),
            Action::StepOver => self.debug_cmd(|s, _| s.step_over()),
            Action::StepOut => self.debug_cmd(|s, _| s.step_out()),
            Action::RunToCursor => self.debug_cmd(|s, (line, _)| {
                s.run_to_line(line);
            }),
            Action::Reset => {
                if let Some(d) = self.st.doc_mut() {
                    d.session.reset();
                    d.follow_pc = true;
                }
            }
            Action::ToggleBreakpoint => {
                if let Some(d) = self.st.doc_mut() {
                    d.session.toggle_breakpoint(d.cursor.0);
                }
            }
            Action::ClearBreakpoints => {
                if let Some(d) = self.st.doc_mut() {
                    d.session.breakpoints.clear();
                    d.session.status = "all breakpoints removed".into();
                }
            }
            Action::SetSpeed(i) => self.st.prefs.speed = i,
            Action::SetClock(mhz) => {
                self.st.prefs.clock_mhz = mhz;
                for d in &mut self.st.docs {
                    d.session.clock_mhz = mhz;
                }
            }
            Action::Goto { doc, line, col } => {
                if let Some(i) = self.st.docs.iter().position(|d| d.id == doc) {
                    self.activate(i);
                    self.st.docs[i].goto = Some((line, col));
                    layout::show_pane(self.tree(), Pane::Editor);
                }
            }
            Action::ShowPane(p) => layout::show_pane(self.tree(), p),
            Action::ResetLayout => {
                *self.tree() = match self.st.mode {
                    Mode::Edit => layout::edit_tree(),
                    Mode::Debug => layout::debug_tree(),
                }
            }
            Action::SetTheme(t) => {
                self.st.prefs.theme = t;
                ctx.set_theme(theme_preference(t));
            }
            Action::Zoom(d) => ctx.set_zoom_factor((ctx.zoom_factor() + d).clamp(0.5, 3.0)),
            Action::ZoomReset => ctx.set_zoom_factor(1.0),
            Action::Find => {
                if self.st.doc().is_some() {
                    self.st.find.open = true;
                    self.st.find.focus = true;
                }
            }
            Action::Modal(m) => self.st.modal = Some(m),
        }
    }

    fn update_title(&mut self, ctx: &Context) {
        let title = match self.st.doc() {
            Some(d) => format!(
                "{}{} — bench51",
                d.name(),
                if d.dirty() { " •" } else { "" }
            ),
            None => "bench51".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = palette(&ctx);
        self.shortcuts(&ctx);
        self.handle_drops(&ctx);
        self.handle_close(&ctx);
        self.pace(&ctx);
        self.st.prefs.zoom = ctx.zoom_factor();

        let bar = |top: i8, bottom: i8| {
            egui::Frame::NONE.fill(p.black).inner_margin(egui::Margin {
                left: 10,
                right: 10,
                top,
                bottom,
            })
        };
        Panel::top("menu")
            .frame(bar(4, 0))
            .show_separator_line(false)
            .show(ui, |ui| chrome::menu_bar(ui, &mut self.st));
        Panel::top("toolbar")
            .frame(bar(4, 6))
            .show_separator_line(false)
            .show(ui, |ui| chrome::toolbar(ui, &mut self.st));
        Panel::bottom("status")
            .frame(bar(6, 6))
            .show_separator_line(false)
            .show(ui, |ui| chrome::status_bar(ui, &self.st));

        CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(p.black)
                    .inner_margin(egui::Margin::symmetric(6, 0)),
            )
            .show(ui, |ui| {
                let tree = match self.st.mode {
                    Mode::Edit => &mut self.edit_tree,
                    Mode::Debug => &mut self.debug_tree,
                };
                let cards = ui.painter().add(Shape::Noop);
                let editor_tile = tree.tiles.find_pane(&Pane::Editor);
                let mut behavior = PaneBehavior {
                    st: &mut self.st,
                    editor_tile,
                };
                tree.ui(&mut behavior, ui);
                ui.painter()
                    .set(cards, Shape::Vec(layout::card_shapes(tree, p.surface)));
            });

        modals::show(&ctx, &mut self.st);
        for action in std::mem::take(&mut self.st.actions) {
            self.apply(&ctx, action);
        }
        self.update_title(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "prefs", &self.st.prefs);
        eframe::set_value(storage, "edit_layout", &self.edit_tree);
        eframe::set_value(storage, "debug_layout", &self.debug_tree);
    }

    /// Layouts, theme and zoom are saved above. egui's own memory would
    /// restore scroll positions keyed by document number onto whatever file
    /// gets that number next session.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}
