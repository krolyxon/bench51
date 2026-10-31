//! Dialogs: unsaved-changes confirmation, target options, shortcuts, about.

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, FontId, Key, Layout, RichText, Stroke, Ui,
};

use crate::state::{Action, Modal, State};
use crate::theme::{BODY, HERO, palette};
use crate::widgets::{self, PillKind};

const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "File",
        &[
            ("Ctrl+N", "new file"),
            ("Ctrl+O", "open .asm / .a51, or .hex (disassembled)"),
            ("Ctrl+S  F2", "save"),
            ("Ctrl+Shift+S", "save as"),
            ("Ctrl+W", "close file"),
            ("Ctrl+F", "find"),
        ],
    ),
    (
        "Build & debug",
        &[
            ("F7", "build target (writes .lst and .hex)"),
            ("Ctrl+F5  F4", "start / stop debug session"),
            ("F5", "run / stop"),
            ("F11", "step into"),
            ("F10", "step over"),
            ("Ctrl+F11  F12", "step out"),
            ("Ctrl+F10  F8", "run to cursor line"),
            ("F9", "toggle breakpoint"),
            ("Ctrl+Shift+F9", "kill all breakpoints"),
            ("F6", "reset CPU"),
            ("Esc", "stop running"),
        ],
    ),
    (
        "Windows",
        &[
            ("Ctrl+Tab", "next tab in the hovered group"),
            ("Ctrl+ +/-/0", "zoom"),
            ("double-click", "edit a memory byte"),
        ],
    ),
];

fn frame(ctx: &Context) -> egui::Frame {
    let p = crate::theme::palette(ctx);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border_visible))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(egui::Margin::same(24))
}

/// Title on the left, `[ X ]` close on the right.
fn header(ui: &mut Ui, title: &str) -> bool {
    let p = palette(ui.ctx());
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(p.label(title));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            close = widgets::pill(ui, "[ X ]", PillKind::Ghost).clicked();
        });
    });
    ui.add_space(12.0);
    close
}

pub fn show(ctx: &Context, st: &mut State) {
    let Some(modal) = st.modal.clone() else {
        return;
    };
    let mut close = false;
    let mut acts = Vec::new();
    let resp = egui::Modal::new(egui::Id::unique("modal"))
        .frame(frame(ctx))
        .backdrop_color(Color32::from_black_alpha(204))
        .show(ctx, |ui| {
            let p = palette(ui.ctx());
            ui.set_max_width(480.0);
            match &modal {
                Modal::ConfirmClose(i) => {
                    let name = st.docs.get(*i).map(|d| d.name()).unwrap_or_default();
                    close |= header(ui, "Unsaved changes");
                    ui.label(
                        RichText::new(format!("Save changes to {name} before closing?"))
                            .size(BODY)
                            .color(p.text_primary),
                    );
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        if widgets::pill(ui, "Save", PillKind::Primary).clicked() {
                            acts.push(Action::Activate(*i));
                            acts.push(Action::Save);
                            acts.push(Action::Close(*i));
                            close = true;
                        }
                        if widgets::pill(ui, "Discard", PillKind::Secondary).clicked() {
                            acts.push(Action::ForceClose(*i));
                            close = true;
                        }
                        close |= widgets::pill(ui, "Cancel", PillKind::Ghost).clicked();
                    });
                }
                Modal::ConfirmQuit => {
                    let n = st.docs.iter().filter(|d| d.dirty()).count();
                    close |= header(ui, "Unsaved changes");
                    ui.label(
                        RichText::new(format!(
                            "{n} file{} unsaved changes.",
                            if n == 1 { " has" } else { "s have" }
                        ))
                        .size(BODY)
                        .color(p.text_primary),
                    );
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        if widgets::pill(ui, "Quit without saving", PillKind::Secondary).clicked() {
                            acts.push(Action::ForceQuit);
                            close = true;
                        }
                        close |= widgets::pill(ui, "Cancel", PillKind::Ghost).clicked();
                    });
                }
                Modal::Options(text) => {
                    close |= header(ui, "Options for target");
                    let mut text = text.clone();
                    ui.label(p.label("Xtal (MHz)"));
                    ui.add_space(4.0);
                    let r = widgets::underline_input(ui, &mut text, "11.0592", 160.0);
                    if !r.has_focus() && !r.lost_focus() && text.is_empty() {
                        r.request_focus();
                    }
                    ui.add_space(4.0);
                    ui.label(
                        p.label("Used to turn machine cycles into elapsed time")
                            .color(p.text_disabled),
                    );
                    let valid = text.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
                    ui.add_space(24.0);
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.horizontal(|ui| {
                        if (widgets::pill(ui, "OK", PillKind::Primary).clicked() || enter)
                            && let Some(v) = valid
                        {
                            acts.push(Action::SetClock(v));
                            close = true;
                        }
                        close |= widgets::pill(ui, "Cancel", PillKind::Ghost).clicked();
                    });
                    if !close {
                        st.modal = Some(Modal::Options(text));
                    }
                }
                Modal::Shortcuts => {
                    close |= header(ui, "Keyboard shortcuts");
                    egui::ScrollArea::vertical()
                        .max_height(420.0)
                        .show(ui, |ui| {
                            for (section, keys) in SHORTCUTS {
                                ui.label(p.label(section).color(p.text_disabled));
                                ui.add_space(4.0);
                                egui::Grid::new(section)
                                    .num_columns(2)
                                    .spacing([24.0, 4.0])
                                    .show(ui, |ui| {
                                        for (k, what) in *keys {
                                            ui.label(
                                                RichText::new(*k)
                                                    .font(FontId::monospace(12.0))
                                                    .color(p.text_display),
                                            );
                                            ui.label(RichText::new(*what).color(p.text_secondary));
                                            ui.end_row();
                                        }
                                    });
                                ui.add_space(16.0);
                            }
                        });
                }
                Modal::About => {
                    close |= header(ui, "About");
                    ui.label(p.hero("BENCH51", HERO));
                    ui.add_space(4.0);
                    ui.label(p.label(format!("version {}", env!("CARGO_PKG_VERSION"))));
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(
                            "An Intel 8051 assembler, simulator and debugger laid out like \
                             Keil µVision. A learning tool, not a cycle-exact replacement \
                             for a vendor simulator.",
                        )
                        .color(p.text_primary),
                    );
                }
            }
        });
    if resp.should_close() {
        close = true;
    }
    if close {
        st.modal = None;
    }
    st.actions.extend(acts);
}
