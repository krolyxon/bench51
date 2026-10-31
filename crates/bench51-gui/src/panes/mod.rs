//! Tool windows docked around the editor.

mod build;
mod disasm;
mod interrupts;
mod memory;
mod ports;
mod project;
mod registers;
mod serial;
mod symbols;
mod timers;

use bench51_core::session::Session;
use eframe::egui::{self, Ui};

use crate::editor;
use crate::layout::Pane;
use crate::state::State;
use crate::widgets;

pub fn show(pane: Pane, ui: &mut Ui, st: &mut State) {
    if pane == Pane::Editor {
        editor::show(ui, st);
        return;
    }
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match pane {
                Pane::Project => project::show(ui, st),
                Pane::Build => build::show(ui, st),
                Pane::Registers => with_session(ui, st, registers::show),
                Pane::Memory => memory::show(ui, st),
                Pane::Disassembly => disasm::show(ui, st),
                Pane::Symbols => symbols::show(ui, st),
                Pane::Serial => serial::show(ui, st),
                Pane::Ports => with_session(ui, st, ports::show),
                Pane::Timers => with_session(ui, st, timers::show),
                Pane::Interrupts => with_session(ui, st, interrupts::show),
                Pane::Editor => unreachable!(),
            }
        });
}

/// Runs `f` with the active document's session, or shows why there is
/// nothing to inspect yet.
fn with_session(ui: &mut Ui, st: &mut State, f: fn(&mut Ui, &mut Session)) {
    match st.docs.get_mut(st.active) {
        Some(doc) if doc.session.build.is_some() => f(ui, &mut doc.session),
        Some(_) => widgets::empty_state(ui, "Not built", "Press F7 to assemble."),
        None => widgets::empty_state(ui, "No program", "Open or create a source file."),
    }
}

/// Whether bit `mask` of SFR `addr` is set, and a click toggles it.
pub fn sfr_chip(ui: &mut Ui, s: &mut Session, name: &str, addr: u8, mask: u8) {
    let v = s.sim.sfr(addr);
    if widgets::bit_chip(ui, name, v & mask != 0).clicked() {
        s.sim.set_sfr(addr, v ^ mask);
    }
}

/// An SFR value differs from the snapshot taken before the last step.
pub fn sfr_changed(s: &Session, addr: u8) -> bool {
    s.sim.sfr(addr) != s.prev_sfr[(addr & 0x7F) as usize]
}
