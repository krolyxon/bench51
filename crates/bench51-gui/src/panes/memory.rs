//! Memory window: IRAM, SFRs, XRAM and code as a hex grid. Double-click a
//! byte to change it.

use bench51_core::session::{MemSpace, Session};
use bench51_core::sfr;
use eframe::egui::{
    self, Align, Align2, FontId, Key, Layout, Rect, Sense, TextEdit, Ui, pos2, vec2,
};

use crate::state::{MemView, State};
use crate::theme::{ACCENT, WARNING, palette};
use crate::widgets;

const PER_ROW: usize = 16;

pub fn show(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let mut space = st.mem.space;
    ui.horizontal(|ui| {
        if widgets::segmented(
            ui,
            &mut space,
            &[
                (MemSpace::Iram, "IRAM"),
                (MemSpace::Sfr, "SFR"),
                (MemSpace::Xram, "XRAM"),
                (MemSpace::Code, "CODE"),
            ],
        ) {
            st.mem.edit = None;
        }
        if space != MemSpace::Sfr {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let r = widgets::underline_input(ui, &mut st.mem.goto, "addr", 64.0);
                ui.label(p.label("Go to"));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    let t = st.mem.goto.trim().trim_start_matches("0x");
                    if let Ok(a) = u32::from_str_radix(t.trim_end_matches(['h', 'H']), 16) {
                        st.mem.scroll_to = Some(a);
                    }
                }
            });
        }
    });
    st.mem.space = space;
    ui.add_space(8.0);
    let Some(doc) = st.docs.get_mut(st.active) else {
        widgets::empty_state(ui, "No program", "Open or create a source file.");
        return;
    };
    if doc.session.build.is_none() {
        widgets::empty_state(ui, "Not built", "Press F7 to assemble.");
        return;
    }
    match space {
        MemSpace::Sfr => sfr_table(ui, &mut doc.session, &mut st.mem),
        _ => hex_grid(ui, &mut doc.session, &mut st.mem),
    }
}

fn parse_byte(s: &str) -> Option<u8> {
    u8::from_str_radix(s.trim().trim_end_matches(['h', 'H']), 16).ok()
}

/// Inline editor for one byte. Returns true when editing ended.
fn edit_cell(ui: &mut Ui, rect: Rect, text: &mut String) -> (bool, Option<u8>) {
    let p = palette(ui.ctx());
    ui.painter()
        .rect_filled(rect.expand(2.0), 2.0, p.surface_raised);
    let r = ui.put(
        rect.expand(2.0),
        TextEdit::singleline(text)
            .font(FontId::monospace(13.0))
            .char_limit(2)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .text_color(p.text_display),
    );
    if !r.has_focus() && !r.lost_focus() {
        r.request_focus();
    }
    if r.lost_focus() {
        let commit = ui.input(|i| i.key_pressed(Key::Enter));
        return (true, commit.then(|| parse_byte(text)).flatten());
    }
    (false, None)
}

fn hex_grid(ui: &mut Ui, s: &mut Session, mem: &mut MemView) {
    let p = palette(ui.ctx());
    let space = mem.space;
    let font = FontId::monospace(13.0);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 4.0;
    let total = if space == MemSpace::Iram {
        256
    } else {
        0x10000
    };
    let addr_digits = if total > 256 { 4 } else { 2 };
    let x_bytes = (addr_digits + 2) as f32 * cw;
    let byte_x = |i: usize| x_bytes + (i * 3) as f32 * cw + if i >= 8 { cw } else { 0.0 };
    let x_ascii = byte_x(PER_ROW) + cw;

    let pc = s.sim.pc as usize;
    let pc_len = bench51_core::isa::info(s.sim.code[pc]).map_or(1, |i| i.size as usize);
    let bps = s.bp_addrs();
    let bank = s.sim.reg_addr(0) as usize;
    let sp = s.sim.sp() as usize;

    let mut area = egui::ScrollArea::vertical()
        .id_salt(("mem", space.name()))
        .auto_shrink([false; 2]);
    if let Some(a) = mem.scroll_to.take() {
        area = area.vertical_scroll_offset((a as usize % total / PER_ROW) as f32 * row_h);
    }
    let mut poke = None;
    area.show_rows(ui, row_h, total / PER_ROW, |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for row in range {
            let base = row * PER_ROW;
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let painter = ui.painter();
            let cy = rect.center().y;
            let addr = if addr_digits == 4 {
                format!("{base:04X}")
            } else {
                format!("{base:02X}")
            };
            painter.text(
                pos2(rect.left(), cy),
                Align2::LEFT_CENTER,
                addr,
                font.clone(),
                p.text_secondary,
            );
            let mut ascii = String::with_capacity(PER_ROW);
            for i in 0..PER_ROW {
                let a = base + i;
                let (v, fg, inverted) = match space {
                    MemSpace::Iram => {
                        let v = s.sim.iram[a];
                        let fg = if v != s.prev_iram[a] {
                            WARNING
                        } else if (bank..bank + 8).contains(&a) {
                            p.text_display
                        } else if v == 0 {
                            p.text_disabled
                        } else {
                            p.text_primary
                        };
                        (v, fg, a == sp)
                    }
                    MemSpace::Xram => {
                        let v = s.sim.xram[a];
                        (
                            v,
                            if v == 0 {
                                p.text_disabled
                            } else {
                                p.text_primary
                            },
                            false,
                        )
                    }
                    _ => {
                        let v = s.sim.code[a];
                        let fg = if bps.contains(&(a as u16)) {
                            ACCENT
                        } else if v == 0 {
                            p.text_disabled
                        } else {
                            p.text_primary
                        };
                        (v, fg, (pc..pc + pc_len).contains(&a))
                    }
                };
                let cell = Rect::from_min_size(
                    pos2(rect.left() + byte_x(i) - 2.0, rect.top() + 1.0),
                    vec2(2.0 * cw + 4.0, row_h - 2.0),
                );
                let editing = mem.edit.as_ref().is_some_and(|(ea, _)| *ea as usize == a);
                if editing {
                    let (_, text) = mem.edit.as_mut().unwrap();
                    let (done, value) = edit_cell(ui, cell.shrink2(vec2(2.0, 1.0)), text);
                    if done {
                        if let Some(v) = value {
                            poke = Some((a as u32, v));
                        }
                        mem.edit = None;
                    }
                } else {
                    let painter = ui.painter();
                    let fg = if inverted {
                        painter.rect_filled(cell, 2.0, p.text_display);
                        p.black
                    } else {
                        fg
                    };
                    painter.text(
                        pos2(rect.left() + byte_x(i), cy),
                        Align2::LEFT_CENTER,
                        format!("{v:02X}"),
                        font.clone(),
                        fg,
                    );
                }
                ascii.push(if v.is_ascii_graphic() || v == b' ' {
                    v as char
                } else {
                    '·'
                });
            }
            ui.painter().text(
                pos2(rect.left() + x_ascii, cy),
                Align2::LEFT_CENTER,
                ascii,
                font.clone(),
                p.text_disabled,
            );
            if resp.double_clicked()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let x = pos.x - rect.left();
                if let Some(i) =
                    (0..PER_ROW).find(|&i| x >= byte_x(i) - 4.0 && x < byte_x(i) + 2.5 * cw)
                {
                    let a = base + i;
                    let v = match space {
                        MemSpace::Iram => s.sim.iram[a],
                        MemSpace::Xram => s.sim.xram[a],
                        _ => s.sim.code[a],
                    };
                    mem.edit = Some((a as u32, format!("{v:02X}")));
                }
            }
        }
    });
    if let Some((a, v)) = poke {
        s.poke(space, a, v);
    }
}

fn sfr_table(ui: &mut Ui, s: &mut Session, mem: &mut MemView) {
    let p = palette(ui.ctx());
    let cols = if ui.available_width() > 620.0 { 3 } else { 2 };
    let per_col = sfr::SFRS.len().div_ceil(cols);
    let mut poke = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            egui::Grid::new("sfr_table")
                .num_columns(cols * 4)
                .spacing([14.0, 5.0])
                .show(ui, |ui| {
                    for r in 0..per_col {
                        for c in 0..cols {
                            let Some(&(name, addr)) = sfr::SFRS.get(c * per_col + r) else {
                                continue;
                            };
                            let v = s.sim.read_direct(addr, true);
                            let changed = super::sfr_changed(s, addr);
                            ui.label(p.label(name).color(p.text_secondary));
                            ui.label(p.data(format!("{addr:02X}")).color(p.text_disabled));
                            let editing = mem.edit.as_ref().is_some_and(|(a, _)| *a == addr as u32);
                            if editing {
                                let (rect, _) =
                                    ui.allocate_exact_size(vec2(24.0, 18.0), Sense::hover());
                                let (_, text) = mem.edit.as_mut().unwrap();
                                let (done, value) = edit_cell(ui, rect, text);
                                if done {
                                    if let Some(v) = value {
                                        poke = Some((addr as u32, v));
                                    }
                                    mem.edit = None;
                                }
                            } else {
                                let color = if changed { WARNING } else { p.text_primary };
                                let resp = ui.add(
                                    egui::Label::new(p.data(format!("{v:02X}")).color(color))
                                        .sense(Sense::click()),
                                );
                                if resp.double_clicked() {
                                    mem.edit = Some((addr as u32, format!("{v:02X}")));
                                }
                            }
                            let bits = p.data(format!("{v:08b}")).size(12.0).color(if changed {
                                WARNING
                            } else {
                                p.text_disabled
                            });
                            ui.label(bits);
                        }
                        ui.end_row();
                    }
                });
        });
    if let Some((a, v)) = poke {
        s.poke(MemSpace::Sfr, a, v);
    }
}
