//! Nothing-style controls: buttons, segmented controls and bars, bit chips,
//! pin dots, stat rows and empty states.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Painter, Rect,
    Response, RichText, Sense, Stroke, StrokeKind, TextWrapMode, Ui, WidgetText, pos2, vec2,
};

use crate::icons::{self, Icon};
use crate::theme::{self, ACCENT, LABEL, LABEL_TRACKING, palette};

fn galley(ui: &Ui, text: RichText) -> std::sync::Arc<egui::Galley> {
    WidgetText::from(text).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Small,
    )
}

/// Square toolbar button with a monoline icon. `on` shows it latched
/// (inverted), e.g. while the debug session is active.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, on: bool) -> Response {
    let p = palette(ui.ctx());
    let size = vec2(30.0, 28.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = CornerRadius::same(4);
        let fg = if !enabled {
            p.text_disabled
        } else if on {
            p.black
        } else if resp.hovered() {
            p.text_display
        } else {
            p.text_primary
        };
        if on && enabled {
            painter.rect_filled(rect, radius, p.text_display);
        } else if resp.hovered() {
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, p.border_visible),
                StrokeKind::Inside,
            );
        }
        icons::paint(painter, icons::icon_rect(rect, 16.0), icon, fg);
    }
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PillKind {
    /// White pill with black text: the one action to take.
    Primary,
    /// Outlined pill.
    Secondary,
    /// Text only.
    Ghost,
}

/// Pill button: Space Mono, ALL CAPS, tracked.
pub fn pill(ui: &mut Ui, text: &str, kind: PillKind) -> Response {
    let p = palette(ui.ctx());
    let label = RichText::new(text.to_uppercase())
        .font(FontId::monospace(12.0))
        .extra_letter_spacing(0.7);
    let g = galley(ui, label);
    let size = vec2(g.size().x + 36.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = resp.hovered();
        let (fill, stroke, fg) = match kind {
            PillKind::Primary => (p.text_display, Stroke::NONE, p.black),
            PillKind::Secondary => (
                Color32::TRANSPARENT,
                Stroke::new(
                    1.0,
                    if hovered {
                        p.text_primary
                    } else {
                        p.border_visible
                    },
                ),
                p.text_primary,
            ),
            PillKind::Ghost => (
                Color32::TRANSPARENT,
                Stroke::NONE,
                if hovered {
                    p.text_display
                } else {
                    p.text_secondary
                },
            ),
        };
        painter.rect(
            rect,
            CornerRadius::same(255),
            fill,
            stroke,
            StrokeKind::Inside,
        );
        let pos = Align2::CENTER_CENTER
            .align_size_within_rect(g.size(), rect)
            .min;
        painter.galley(pos, g, fg);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Segmented control: the active segment is inverted. Returns true if the
/// value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = palette(ui.ctx());
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, l)| galley(ui, p.label(l)))
        .collect();
    let pad = 10.0;
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 2.0 * pad).collect();
    let size = vec2(widths.iter().sum(), 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let radius = CornerRadius::same(4);
    let mut changed = false;
    let mut x = rect.left();
    for (i, ((v, label), g)) in options.iter().zip(galleys).enumerate() {
        let seg = Rect::from_min_size(pos2(x, rect.top()), vec2(widths[i], rect.height()));
        x += widths[i];
        let resp = ui
            .interact(seg, ui.auto_id_with(("seg", label, i)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let active = *value == *v;
        if resp.clicked() && !active {
            *value = *v;
            changed = true;
        }
        let fg = if active {
            ui.painter().rect_filled(seg, radius, p.text_display);
            p.black
        } else if resp.hovered() {
            p.text_display
        } else {
            p.text_secondary
        };
        let pos = Align2::CENTER_CENTER
            .align_size_within_rect(g.size(), seg)
            .min;
        ui.painter().galley(pos, g, fg);
    }
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, p.border_visible),
        StrokeKind::Inside,
    );
    changed
}

/// Segmented bar: `filled` of `segments` square blocks with 2 px gaps.
/// Returns the index of a clicked segment.
pub fn segment_bar(
    ui: &mut Ui,
    width: f32,
    height: f32,
    segments: usize,
    filled: usize,
    color: Color32,
    clickable: bool,
) -> Option<usize> {
    let p = palette(ui.ctx());
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
    let gap = 2.0;
    let seg_w = (width - gap * (segments as f32 - 1.0)) / segments as f32;
    let hover_idx = resp
        .hover_pos()
        .filter(|_| clickable)
        .map(|pos| (((pos.x - rect.left()) / (seg_w + gap)) as usize).min(segments - 1));
    let empty = if ui.visuals().dark_mode {
        p.border
    } else {
        Color32::from_gray(0xE0)
    };
    for i in 0..segments {
        let x = rect.left() + i as f32 * (seg_w + gap);
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(seg_w, height));
        let c = if i < filled {
            color
        } else if hover_idx.is_some_and(|h| i <= h) {
            p.text_disabled
        } else {
            empty
        };
        ui.painter().rect_filled(r, 0.0, c);
    }
    if clickable && resp.clicked() {
        resp.on_hover_cursor(CursorIcon::PointingHand);
        return hover_idx;
    }
    if clickable {
        resp.on_hover_cursor(CursorIcon::PointingHand);
    }
    None
}

/// Bit flag as a technical tag: lit when set. Clicking toggles it.
pub fn bit_chip(ui: &mut Ui, name: &str, on: bool) -> Response {
    let p = palette(ui.ctx());
    let g = galley(
        ui,
        RichText::new(name)
            .font(FontId::monospace(LABEL))
            .extra_letter_spacing(LABEL_TRACKING * 0.5),
    );
    let size = vec2(g.size().x + 12.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        let (stroke, fg) = if on {
            (p.text_display, p.text_display)
        } else if hovered {
            (p.text_secondary, p.text_secondary)
        } else {
            (p.border_visible, p.text_disabled)
        };
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, stroke),
            StrokeKind::Inside,
        );
        let pos = Align2::CENTER_CENTER
            .align_size_within_rect(g.size(), rect)
            .min;
        ui.painter().galley(pos, g, fg);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    High,
    Low,
    /// Held low from outside the chip, like a pressed button.
    PulledLow,
}

/// A port pin as an LED-like dot.
pub fn pin_dot(ui: &mut Ui, state: Dot) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        let r = 5.0;
        match state {
            Dot::High => {
                ui.painter().circle_filled(c, r, p.text_display);
            }
            Dot::PulledLow => {
                ui.painter().circle_filled(c, r, ACCENT);
            }
            Dot::Low => {
                ui.painter()
                    .circle_stroke(c, r - 0.5, Stroke::new(1.5, p.text_disabled));
            }
        }
        if resp.hovered() {
            ui.painter()
                .circle_stroke(c, r + 3.0, Stroke::new(1.0, p.border_visible));
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Label on the left, value on the right of the available width.
pub fn stat_row(ui: &mut Ui, label: &str, value: RichText) {
    let p = palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(p.label(label));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(value);
        });
    });
}

/// Uniform dot grid, the dot-matrix motif for empty backgrounds.
pub fn dot_grid(painter: &Painter, rect: Rect, color: Color32, pitch: f32) {
    let x0 = (rect.left() / pitch).ceil() * pitch;
    let y0 = (rect.top() / pitch).ceil() * pitch;
    let mut y = y0;
    while y < rect.bottom() {
        let mut x = x0;
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 0.9, color);
            x += pitch;
        }
        y += pitch;
    }
}

/// Centred two-line empty state on a faint dot grid.
pub fn empty_state(ui: &mut Ui, headline: &str, hint: &str) {
    let p = palette(ui.ctx());
    let rect = ui.available_rect_before_wrap();
    let grid = if ui.visuals().dark_mode {
        p.border
    } else {
        p.border_visible.gamma_multiply(0.6)
    };
    dot_grid(ui.painter(), rect, grid, 16.0);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Center)),
        |ui| {
            ui.add_space((rect.height() / 2.0 - 24.0).max(8.0));
            ui.label(p.label(headline).color(p.text_secondary));
            ui.add_space(6.0);
            ui.label(
                RichText::new(hint)
                    .size(theme::BODY - 1.0)
                    .color(p.text_disabled),
            );
        },
    );
}

/// ALL CAPS section heading with breathing room above.
pub fn section(ui: &mut Ui, title: &str) {
    let p = palette(ui.ctx());
    ui.add_space(12.0);
    ui.label(p.label(title).color(p.text_disabled));
    ui.add_space(2.0);
}

/// Underlined single-line input in Space Mono.
pub fn underline_input(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    let p = palette(ui.ctx());
    let out = egui::TextEdit::singleline(text)
        .font(FontId::monospace(13.0))
        .hint_text(
            RichText::new(hint)
                .font(FontId::monospace(13.0))
                .color(p.text_disabled),
        )
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::symmetric(0, 3))
        .desired_width(width)
        .show(ui);
    let resp = out.response.response;
    let color = if resp.has_focus() {
        p.text_primary
    } else {
        p.border_visible
    };
    let r = resp.rect;
    ui.painter()
        .hline(r.x_range(), r.bottom(), Stroke::new(1.0, color));
    resp
}
