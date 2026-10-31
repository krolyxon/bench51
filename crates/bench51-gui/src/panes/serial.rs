//! UART #1: what the 8051 transmits, and a line to type into its receiver.

use bench51_core::sfr::SCON;
use eframe::egui::{self, Align, FontId, Key, Layout, RichText, Ui};

use crate::state::State;
use crate::theme::palette;
use crate::widgets::{self, PillKind};

pub fn show(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let Some(doc) = st.docs.get_mut(st.active) else {
        widgets::empty_state(ui, "No program", "Open or create a source file.");
        return;
    };
    let s = &mut doc.session;
    let scon = s.sim.sfr(SCON);
    ui.horizontal(|ui| {
        ui.label(p.label(format!("Mode {}", scon >> 6)));
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        for (name, mask) in [("REN", 0x10), ("TI", 0x02), ("RI", 0x01)] {
            super::sfr_chip(ui, s, name, SCON, mask);
        }
        ui.add_space(8.0);
        ui.label(
            p.label(format!("{} queued", s.sim.uart_rx_pending().len()))
                .color(p.text_disabled),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::pill(ui, "Clear", PillKind::Ghost).clicked() {
                s.sim.uart.tx_log.clear();
            }
        });
    });
    ui.add_space(6.0);

    let mut text = String::new();
    for &b in &s.sim.uart.tx_log {
        match b {
            b'\n' => text.push('\n'),
            b'\r' => {}
            0x20..=0x7E => text.push(b as char),
            _ => text.push('·'),
        }
    }
    let input_h = 34.0;
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false; 2])
        .max_height((ui.available_height() - input_h).max(40.0))
        .show(ui, |ui| {
            if text.is_empty() {
                ui.label(p.label("[no output yet]").color(p.text_disabled));
            } else {
                ui.label(
                    RichText::new(text)
                        .font(FontId::monospace(13.0))
                        .color(p.text_primary),
                );
            }
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(">")
                .font(FontId::monospace(13.0))
                .color(p.text_secondary),
        );
        let w = ui.available_width();
        let r = widgets::underline_input(
            ui,
            &mut st.serial_input,
            "type, then Enter sends it with CR",
            w,
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let mut bytes = std::mem::take(&mut st.serial_input).into_bytes();
            bytes.push(b'\r');
            if let Some(doc) = st.docs.get_mut(st.active) {
                doc.session.sim.uart_send(&bytes);
            }
            r.request_focus();
        }
    });
}
