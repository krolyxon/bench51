//! Window chrome: menu bar, toolbar and status bar. They sit directly on
//! the page, without boxes.

use bench51_core::session::SPEEDS;
use eframe::egui::{self, Align, Button, FontId, Layout, RichText, Stroke, Ui};

use crate::icons::Icon;
use crate::layout::Pane;
use crate::state::{Action, Modal, Mode, State, ThemePref};
use crate::theme::{ACCENT, LABEL, LABEL_TRACKING, palette};
use crate::widgets;

fn item(ui: &mut Ui, text: &str, shortcut: &str) -> bool {
    let p = palette(ui.ctx());
    let label = RichText::new(text.to_uppercase())
        .font(FontId::monospace(12.0))
        .extra_letter_spacing(0.5)
        .color(p.text_primary);
    let mut b = Button::new(label).min_size(egui::vec2(240.0, 24.0));
    if !shortcut.is_empty() {
        b = b.shortcut_text(
            RichText::new(shortcut)
                .font(FontId::monospace(11.0))
                .color(p.text_disabled),
        );
    }
    ui.add(b).clicked()
}

fn check(ui: &mut Ui, text: &str, on: bool) -> bool {
    let mark = if on { "●  " } else { "    " };
    item(ui, &format!("{mark}{text}"), "")
}

fn title(ui: &Ui, text: &str) -> RichText {
    let p = palette(ui.ctx());
    RichText::new(text.to_uppercase())
        .font(FontId::monospace(LABEL))
        .extra_letter_spacing(LABEL_TRACKING)
        .color(p.text_secondary)
}

pub fn menu_bar(ui: &mut Ui, st: &mut State) {
    let mut acts = Vec::new();
    let has_doc = st.doc().is_some();
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button(title(ui, "File"), |ui| {
            if item(ui, "New", "Ctrl+N") {
                acts.push(Action::New);
            }
            if item(ui, "Open…", "Ctrl+O") {
                acts.push(Action::Open);
            }
            ui.menu_button(title(ui, "Open recent"), |ui| {
                if st.prefs.recent.is_empty() {
                    ui.label(palette(ui.ctx()).label("none"));
                }
                for path in &st.prefs.recent {
                    let name = path.display().to_string();
                    if ui
                        .button(RichText::new(name).font(FontId::monospace(12.0)))
                        .clicked()
                    {
                        acts.push(Action::OpenPath(path.clone()));
                    }
                }
            });
            ui.separator();
            if item(ui, "Save", "Ctrl+S") {
                acts.push(Action::Save);
            }
            if item(ui, "Save as…", "Ctrl+Shift+S") {
                acts.push(Action::SaveAs);
            }
            if has_doc && item(ui, "Close file", "Ctrl+W") {
                acts.push(Action::Close(st.active));
            }
            ui.separator();
            if item(ui, "Quit", "Ctrl+Q") {
                acts.push(Action::Quit);
            }
        });
        ui.menu_button(title(ui, "Edit"), |ui| {
            if item(ui, "Find…", "Ctrl+F") {
                acts.push(Action::Find);
            }
            ui.separator();
            if item(ui, "Toggle breakpoint", "F9") {
                acts.push(Action::ToggleBreakpoint);
            }
            if item(ui, "Kill all breakpoints", "Ctrl+Shift+F9") {
                acts.push(Action::ClearBreakpoints);
            }
        });
        ui.menu_button(title(ui, "View"), |ui| {
            for pane in Pane::ALL {
                if item(ui, pane.title(), "") {
                    acts.push(Action::ShowPane(pane));
                }
            }
            ui.separator();
            ui.menu_button(title(ui, "Theme"), |ui| {
                for (t, name) in [
                    (ThemePref::System, "System"),
                    (ThemePref::Dark, "Dark"),
                    (ThemePref::Light, "Light"),
                ] {
                    if check(ui, name, st.prefs.theme == t) {
                        acts.push(Action::SetTheme(t));
                    }
                }
            });
            if item(ui, "Zoom in", "Ctrl++") {
                acts.push(Action::Zoom(0.1));
            }
            if item(ui, "Zoom out", "Ctrl+-") {
                acts.push(Action::Zoom(-0.1));
            }
            if item(ui, "Actual size", "Ctrl+0") {
                acts.push(Action::ZoomReset);
            }
            ui.separator();
            if item(ui, "Reset layout", "") {
                acts.push(Action::ResetLayout);
            }
        });
        ui.menu_button(title(ui, "Project"), |ui| {
            if item(ui, "Build target", "F7") {
                acts.push(Action::Build);
            }
            ui.separator();
            if item(ui, "Options for target…", "") {
                acts.push(Action::Modal(Modal::Options(
                    st.prefs.clock_mhz.to_string(),
                )));
            }
        });
        ui.menu_button(title(ui, "Debug"), |ui| {
            let session = if st.mode == Mode::Debug {
                "Stop debug session"
            } else {
                "Start debug session"
            };
            if item(ui, session, "Ctrl+F5") {
                acts.push(Action::ToggleDebug);
            }
            ui.separator();
            let run = if st.running() { "Stop" } else { "Run" };
            if item(ui, run, "F5") {
                acts.push(Action::RunPause);
            }
            if item(ui, "Step into", "F11") {
                acts.push(Action::StepInto);
            }
            if item(ui, "Step over", "F10") {
                acts.push(Action::StepOver);
            }
            if item(ui, "Step out", "Ctrl+F11") {
                acts.push(Action::StepOut);
            }
            if item(ui, "Run to cursor line", "Ctrl+F10") {
                acts.push(Action::RunToCursor);
            }
            if item(ui, "Reset CPU", "F6") {
                acts.push(Action::Reset);
            }
            ui.separator();
            ui.menu_button(title(ui, "Speed"), |ui| {
                for (i, n) in SPEEDS.iter().enumerate() {
                    if check(ui, &format!("{n} / tick"), st.prefs.speed == i) {
                        acts.push(Action::SetSpeed(i));
                    }
                }
            });
        });
        ui.menu_button(title(ui, "Peripherals"), |ui| {
            for (pane, name) in [
                (Pane::Ports, "I/O ports"),
                (Pane::Timers, "Timers"),
                (Pane::Interrupts, "Interrupts"),
                (Pane::Serial, "Serial (UART #1)"),
            ] {
                if item(ui, name, "") {
                    acts.push(Action::ShowPane(pane));
                }
            }
        });
        ui.menu_button(title(ui, "Help"), |ui| {
            if item(ui, "Keyboard shortcuts", "F1") {
                acts.push(Action::Modal(Modal::Shortcuts));
            }
            if item(ui, "About bench51", "") {
                acts.push(Action::Modal(Modal::About));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let p = palette(ui.ctx());
            ui.label(title(ui, "bench51").color(p.text_disabled));
        });
    });
    st.actions.extend(acts);
}

pub fn toolbar(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let has_doc = st.doc().is_some();
    let running = st.running();
    let debug = st.mode == Mode::Debug;
    let mut acts = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let mut b = |ui: &mut Ui, icon, tip, enabled, on, act| {
            if widgets::icon_button(ui, icon, tip, enabled, on).clicked() {
                acts.push(act);
            }
        };
        b(ui, Icon::New, "New file (Ctrl+N)", true, false, Action::New);
        b(ui, Icon::Open, "Open (Ctrl+O)", true, false, Action::Open);
        b(
            ui,
            Icon::Save,
            "Save (Ctrl+S)",
            has_doc,
            false,
            Action::Save,
        );
        ui.add_space(18.0);
        b(
            ui,
            Icon::Build,
            "Build target (F7)",
            has_doc,
            false,
            Action::Build,
        );
        ui.add_space(18.0);
        b(
            ui,
            Icon::Debug,
            if debug {
                "Stop debug session (Ctrl+F5)"
            } else {
                "Start debug session (Ctrl+F5)"
            },
            has_doc,
            debug,
            Action::ToggleDebug,
        );
        ui.add_space(18.0);
        b(
            ui,
            Icon::Reset,
            "Reset CPU (F6)",
            has_doc,
            false,
            Action::Reset,
        );
        if running {
            b(
                ui,
                Icon::Pause,
                "Stop (F5)",
                has_doc,
                false,
                Action::RunPause,
            );
        } else {
            b(ui, Icon::Run, "Run (F5)", has_doc, false, Action::RunPause);
        }
        b(
            ui,
            Icon::StepInto,
            "Step into (F11)",
            has_doc,
            false,
            Action::StepInto,
        );
        b(
            ui,
            Icon::StepOver,
            "Step over (F10)",
            has_doc,
            false,
            Action::StepOver,
        );
        b(
            ui,
            Icon::StepOut,
            "Step out (Ctrl+F11)",
            has_doc,
            false,
            Action::StepOut,
        );
        b(
            ui,
            Icon::RunToCursor,
            "Run to cursor line (Ctrl+F10)",
            has_doc,
            false,
            Action::RunToCursor,
        );
        ui.add_space(18.0);
        b(
            ui,
            Icon::Breakpoint,
            "Toggle breakpoint (F9)",
            has_doc,
            false,
            Action::ToggleBreakpoint,
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add_space(6.0);
            let speed = st.prefs.speed;
            ui.label(
                RichText::new(format!("{:>6} / TICK", SPEEDS[speed]))
                    .font(FontId::monospace(LABEL))
                    .color(p.text_secondary),
            );
            if let Some(i) = widgets::segment_bar(
                ui,
                136.0,
                10.0,
                SPEEDS.len(),
                speed + 1,
                p.text_display,
                true,
            ) {
                acts.push(Action::SetSpeed(i));
            }
            ui.label(p.label("Speed"));
        });
    });
    st.actions.extend(acts);
}

/// True for status messages that report a failure.
fn is_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    [
        "error(s)",
        "cannot ",
        "invalid ",
        "failed",
        "reserved opcode",
        "out of range",
    ]
    .iter()
    .any(|w| m.contains(w))
}

pub fn status_bar(ui: &mut Ui, st: &State) {
    let p = palette(ui.ctx());
    let text = |s: String| {
        RichText::new(s.to_uppercase())
            .font(FontId::monospace(LABEL))
            .extra_letter_spacing(0.6)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let doc = st.doc();
        let (state, live) = match doc {
            None => ("no file", false),
            Some(d) if d.session.running => ("running", true),
            Some(d) if d.session.build.is_none() => ("not built", false),
            Some(d) if d.stale() => ("source changed", false),
            Some(_) if st.mode == Mode::Debug => ("stopped", false),
            Some(_) => ("ready", false),
        };
        let mode = if st.mode == Mode::Debug {
            "simulation"
        } else {
            "edit"
        };
        ui.label(text(mode.into()).color(p.text_disabled));
        // Running: a dot that blinks in hard 2 Hz steps.
        let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        let on = !live || (ui.input(|i| i.time) * 4.0) as i64 % 2 == 0;
        if live && on {
            ui.painter().circle_filled(r.center(), 3.5, p.text_display);
        } else {
            ui.painter()
                .circle_stroke(r.center(), 3.0, Stroke::new(1.0, p.text_disabled));
        }
        ui.label(text(format!("[{state}]")).color(if live {
            p.text_display
        } else {
            p.text_secondary
        }));
        if let Some(d) = doc {
            let msg = &d.session.status;
            if !msg.is_empty() {
                let color = if is_error(msg) {
                    ACCENT
                } else {
                    p.text_primary
                };
                let shown = if is_error(msg) {
                    format!("[error: {msg}]")
                } else {
                    msg.clone()
                };
                ui.label(text(shown).color(color));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(text(format!("{} MHz", st.prefs.clock_mhz)).color(p.text_disabled));
            if let Some(d) = doc {
                if d.session.build.is_some() {
                    ui.label(text(format!("cyc {}", d.session.sim.cycles)).color(p.text_secondary));
                    ui.label(text(format!("t {}", d.session.elapsed())).color(p.text_secondary));
                }
                ui.label(
                    text(format!("l {}  c {}", d.cursor.0 + 1, d.cursor.1 + 1))
                        .color(p.text_secondary),
                );
            }
        });
    });
}
