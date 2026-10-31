//! Keil's register window. The program counter is the screen's hero.

use bench51_core::session::Session;
use bench51_core::sfr::{ACC, B, DPH, DPL, PSW, SP};
use bench51_core::sim::disasm;
use eframe::egui::{self, RichText, Ui};

use super::{sfr_changed, sfr_chip};
use crate::theme::{HERO, WARNING, palette};
use crate::widgets;

pub fn show(ui: &mut Ui, s: &mut Session) {
    let p = palette(ui.ctx());
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.label(p.label("Program counter"));
            ui.label(p.hero(format!("{:04X}", s.sim.pc), HERO));
            let here = disasm::disassemble(&s.sim.code, s.sim.pc);
            let state = if s.sim.power_down {
                "power-down"
            } else if s.sim.idle {
                "idle"
            } else {
                ""
            };
            ui.horizontal(|ui| {
                ui.label(p.data(here.text).color(p.text_secondary));
                if !state.is_empty() {
                    ui.label(p.label(state).color(WARNING));
                }
            });

            widgets::section(ui, "Registers");
            let value = |v: String, changed: bool| {
                p.data(v)
                    .color(if changed { WARNING } else { p.text_primary })
            };
            let iram_changed = |a: u8| s.sim.iram[a as usize] != s.prev_iram[a as usize];
            egui::Grid::new("regs")
                .num_columns(4)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    let pairs: [(&str, String, bool); 4] = [
                        ("A", format!("{:02X}", s.sim.acc()), sfr_changed(s, ACC)),
                        ("B", format!("{:02X}", s.sim.b()), sfr_changed(s, B)),
                        ("SP", format!("{:02X}", s.sim.sp()), sfr_changed(s, SP)),
                        (
                            "DPTR",
                            format!("{:04X}", s.sim.dptr()),
                            sfr_changed(s, DPL) || sfr_changed(s, DPH),
                        ),
                    ];
                    for row in pairs.chunks(2) {
                        for (name, v, ch) in row {
                            ui.label(p.label(*name));
                            ui.label(value(v.clone(), *ch));
                        }
                        ui.end_row();
                    }
                    for r in 0..4u8 {
                        for n in [r, r + 4] {
                            let a = s.sim.reg_addr(n);
                            ui.label(p.label(format!("R{n}")));
                            ui.label(value(
                                format!("{:02X}", s.sim.iram[a as usize]),
                                iram_changed(a),
                            ));
                        }
                        ui.end_row();
                    }
                });

            widgets::section(ui, "PSW");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(value(format!("{:02X}", s.sim.psw()), sfr_changed(s, PSW)));
                ui.add_space(6.0);
                for (name, mask) in [
                    ("CY", 0x80),
                    ("AC", 0x40),
                    ("F0", 0x20),
                    ("RS1", 0x10),
                    ("RS0", 0x08),
                    ("OV", 0x04),
                    ("P", 0x01),
                ] {
                    sfr_chip(ui, s, name, PSW, mask);
                }
            });
            ui.add_space(4.0);
            widgets::stat_row(ui, "Bank", p.data(s.sim.bank().to_string()));

            widgets::section(ui, "Time");
            widgets::stat_row(ui, "Cycles", p.data(s.sim.cycles.to_string()));
            widgets::stat_row(ui, "Elapsed", p.data(s.elapsed()));
            widgets::stat_row(
                ui,
                "Clock",
                RichText::new(format!("{} MHz", s.clock_mhz))
                    .font(crate::theme::mono(crate::theme::LABEL + 1.0))
                    .color(p.text_secondary),
            );
        });
}
