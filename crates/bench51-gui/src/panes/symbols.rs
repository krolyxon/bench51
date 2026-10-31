//! Symbol table of the build, with live values for memory symbols.

use bench51_core::asm::SymKind;
use eframe::egui::{self, CursorIcon, Sense, Ui};

use crate::state::{Action, State};
use crate::theme::{WARNING, palette};
use crate::widgets;

pub fn show(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let Some(doc) = st.docs.get(st.active) else {
        widgets::empty_state(ui, "No program", "Open or create a source file.");
        return;
    };
    let doc_id = doc.id;
    let s = &doc.session;
    let Some(build) = &s.build else {
        widgets::empty_state(ui, "Not built", "Press F7 to assemble.");
        return;
    };
    let mut goto = None;
    if build.out.symbols.is_empty() {
        widgets::empty_state(ui, "No symbols", "Labels and EQU names appear here.");
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            egui::Grid::new("symbols")
                .num_columns(4)
                .spacing([20.0, 6.0])
                .show(ui, |ui| {
                    for h in ["Name", "Kind", "Value", "Now"] {
                        ui.label(p.label(h).color(p.text_disabled));
                    }
                    ui.end_row();
                    for (name, sym) in &build.out.symbols {
                        let v = sym.value;
                        let name_label = egui::Label::new(p.data(name).color(p.text_display))
                            .sense(Sense::click());
                        let resp = ui
                            .add(name_label)
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text("Go to definition");
                        if resp.clicked() {
                            goto = Some(sym.line);
                        }
                        ui.label(p.label(sym.kind.name()));
                        let width = if (0..=0xFF).contains(&v) { 2 } else { 4 };
                        ui.label(
                            p.data(format!("{:0width$X}", v & 0xFFFF))
                                .color(p.text_secondary),
                        );
                        let now = match sym.kind {
                            SymKind::Data | SymKind::Idata if (0..=0xFF).contains(&v) => {
                                let a = v as usize;
                                Some((
                                    format!("{:02X}", s.sim.iram[a]),
                                    s.sim.iram[a] != s.prev_iram[a],
                                ))
                            }
                            SymKind::Xdata if (0..=0xFFFF).contains(&v) => {
                                Some((format!("{:02X}", s.sim.xram[v as usize]), false))
                            }
                            SymKind::Bit if (0..=0xFF).contains(&v) => {
                                Some((u8::from(s.sim.read_bit(v as u8, true)).to_string(), false))
                            }
                            _ => None,
                        };
                        match now {
                            Some((t, changed)) => ui.label(p.data(t).color(if changed {
                                WARNING
                            } else {
                                p.text_primary
                            })),
                            None => ui.label(p.data("—").color(p.text_disabled)),
                        };
                        ui.end_row();
                    }
                });
        });
    if let Some(line) = goto {
        st.actions.push(Action::Goto {
            doc: doc_id,
            line,
            col: 0,
        });
    }
}
