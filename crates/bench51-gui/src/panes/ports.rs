//! I/O ports: the latch each instruction writes, and the pins the program
//! reads. Click a pin to hold it low from outside, like a button.

use bench51_core::session::Session;
use bench51_core::sfr::P0;
use eframe::egui::{self, Ui};

use crate::theme::palette;
use crate::widgets::{self, Dot};

pub fn show(ui: &mut Ui, s: &mut Session) {
    let p = palette(ui.ctx());
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            egui::Grid::new("ports")
                .num_columns(11)
                .spacing([2.0, 4.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.label("");
                    for bit in (0..8).rev() {
                        ui.vertical_centered(|ui| {
                            ui.label(p.label(bit.to_string()).color(p.text_disabled));
                        });
                    }
                    ui.label("");
                    ui.end_row();
                    for port in 0..4usize {
                        let addr = P0 + 0x10 * port as u8;
                        let latch = s.sim.sfr(addr);
                        let pins = s.sim.port_pins(port);
                        ui.label(p.label(format!("P{port}")).color(p.text_primary));
                        ui.label(p.label("latch").color(p.text_disabled));
                        for bit in (0..8u8).rev() {
                            let m = 1 << bit;
                            let dot = if latch & m != 0 { Dot::High } else { Dot::Low };
                            let r = widgets::pin_dot(ui, dot)
                                .on_hover_text(format!("P{port}.{bit} latch — click to toggle"));
                            if r.clicked() {
                                s.sim.set_sfr(addr, latch ^ m);
                            }
                        }
                        ui.label(p.data(format!("{latch:02X}")).color(p.text_secondary));
                        ui.end_row();

                        ui.label("");
                        ui.label(p.label("pins").color(p.text_disabled));
                        for bit in (0..8u8).rev() {
                            let m = 1 << bit;
                            let dot = if s.sim.pins[port] & m == 0 {
                                Dot::PulledLow
                            } else if pins & m != 0 {
                                Dot::High
                            } else {
                                Dot::Low
                            };
                            let r = widgets::pin_dot(ui, dot).on_hover_text(format!(
                                "P{port}.{bit} pin — click to pull low from outside"
                            ));
                            if r.clicked() {
                                s.sim.pins[port] ^= m;
                            }
                        }
                        ui.label(p.data(format!("{pins:02X}")));
                        ui.end_row();
                        ui.add_space(6.0);
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            ui.label(
                p.label("● 1   ○ 0   red: held low outside")
                    .color(p.text_disabled),
            );
        });
}
