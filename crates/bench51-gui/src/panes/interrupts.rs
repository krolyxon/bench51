//! Interrupt system: enable, priority and request flag of each source.

use bench51_core::session::{Session, vector_name};
use bench51_core::sfr::{IE, IP, SCON, TCON};
use eframe::egui::{self, Ui};

use super::sfr_chip;
use crate::theme::{WARNING, palette};
use crate::widgets;

/// (name, vector, IE/IP mask, request flag as (SFR, mask)).
const SOURCES: [(&str, u16, u8, (u8, u8)); 5] = [
    ("INT0", 0x03, 0x01, (TCON, 0x02)),
    ("T0", 0x0B, 0x02, (TCON, 0x20)),
    ("INT1", 0x13, 0x04, (TCON, 0x08)),
    ("T1", 0x1B, 0x08, (TCON, 0x80)),
    ("UART", 0x23, 0x10, (SCON, 0x03)),
];

pub fn show(ui: &mut Ui, s: &mut Session) {
    let p = palette(ui.ctx());
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                sfr_chip(ui, s, "EA", IE, 0x80);
                ui.add_space(12.0);
                let state = if s.sim.power_down {
                    ("power-down", WARNING)
                } else if s.sim.idle {
                    ("idle", WARNING)
                } else {
                    ("running", p.text_secondary)
                };
                ui.label(p.label("CPU"));
                ui.label(p.label(state.0).color(state.1));
            });
            ui.add_space(10.0);
            egui::Grid::new("irqs")
                .num_columns(5)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    for h in ["Source", "Vector", "Enable", "Priority", "Request"] {
                        ui.label(p.label(h).color(p.text_disabled));
                    }
                    ui.end_row();
                    for (name, vector, mask, (flag_sfr, flag_mask)) in SOURCES {
                        let active = s.sim.last_irq == Some(vector) && !s.sim.in_service.is_empty();
                        ui.label(p.label(name).color(if active {
                            p.text_display
                        } else {
                            p.text_primary
                        }));
                        ui.label(p.data(format!("{vector:04X}")).color(p.text_secondary));
                        sfr_chip(ui, s, "EN", IE, mask);
                        sfr_chip(ui, s, "HI", IP, mask);
                        let req = s.sim.sfr(flag_sfr) & flag_mask != 0;
                        if flag_sfr == SCON {
                            widgets::bit_chip(ui, "RI|TI", req);
                        } else {
                            sfr_chip(ui, s, "REQ", flag_sfr, flag_mask);
                        }
                        ui.end_row();
                    }
                });
            widgets::section(ui, "In service");
            let levels = if s.sim.in_service.is_empty() {
                "none".to_string()
            } else {
                s.sim
                    .in_service
                    .iter()
                    .map(|l| format!("L{l}"))
                    .collect::<Vec<_>>()
                    .join(" > ")
            };
            widgets::stat_row(ui, "Levels", p.data(levels));
            let last = s.sim.last_irq.map_or("—".to_string(), |v| {
                format!("{v:04X}h {}", vector_name(v))
            });
            widgets::stat_row(ui, "Last taken", p.data(last));
        });
}
