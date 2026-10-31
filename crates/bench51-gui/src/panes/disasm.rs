//! Disassembly window: every instruction of the build with its address,
//! bytes and label, decoded from code memory as it is now.

use bench51_core::sim::disasm;
use bench51_core::syntax::{Tok, tokens};
use eframe::egui::{self, Align2, CursorIcon, FontId, Id, Rect, Sense, Ui, pos2, vec2};

use crate::state::{Action, State};
use crate::theme::{ACCENT, palette};
use crate::widgets;

struct Row {
    line: usize,
    addr: u16,
    len: usize,
    label: String,
    /// Source text for DB/DW lines, which aren't instructions.
    data: Option<String>,
}

fn rows(source: &[String], build: &bench51_core::session::Build) -> Vec<Row> {
    let mut out = Vec::new();
    for (line, lo) in build.out.lines.iter().enumerate() {
        let (Some(addr), false) = (lo.addr, lo.bytes.is_empty()) else {
            continue;
        };
        let text = source.get(line).map_or("", String::as_str);
        let toks = tokens(text);
        let mut label = String::new();
        let mut data = None;
        for (r, t) in &toks {
            match t {
                Tok::Label if label.is_empty() => label = text[r.clone()].to_string(),
                Tok::Directive => {
                    let rest = &text[r.start..];
                    let rest = rest.split(';').next().unwrap_or(rest).trim_end();
                    data = Some(rest.to_string());
                    break;
                }
                Tok::Mnemonic => break,
                _ => {}
            }
        }
        out.push(Row {
            line,
            addr,
            len: lo.bytes.len(),
            label,
            data,
        });
    }
    out
}

pub fn show(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let Some(doc) = st.docs.get_mut(st.active) else {
        widgets::empty_state(ui, "No program", "Open or create a source file.");
        return;
    };
    let doc_id = doc.id;
    let s = &mut doc.session;
    let Some(build) = &s.build else {
        widgets::empty_state(ui, "Not built", "Press F7 to assemble.");
        return;
    };
    let rows = rows(&build.source, build);
    let font = FontId::monospace(13.0);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 6.0;
    let col_addr = 20.0;
    let col_bytes = col_addr + 6.0 * cw;
    let col_label = col_bytes + 10.0 * cw;
    let col_insn = col_label + 10.0 * cw;

    // Header.
    let (hdr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    for (x, name) in [
        (col_addr, "Addr"),
        (col_bytes, "Bytes"),
        (col_label, "Label"),
        (col_insn, "Instruction"),
    ] {
        ui.painter().text(
            pos2(hdr.left() + x, hdr.center().y),
            Align2::LEFT_CENTER,
            name.to_uppercase(),
            FontId::monospace(crate::theme::LABEL),
            p.text_disabled,
        );
    }
    ui.add_space(4.0);

    let pc = s.sim.pc;
    let show_pc = !s.running || s.speed <= 2;
    let pc_row = rows.iter().position(|r| r.addr == pc);
    let bps = s.breakpoints.clone();

    // Follow the PC whenever it moves while stopped.
    let last_id = Id::unique(("disasm_last_pc", doc_id));
    let last = ui.data(|d| d.get_temp::<u16>(last_id));
    let mut area = egui::ScrollArea::vertical()
        .id_salt(("disasm", doc_id))
        .auto_shrink([false; 2]);
    if show_pc && last != Some(pc) {
        ui.data_mut(|d| d.insert_temp(last_id, pc));
        if let Some(i) = pc_row {
            let view = ui.available_height();
            area = area.vertical_scroll_offset((i as f32 * row_h - view / 2.0).max(0.0));
        }
    }

    let mut toggle = None;
    let mut goto = None;
    area.show_rows(ui, row_h, rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for i in range {
            let r = &rows[i];
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
            let painter = ui.painter();
            let cy = rect.center().y;
            let is_pc = show_pc && pc_row == Some(i);
            if is_pc {
                painter.rect_filled(rect, 0.0, p.surface_raised);
                painter.rect_filled(
                    Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                    0.0,
                    ACCENT,
                );
            }
            if bps.contains(&r.line) {
                painter.circle_filled(pos2(rect.left() + 10.0, cy), 4.0, ACCENT);
            }
            let text = |x: f32, s: String, c| {
                painter.text(
                    pos2(rect.left() + x, cy),
                    Align2::LEFT_CENTER,
                    s,
                    font.clone(),
                    c,
                );
            };
            text(
                col_addr,
                format!("{:04X}", r.addr),
                if is_pc {
                    p.text_display
                } else {
                    p.text_secondary
                },
            );
            let bytes: Vec<String> = (0..r.len.min(3))
                .map(|k| format!("{:02X}", s.sim.code[r.addr.wrapping_add(k as u16) as usize]))
                .collect();
            let more = if r.len > 3 { "…" } else { "" };
            text(col_bytes, bytes.join(" ") + more, p.text_disabled);
            text(col_label, r.label.clone(), p.text_display);
            match &r.data {
                Some(d) => text(col_insn, d.clone(), p.text_secondary),
                None => text(
                    col_insn,
                    disasm::disassemble(&s.sim.code, r.addr).text,
                    if is_pc {
                        p.text_display
                    } else {
                        p.text_primary
                    },
                ),
            }
            let resp = resp.on_hover_cursor(CursorIcon::Default);
            if resp.clicked()
                && resp
                    .interact_pointer_pos()
                    .is_some_and(|pos| pos.x < rect.left() + col_addr)
            {
                toggle = Some(r.line);
            } else if resp.double_clicked() {
                goto = Some(r.line);
            }
        }
    });
    if let Some(line) = toggle {
        s.toggle_breakpoint(line);
    }
    if let Some(line) = goto {
        st.actions.push(Action::Goto {
            doc: doc_id,
            line,
            col: 0,
        });
    }
}
