//! The code editor: document tabs, a find bar, syntax-highlighted text and
//! a gutter with breakpoints, line numbers, addresses, errors and the PC.

use std::collections::{HashMap, HashSet};

use bench51_core::syntax::tokens;
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Id, Key, Layout, Modifiers, Rect, RichText,
    Sense, Shape, Stroke, TextBuffer, TextFormat, Ui, Vec2,
    cache::{ComputerMut, FrameCache},
    pos2,
    text::{CCursor, CCursorRange, LayoutJob},
    text_edit::TextEditState,
    vec2,
};

use crate::state::{Action, Document, Mode, State};
use crate::theme::{self, ACCENT, BODY, DARK, LIGHT, WARNING, palette};
use crate::widgets::{self, PillKind};

const MARGIN: Vec2 = vec2(10.0, 8.0);

pub fn show(ui: &mut Ui, st: &mut State) {
    doc_tabs(ui, st);
    if st.docs.is_empty() {
        welcome(ui, st);
        return;
    }
    if st.find.open {
        find_bar(ui, st);
    }
    code(ui, st);
}

// ------------------------------------------------------------- tabs, find

fn doc_tabs(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let height = 30.0;
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().hline(
        strip.x_range(),
        strip.bottom() - 0.5,
        Stroke::new(1.0, p.border),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(strip.shrink2(vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 0.0;
    for (i, doc) in st.docs.iter().enumerate() {
        let active = i == st.active;
        let name = doc.name();
        let galley =
            ui.painter()
                .layout_no_wrap(name, FontId::monospace(12.0), Color32::PLACEHOLDER);
        let pad = 12.0;
        let dirty_w = if doc.dirty() { 10.0 } else { 0.0 };
        let w = galley.size().x + 2.0 * pad + dirty_w + 14.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::click());
        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        let fg = if active {
            p.text_display
        } else if resp.hovered() {
            p.text_secondary
        } else {
            p.text_disabled
        };
        let text_pos = Align2::LEFT_CENTER
            .align_size_within_rect(galley.size(), rect.shrink2(vec2(pad, 0.0)))
            .min;
        let text_w = galley.size().x;
        ui.painter().galley(text_pos, galley, fg);
        if active {
            ui.painter().hline(
                text_pos.x..=text_pos.x + text_w,
                rect.bottom() - 1.5,
                Stroke::new(2.0, p.text_display),
            );
        }
        if doc.dirty() {
            ui.painter().circle_filled(
                pos2(text_pos.x + text_w + 7.0, rect.center().y),
                2.5,
                p.text_secondary,
            );
        }
        let close =
            Rect::from_center_size(pos2(rect.right() - pad, rect.center().y), Vec2::splat(12.0));
        let close_resp = ui
            .interact(close, Id::unique(("doc_close", doc.id)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if resp.hovered() || close_resp.hovered() || active {
            let c = if close_resp.hovered() {
                p.text_display
            } else {
                p.text_disabled
            };
            let r = close.shrink(3.0);
            let s = Stroke::new(1.2, c);
            ui.painter()
                .line_segment([r.left_top(), r.right_bottom()], s);
            ui.painter()
                .line_segment([r.right_top(), r.left_bottom()], s);
        }
        if close_resp.clicked() || resp.clicked_by(egui::PointerButton::Middle) {
            st.actions.push(Action::Close(i));
        } else if resp.clicked() {
            st.actions.push(Action::Activate(i));
        }
    }
    let plus = ui
        .add(
            egui::Button::new(
                RichText::new("+")
                    .font(FontId::monospace(14.0))
                    .color(p.text_disabled),
            )
            .frame(false),
        )
        .on_hover_text("New file (Ctrl+N)");
    if plus.clicked() {
        st.actions.push(Action::New);
    }
    if st.mode == Mode::Debug {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            ui.label(p.label("Read-only while debugging").color(p.text_disabled));
        });
    }
}

fn find_bar(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let mut step = None;
    let mut close = false;
    ui.horizontal(|ui| {
        ui.add_space(MARGIN.x);
        ui.label(p.label("Find"));
        let resp = widgets::underline_input(ui, &mut st.find.query, "text", 220.0);
        if std::mem::take(&mut st.find.focus) {
            resp.request_focus();
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            step = Some(!ui.input(|i| i.modifiers.shift));
            resp.request_focus();
        }
        if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        let count = st
            .doc()
            .map_or(0, |d| find_all(&d.text, &st.find.query).len());
        ui.label(p.label(format!("{count} found")).color(p.text_disabled));
        if widgets::pill(ui, "Prev", PillKind::Ghost).clicked() {
            step = Some(false);
        }
        if widgets::pill(ui, "Next", PillKind::Ghost).clicked() {
            step = Some(true);
        }
        if widgets::pill(ui, "Close", PillKind::Ghost).clicked() {
            close = true;
        }
    });
    if let (Some(forward), Some(doc)) = (step, st.docs.get_mut(st.active)) {
        find_step(doc, &st.find.query, forward);
    }
    if close {
        st.find.open = false;
    }
}

/// Byte ranges of case-insensitive matches of `query` in `text`.
fn find_all(text: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let hay = text.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    hay.match_indices(&needle)
        .take(2000)
        .map(|(i, m)| (i, i + m.len()))
        .collect()
}

fn find_step(doc: &mut Document, query: &str, forward: bool) {
    let matches = find_all(&doc.text, query);
    if matches.is_empty() {
        doc.find_hit = None;
        return;
    }
    let here = char_to_byte(&doc.text, doc.cursor_char);
    let hit = doc.find_hit.filter(|h| matches.contains(h));
    let pick = if forward {
        let from = hit.map_or(here, |h| h.0 + 1);
        matches.iter().find(|m| m.0 >= from).or(matches.first())
    } else {
        let before = hit.map_or(here, |h| h.0);
        matches
            .iter()
            .rev()
            .find(|m| m.0 < before)
            .or(matches.last())
    };
    if let Some(&(a, b)) = pick {
        let ca = doc.text[..a].chars().count();
        let cb = ca + doc.text[a..b].chars().count();
        doc.select = Some((ca, cb));
        doc.find_hit = Some((a, b));
    }
}

// -------------------------------------------------------------- welcome

fn welcome(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    let rect = ui.available_rect_before_wrap();
    let grid = if ui.visuals().dark_mode {
        p.border
    } else {
        p.border_visible.gamma_multiply(0.5)
    };
    widgets::dot_grid(ui.painter(), rect, grid, 16.0);
    let inner = rect.shrink2(vec2(56.0, 48.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.add_space(24.0);
        ui.label(p.hero("BENCH51", theme::HERO_XL));
        ui.add_space(4.0);
        ui.label(p.label("8051 assembler · simulator · debugger"));
        ui.add_space(40.0);
        ui.horizontal(|ui| {
            if widgets::pill(ui, "New file", PillKind::Primary).clicked() {
                st.actions.push(Action::New);
            }
            ui.add_space(8.0);
            if widgets::pill(ui, "Open…", PillKind::Secondary).clicked() {
                st.actions.push(Action::Open);
            }
        });
        if !st.prefs.recent.is_empty() {
            ui.add_space(48.0);
            ui.label(p.label("Recent"));
            ui.add_space(8.0);
            for path in st.prefs.recent.clone() {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into(),
                );
                let dir = path
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    let r = ui
                        .add(
                            egui::Label::new(
                                RichText::new(name)
                                    .font(FontId::monospace(BODY))
                                    .color(p.text_primary),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    ui.label(
                        RichText::new(dir)
                            .font(FontId::monospace(12.0))
                            .color(p.text_disabled),
                    );
                    if r.clicked() {
                        st.actions.push(Action::OpenPath(path.clone()));
                    }
                });
            }
        }
    });
}

// ----------------------------------------------------------------- code

#[derive(Default)]
struct Highlighter;

impl ComputerMut<(bool, &str), LayoutJob> for Highlighter {
    fn compute(&mut self, (dark, text): (bool, &str)) -> LayoutJob {
        highlight(text, dark)
    }
}

type HighlightCache = FrameCache<LayoutJob, Highlighter>;

/// Lays out `text` with the greyscale syntax palette. The job's text is
/// exactly `text`, as `TextEdit` requires.
pub fn highlight(text: &str, dark: bool) -> LayoutJob {
    let p = if dark { &DARK } else { &LIGHT };
    let font = FontId::monospace(BODY);
    let bold = FontId::new(BODY, theme::mono_bold());
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            job.append("\n", 0.0, TextFormat::simple(font.clone(), p.text_primary));
        }
        for (r, tok) in tokens(line) {
            let (color, strong) = p.syntax(tok);
            let font_id = if strong { bold.clone() } else { font.clone() };
            job.append(&line[r], 0.0, TextFormat::simple(font_id, color));
        }
    }
    job
}

pub fn char_to_byte(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map_or(text.len(), |(b, _)| b)
}

/// (line, column) of char index `ci`.
fn line_col(text: &str, ci: usize) -> (usize, usize) {
    let b = char_to_byte(text, ci);
    let before = &text[..b];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count())
}

/// Char index of (line, column), clamped to the text.
fn char_index(text: &str, line: usize, col: usize) -> usize {
    let mut ci = 0;
    for (i, l) in text.split('\n').enumerate() {
        let n = l.chars().count();
        if i == line {
            return ci + col.min(n);
        }
        ci += n + 1;
    }
    text.chars().count()
}

/// Replaces the selection with spaces up to the next 8-column tab stop,
/// matching the assembler listing.
fn insert_tab(ctx: &egui::Context, id: Id, text: &mut String) {
    let Some(mut state) = TextEditState::load(ctx, id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let (a, b) = {
        let (x, y) = (range.primary.index.0, range.secondary.index.0);
        (x.min(y), x.max(y))
    };
    let (ba, bb) = (char_to_byte(text, a), char_to_byte(text, b));
    let start = text[..ba].rfind('\n').map_or(0, |i| i + 1);
    let col = text[start..ba].chars().count();
    let n = 8 - col % 8;
    text.replace_range(ba..bb, &" ".repeat(n));
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(a + n))));
    state.store(ctx, id);
}

/// What the gutter and backgrounds need, gathered before the text is
/// borrowed by the `TextEdit`.
struct Marks {
    errors: HashMap<usize, String>,
    breakpoints: HashSet<usize>,
    pc_line: Option<usize>,
    /// Code address of each line, shown while debugging.
    addrs: Option<HashMap<usize, u16>>,
}

fn code(ui: &mut Ui, st: &mut State) {
    let debug = st.mode == Mode::Debug;
    let query = if st.find.open {
        st.find.query.clone()
    } else {
        String::new()
    };
    let Some(doc) = st.docs.get_mut(st.active) else {
        return;
    };
    let p = palette(ui.ctx());
    let dark = ui.visuals().dark_mode;
    let font = FontId::monospace(BODY);
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let n_lines = doc.text.split('\n').count();
    let digits = n_lines.to_string().len().max(3);

    let s = &doc.session;
    let built = s.build.is_some() && !doc.stale();
    let marks = Marks {
        errors: s
            .diagnostics_for(&doc.text)
            .iter()
            .rev()
            .map(|d| (d.line, d.msg.clone()))
            .collect(),
        breakpoints: s.breakpoints.clone(),
        pc_line: (debug && built && (!s.running || s.speed <= 2))
            .then(|| s.pc_line())
            .flatten(),
        addrs: (debug && built).then(|| {
            let b = s.build.as_ref().unwrap();
            (0..b.out.lines.len())
                .filter_map(|l| Some((l, b.line_addr(l)?)))
                .collect()
        }),
    };
    let follow = std::mem::take(&mut doc.follow_pc)
        .then_some(marks.pc_line)
        .flatten();

    let bp_w = 20.0;
    let num_w = digits as f32 * char_w;
    let addr_w = if marks.addrs.is_some() {
        4.0 * char_w + 16.0
    } else {
        0.0
    };
    let gutter_w = bp_w + num_w + addr_w + 16.0;
    let edit_id = Id::unique(("editor", doc.id));
    let ctx = ui.ctx().clone();

    if !debug
        && ctx.memory(|m| m.has_focus(edit_id))
        && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab))
    {
        insert_tab(&ctx, edit_id, &mut doc.text);
    }

    // Programmatic cursor moves: jump to a line (taking keyboard focus), or
    // select a find match (leaving focus in the find bar, so Enter there
    // can't overwrite the match).
    let mut scroll_to_char = None;
    let jump = doc
        .goto
        .take()
        .map(|(line, col)| {
            let c = char_index(&doc.text, line, col);
            ((c, c), true)
        })
        .or(doc.select.take().map(|r| (r, false)));
    if let Some(((a, b), focus)) = jump {
        let mut state = TextEditState::load(&ctx, edit_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        state.store(&ctx, edit_id);
        if focus {
            ctx.memory_mut(|m| m.request_focus(edit_id));
        }
        // The TextEdit only reports its cursor while focused.
        doc.cursor_char = a;
        doc.cursor = line_col(&doc.text, a);
        scroll_to_char = Some(a);
    }
    let hit = doc.find_hit;

    let matches = find_all(&doc.text, &query);
    let mut clicked_line = None;
    let mut cursor = None;

    egui::ScrollArea::both()
        .id_salt(("code", doc.id))
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let bg = ui.painter().add(Shape::Noop);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let min_h = n_lines as f32 * row_h + 2.0 * MARGIN.y;
                let (gutter, gutter_resp) = ui.allocate_exact_size(
                    vec2(gutter_w, min_h.max(ui.available_height())),
                    Sense::click(),
                );

                let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, _wrap: f32| {
                    let job = ui.memory_mut(|m| {
                        m.caches
                            .cache::<HighlightCache>()
                            .get((dark, buf.as_str()))
                            .clone()
                    });
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let mut ro: &str;
                let buf: &mut dyn TextBuffer = if debug {
                    ro = doc.text.as_str();
                    &mut ro
                } else {
                    &mut doc.text
                };
                let out = egui::TextEdit::multiline(buf)
                    .id(edit_id)
                    .font(font.clone())
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::symmetric(MARGIN.x as i8, MARGIN.y as i8))
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui);
                cursor = out.cursor_range.map(|r| r.primary.index.0);

                let galley = &out.galley;
                let origin = out.galley_pos;
                let content_right = ui
                    .max_rect()
                    .right()
                    .max(origin.x + galley.size().x + MARGIN.x);
                let row_rect = |line: usize| -> Rect {
                    let y = galley
                        .rows
                        .get(line)
                        .map_or(origin.y + line as f32 * row_h, |r| origin.y + r.pos.y);
                    Rect::from_min_max(pos2(gutter.left(), y), pos2(content_right, y + row_h))
                };

                // Backgrounds: find matches and the PC row.
                let mut shapes = Vec::new();
                for &(a, b) in matches.iter().take(500) {
                    let ca = doc_text_chars(galley.text(), a);
                    let cb = doc_text_chars(galley.text(), b);
                    let ra = galley.pos_from_cursor(CCursor::new(ca));
                    let rb = galley.pos_from_cursor(CCursor::new(cb));
                    if (ra.top() - rb.top()).abs() < 1.0 {
                        let r = Rect::from_min_max(ra.min, pos2(rb.left(), ra.bottom()))
                            .translate(origin.to_vec2());
                        let current = hit == Some((a, b));
                        shapes.push(Shape::rect_filled(
                            r.expand(1.0),
                            2.0,
                            WARNING.gamma_multiply(if current { 0.85 } else { 0.3 }),
                        ));
                    }
                }
                if let Some(pc) = marks.pc_line {
                    let r = row_rect(pc);
                    shapes.push(Shape::rect_filled(r, 0.0, p.surface_raised));
                    shapes.push(Shape::rect_filled(
                        Rect::from_min_size(r.min, vec2(2.0, r.height())),
                        0.0,
                        ACCENT,
                    ));
                }
                ui.painter().set(bg, Shape::Vec(shapes));

                // Gutter.
                let clip = ui.clip_rect();
                let painter = ui.painter();
                let first = ((clip.top() - origin.y) / row_h).floor().max(0.0) as usize;
                let last = (((clip.bottom() - origin.y) / row_h).ceil() as usize + 1).min(n_lines);
                let cursor_line = doc.cursor.0;
                for line in first..last {
                    let r = row_rect(line);
                    let cy = r.center().y;
                    let err = marks.errors.contains_key(&line);
                    let bp = marks.breakpoints.contains(&line);
                    let bx = gutter.left() + 4.0 + bp_w / 2.0;
                    if bp {
                        painter.circle_filled(pos2(bx, cy), 4.5, ACCENT);
                    } else if err {
                        let s = Stroke::new(1.5, ACCENT);
                        let d = 3.5;
                        painter.line_segment([pos2(bx - d, cy - d), pos2(bx + d, cy + d)], s);
                        painter.line_segment([pos2(bx + d, cy - d), pos2(bx - d, cy + d)], s);
                    }
                    let num_color = if err {
                        ACCENT
                    } else if Some(line) == marks.pc_line {
                        p.text_display
                    } else if line == cursor_line && !debug {
                        p.text_secondary
                    } else {
                        p.text_disabled
                    };
                    painter.text(
                        pos2(gutter.left() + 4.0 + bp_w + num_w, cy),
                        Align2::RIGHT_CENTER,
                        (line + 1).to_string(),
                        font.clone(),
                        num_color,
                    );
                    if let Some(addr) = marks.addrs.as_ref().and_then(|a| a.get(&line)) {
                        painter.text(
                            pos2(gutter.left() + 4.0 + bp_w + num_w + 12.0, cy),
                            Align2::LEFT_CENTER,
                            format!("{addr:04X}"),
                            font.clone(),
                            p.text_disabled,
                        );
                    }
                }
                let gutter_resp = gutter_resp.on_hover_cursor(CursorIcon::PointingHand);
                if gutter_resp.clicked()
                    && let Some(pos) = gutter_resp.interact_pointer_pos()
                {
                    let line = ((pos.y - origin.y) / row_h).floor();
                    if line >= 0.0 && (line as usize) < n_lines {
                        clicked_line = Some(line as usize);
                    }
                }
                let hovered_line = gutter_resp
                    .hover_pos()
                    .map(|pos| ((pos.y - origin.y) / row_h).floor() as usize);
                if let Some(msg) = hovered_line.and_then(|l| marks.errors.get(&l)) {
                    gutter_resp.on_hover_text(msg);
                }

                if let Some(line) = follow {
                    ui.scroll_to_rect(row_rect(line), Some(Align::Center));
                }
                if let Some(ci) = scroll_to_char {
                    let r = galley
                        .pos_from_cursor(CCursor::new(ci))
                        .translate(origin.to_vec2());
                    ui.scroll_to_rect(r.expand2(vec2(40.0, 0.0)), Some(Align::Center));
                }
            });
        });

    if let Some(ci) = cursor {
        doc.cursor_char = ci;
        doc.cursor = line_col(&doc.text, ci);
    }
    if let Some(line) = clicked_line {
        doc.session.toggle_breakpoint(line);
    }
}

/// Char index of byte offset `b` in `text`.
fn doc_text_chars(text: &str, b: usize) -> usize {
    text[..b.min(text.len())].chars().count()
}

#[cfg(test)]
mod tests {
    use super::{char_index, highlight, line_col};

    #[test]
    fn layout_job_reproduces_the_text() {
        let text = "main:  MOV A,#1 ; µs\n\n\tDB \"x\"\n";
        assert_eq!(highlight(text, true).text, text);
    }

    #[test]
    fn cursor_conversions_round_trip() {
        let text = "ab\nµcd\n\nx";
        for ci in 0..=text.chars().count() {
            let (l, c) = line_col(text, ci);
            assert_eq!(char_index(text, l, c), ci);
        }
        assert_eq!(line_col(text, 4), (1, 1));
    }
}
