//! Build Output: Keil-style messages, the listing and the Intel HEX file.

use bench51_core::syntax::tokens;
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align, CursorIcon, FontId, Layout, RichText, Sense, TextFormat, Ui};

use crate::state::{Action, BuildTab, LogKind, Mode, State};
use crate::theme::{ACCENT, HERO, palette};
use crate::widgets;

pub fn show(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let summary = st.doc().map(|d| match &d.session.last_asm {
        Some((src, out)) if out.ok() => {
            (out.code_size().to_string(), "bytes", false, src != &d.text)
        }
        Some((src, out)) => (
            out.diagnostics.len().to_string(),
            "errors",
            true,
            src != &d.text,
        ),
        None => (String::new(), "not built", false, false),
    });
    ui.horizontal(|ui| {
        if let Some((n, unit, err, changed)) = &summary {
            let color = if *err { ACCENT } else { p.text_display };
            // The PC is the hero while debugging; one per screen.
            if n.is_empty() {
            } else if st.mode == Mode::Edit {
                ui.label(p.hero(n, HERO).color(color));
            } else {
                ui.label(p.data(n).size(18.0).color(color));
            }
            ui.vertical(|ui| {
                ui.add_space(if st.mode == Mode::Edit { 14.0 } else { 2.0 });
                ui.label(p.label(*unit));
                if *changed {
                    ui.label(p.label("[source changed]").color(p.text_disabled));
                }
            });
        }
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            widgets::segmented(
                ui,
                &mut st.build_tab,
                &[
                    (BuildTab::Messages, "Messages"),
                    (BuildTab::Listing, "Listing"),
                    (BuildTab::Hex, "HEX"),
                ],
            );
        });
    });
    ui.add_space(8.0);
    match st.build_tab {
        BuildTab::Messages => messages(ui, st),
        BuildTab::Listing => listing(ui, st),
        BuildTab::Hex => hex(ui, st),
    }
}

fn messages(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    if st.log.is_empty() {
        widgets::empty_state(
            ui,
            "No messages",
            "Build output appears here. F7 assembles.",
        );
        return;
    }
    let mut goto = None;
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            for line in &st.log {
                let color = match line.kind {
                    LogKind::Info => p.text_secondary,
                    LogKind::Ok => p.text_primary,
                    LogKind::Error => ACCENT,
                };
                let text = RichText::new(&line.text)
                    .font(FontId::monospace(12.5))
                    .color(color);
                if let Some(target) = line.target {
                    let r = ui
                        .add(egui::Label::new(text).sense(Sense::click()))
                        .on_hover_cursor(CursorIcon::PointingHand);
                    if r.clicked() {
                        goto = Some(target);
                    }
                } else {
                    ui.label(text);
                }
            }
        });
    if let Some((doc, line, col)) = goto {
        st.actions.push(Action::Goto { doc, line, col });
    }
}

fn rows(ui: &mut Ui, id: &str, lines: &[&str], mut row: impl FnMut(&str) -> LayoutJob) {
    let row_h = ui.fonts_mut(|f| f.row_height(&FontId::monospace(12.5))) + 2.0;
    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false; 2])
        .show_rows(ui, row_h, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                ui.add(egui::Label::new(row(lines[i])).extend());
            }
        });
}

fn listing(ui: &mut Ui, st: &State) {
    let p = palette(ui.ctx());
    let Some((src, out)) = st.doc().and_then(|d| d.session.last_asm.as_ref()) else {
        widgets::empty_state(ui, "No listing", "Press F7 to assemble.");
        return;
    };
    let text = out.listing(src);
    let lines: Vec<&str> = text.lines().collect();
    let size = 12.5;
    rows(ui, "listing", &lines, |l| {
        let font = FontId::monospace(size);
        if l.starts_with("***") {
            return LayoutJob::simple_singleline(l.to_string(), font, ACCENT);
        }
        let is_code = l.len() > 25 && l.is_char_boundary(25);
        if is_code && (l.as_bytes()[0].is_ascii_hexdigit() || l.starts_with(' ')) {
            let (head, rest) = l.split_at(25);
            let mut job = LayoutJob::default();
            let head_color = if l.starts_with(' ') {
                p.text_disabled
            } else {
                p.text_secondary
            };
            job.append(head, 0.0, TextFormat::simple(font, head_color));
            for (r, tok) in tokens(rest) {
                let (color, _) = p.syntax(tok);
                job.append(
                    &rest[r],
                    0.0,
                    TextFormat::simple(FontId::monospace(size), color),
                );
            }
            return job;
        }
        LayoutJob::simple_singleline(l.to_string(), font, p.text_secondary)
    });
}

fn hex(ui: &mut Ui, st: &State) {
    let p = palette(ui.ctx());
    let Some((_, out)) = st.doc().and_then(|d| d.session.last_asm.as_ref()) else {
        widgets::empty_state(ui, "No HEX file", "Press F7 to assemble.");
        return;
    };
    if !out.ok() {
        widgets::empty_state(ui, "No HEX file", "Fix the errors to generate Intel HEX.");
        return;
    }
    let text = out.intel_hex();
    let lines: Vec<&str> = text.lines().collect();
    rows(ui, "hex", &lines, |l| {
        let font = FontId::monospace(12.5);
        let mut job = LayoutJob::default();
        if l.len() < 11 {
            job.append(l, 0.0, TextFormat::simple(font, p.text_primary));
            return job;
        }
        let n = l.len();
        for (part, color) in [
            (&l[..1], p.text_disabled),
            (&l[1..3], p.text_secondary),
            (&l[3..7], p.text_display),
            (&l[7..9], p.text_secondary),
            (&l[9..n - 2], p.text_primary),
            (&l[n - 2..], p.text_disabled),
        ] {
            job.append(part, 0.0, TextFormat::simple(font.clone(), color));
        }
        job
    });
}
