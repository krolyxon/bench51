//! Timer/counter 0 and 1: mode, control bits and progress to overflow.

use bench51_core::session::Session;
use bench51_core::sfr::{TCON, TH0, TH1, TL0, TL1, TMOD};
use eframe::egui::{self, Align, Layout, Ui};

use super::{sfr_changed, sfr_chip};
use crate::theme::{WARNING, palette};
use crate::widgets;

/// Count so far and the count at which the timer overflows.
pub fn progress(mode: u8, th: u8, tl: u8) -> (u32, u32) {
    match mode {
        0 => (((th as u32) << 5) | (tl as u32 & 0x1F), 1 << 13),
        1 => (((th as u32) << 8) | tl as u32, 1 << 16),
        2 => {
            let span = 256 - th as u32;
            ((tl as u32).saturating_sub(th as u32).min(span), span.max(1))
        }
        _ => (tl as u32, 256),
    }
}

fn timer(ui: &mut Ui, s: &mut Session, n: u8) {
    let p = palette(ui.ctx());
    let (th, tl) = if n == 0 { (TH0, TL0) } else { (TH1, TL1) };
    let shift = 4 * n;
    let ctl = (s.sim.sfr(TMOD) >> shift) & 0x0F;
    let mut mode = ctl & 3;
    let (tr, tf) = if n == 0 { (0x10, 0x20) } else { (0x40, 0x80) };
    let running = s.sim.sfr(TCON) & tr != 0;

    ui.horizontal(|ui| {
        ui.label(p.label(format!("Timer {n}")).color(p.text_primary));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let desc = match mode {
                0 => "13-bit",
                1 => "16-bit",
                2 => "8-bit auto-reload",
                _ => "split 8-bit",
            };
            ui.label(p.label(desc).color(p.text_disabled));
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let changed = sfr_changed(s, th) || sfr_changed(s, tl);
        ui.label(
            p.data(format!("{:02X} {:02X}", s.sim.sfr(th), s.sim.sfr(tl)))
                .size(18.0)
                .color(if changed { WARNING } else { p.text_primary }),
        );
        ui.label(p.label(format!("TH{n} TL{n}")).color(p.text_disabled));
    });
    let (count, max) = progress(mode, s.sim.sfr(th), s.sim.sfr(tl));
    let segments = 24;
    let filled = (count as u64 * segments as u64 / max as u64) as usize;
    let color = if running {
        p.text_display
    } else {
        p.text_disabled
    };
    let w = ui.available_width().min(360.0);
    widgets::segment_bar(ui, w, 8.0, segments, filled, color, false);
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if widgets::segmented(ui, &mut mode, &[(0, "M0"), (1, "M1"), (2, "M2"), (3, "M3")]) {
            let tmod = s.sim.sfr(TMOD) & !(3 << shift) | (mode << shift);
            s.sim.set_sfr(TMOD, tmod);
        }
        ui.add_space(6.0);
        sfr_chip(ui, s, "GATE", TMOD, 0x08 << shift);
        sfr_chip(ui, s, "C/T", TMOD, 0x04 << shift);
        sfr_chip(ui, s, &format!("TR{n}"), TCON, tr);
        sfr_chip(ui, s, &format!("TF{n}"), TCON, tf);
    });
}

pub fn show(ui: &mut Ui, s: &mut Session) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            timer(ui, s, 0);
            ui.add_space(24.0);
            timer(ui, s, 1);
        });
}

#[cfg(test)]
mod tests {
    use super::progress;

    #[test]
    fn overflow_progress_per_mode() {
        assert_eq!(progress(1, 0x80, 0x00), (0x8000, 0x10000));
        assert_eq!(progress(0, 0x01, 0xFF), (0x3F, 0x2000));
        assert_eq!(progress(2, 0xFD, 0xFE), (1, 3));
        assert_eq!(progress(2, 0xFF, 0xFF), (0, 1));
        assert_eq!(progress(3, 0x12, 0x40), (0x40, 256));
    }
}
