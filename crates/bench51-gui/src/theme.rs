//! Nothing design tokens mapped onto egui: palettes for both themes, the
//! bundled fonts, and text helpers that keep the type budget in one place.

use std::sync::Arc;

use bench51_core::syntax::Tok;
use eframe::egui::{
    self, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText,
    Shadow, Stroke, Style, TextStyle, Theme, Visuals,
    epaint::text::{FontTweak, VariationCoords},
};

/// The colour tokens of one theme (`references/tokens.md` §2).
pub struct Palette {
    /// Page background: OLED black / warm off-white paper.
    pub black: Color32,
    pub surface: Color32,
    pub surface_raised: Color32,
    pub border: Color32,
    pub border_visible: Color32,
    pub text_disabled: Color32,
    pub text_secondary: Color32,
    pub text_primary: Color32,
    pub text_display: Color32,
    pub interactive: Color32,
}

pub const DARK: Palette = Palette {
    black: Color32::from_rgb(0x00, 0x00, 0x00),
    surface: Color32::from_rgb(0x11, 0x11, 0x11),
    surface_raised: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    border: Color32::from_rgb(0x22, 0x22, 0x22),
    border_visible: Color32::from_rgb(0x33, 0x33, 0x33),
    text_disabled: Color32::from_rgb(0x66, 0x66, 0x66),
    text_secondary: Color32::from_rgb(0x99, 0x99, 0x99),
    text_primary: Color32::from_rgb(0xE8, 0xE8, 0xE8),
    text_display: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    interactive: Color32::from_rgb(0x5B, 0x9B, 0xF6),
};

pub const LIGHT: Palette = Palette {
    black: Color32::from_rgb(0xF5, 0xF5, 0xF5),
    surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface_raised: Color32::from_rgb(0xF0, 0xF0, 0xF0),
    border: Color32::from_rgb(0xE8, 0xE8, 0xE8),
    border_visible: Color32::from_rgb(0xCC, 0xCC, 0xCC),
    text_disabled: Color32::from_rgb(0x99, 0x99, 0x99),
    text_secondary: Color32::from_rgb(0x66, 0x66, 0x66),
    text_primary: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    text_display: Color32::from_rgb(0x00, 0x00, 0x00),
    interactive: Color32::from_rgb(0x00, 0x7A, 0xFF),
};

/// Signal red: breakpoints, errors, the PC bar. Never decorative.
pub const ACCENT: Color32 = Color32::from_rgb(0xD7, 0x19, 0x21);
/// Values that changed since the last step.
pub const WARNING: Color32 = Color32::from_rgb(0xD4, 0xA8, 0x43);

/// The three type sizes: labels, body/data, and the one hero per screen.
pub const LABEL: f32 = 11.0;
pub const BODY: f32 = 14.0;
pub const HERO: f32 = 44.0;
/// Doto at welcome-screen scale.
pub const HERO_XL: f32 = 72.0;
/// Tracking for ALL CAPS labels (0.08em at 11px).
pub const LABEL_TRACKING: f32 = 0.9;

pub fn palette(ctx: &Context) -> &'static Palette {
    match ctx.theme() {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

pub fn mono_bold() -> FontFamily {
    FontFamily::Name("mono-bold".into())
}

pub fn grotesk_medium() -> FontFamily {
    FontFamily::Name("grotesk-medium".into())
}

pub fn doto() -> FontFamily {
    FontFamily::Name("doto".into())
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

impl Palette {
    /// Instrument-panel label: Space Mono, ALL CAPS, tracked, secondary grey.
    pub fn label(&self, text: impl AsRef<str>) -> RichText {
        RichText::new(text.as_ref().to_uppercase())
            .font(mono(LABEL))
            .extra_letter_spacing(LABEL_TRACKING)
            .color(self.text_secondary)
    }

    /// Data value in Space Mono at body size.
    pub fn data(&self, text: impl Into<String>) -> RichText {
        RichText::new(text)
            .font(mono(BODY))
            .color(self.text_primary)
    }

    /// Dot-matrix hero number. One per screen.
    pub fn hero(&self, text: impl Into<String>, size: f32) -> RichText {
        RichText::new(text)
            .font(FontId::new(size, doto()))
            .color(self.text_display)
    }

    /// Syntax colours: greyscale first, hierarchy by brightness and weight.
    pub fn syntax(&self, tok: Tok) -> (Color32, bool) {
        match tok {
            Tok::Label => (self.text_display, true),
            Tok::Mnemonic => (self.text_display, false),
            Tok::Register | Tok::Sfr | Tok::Plain => (self.text_primary, false),
            Tok::Number | Tok::Str | Tok::Directive => (self.text_secondary, false),
            Tok::Comment => (self.text_disabled, false),
        }
    }
}

fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8], tweak: FontTweak| {
        defs.font_data.insert(
            name.to_owned(),
            Arc::new(FontData::from_static(bytes).tweak(tweak)),
        );
    };
    add(
        "space-grotesk",
        include_bytes!("../assets/fonts/SpaceGrotesk-Regular.ttf"),
        FontTweak::default(),
    );
    add(
        "space-grotesk-medium",
        include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"),
        FontTweak::default(),
    );
    add(
        "space-mono",
        include_bytes!("../assets/fonts/SpaceMono-Regular.ttf"),
        FontTweak::default(),
    );
    add(
        "space-mono-bold",
        include_bytes!("../assets/fonts/SpaceMono-Bold.ttf"),
        FontTweak::default(),
    );
    // Round dots, bold enough to read as a display face.
    add(
        "doto",
        include_bytes!("../assets/fonts/Doto[ROND,wght].ttf"),
        FontTweak {
            coords: VariationCoords::new([(b"ROND", 100.0), (b"wght", 800.0)]),
            ..FontTweak::default()
        },
    );

    // egui's own fonts stay behind ours as fallbacks for symbols.
    let prop_fallback = defs.families[&FontFamily::Proportional].clone();
    let mono_fallback = defs.families[&FontFamily::Monospace].clone();
    let with = |first: &[&str], rest: &[String]| -> Vec<String> {
        first
            .iter()
            .map(|s| s.to_string())
            .chain(rest.iter().cloned())
            .collect()
    };
    defs.families.insert(
        FontFamily::Proportional,
        with(&["space-grotesk"], &prop_fallback),
    );
    defs.families
        .insert(FontFamily::Monospace, with(&["space-mono"], &mono_fallback));
    defs.families
        .insert(mono_bold(), with(&["space-mono-bold"], &mono_fallback));
    defs.families.insert(
        grotesk_medium(),
        with(&["space-grotesk-medium"], &prop_fallback),
    );
    defs.families
        .insert(doto(), with(&["doto", "space-mono"], &mono_fallback));
    defs
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let technical = CornerRadius::same(4);
    v.panel_fill = p.black;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border_visible);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_highlight_topmost = false;
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.faint_bg_color = p.surface_raised;
    v.code_bg_color = p.surface_raised;
    v.hyperlink_color = p.interactive;
    v.warn_fg_color = WARNING;
    v.error_fg_color = ACCENT;
    v.selection.bg_fill = if dark {
        Color32::from_white_alpha(46)
    } else {
        Color32::from_black_alpha(36)
    };
    v.selection.stroke = Stroke::new(1.0, p.text_display);
    v.text_cursor.stroke = Stroke::new(2.0, p.text_display);
    v.striped = false;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;
    v.slider_trailing_fill = true;
    v.resize_corner_size = 8.0;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_primary);
    w.noninteractive.corner_radius = technical;

    w.inactive.bg_fill = p.surface_raised;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border_visible);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text_primary);
    w.inactive.corner_radius = technical;
    w.inactive.expansion = 0.0;

    // Hover: the border and text brighten. No scale, no glow.
    w.hovered.bg_fill = p.surface_raised;
    w.hovered.weak_bg_fill = p.surface_raised;
    w.hovered.bg_stroke = Stroke::new(1.0, p.text_secondary);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text_display);
    w.hovered.corner_radius = technical;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.surface_raised;
    w.active.weak_bg_fill = p.surface_raised;
    w.active.bg_stroke = Stroke::new(1.0, p.text_primary);
    w.active.fg_stroke = Stroke::new(2.0, p.text_display);
    w.active.corner_radius = technical;
    w.active.expansion = 0.0;

    w.open = w.hovered;
    v
}

fn style(theme: Theme) -> Style {
    let (p, dark) = match theme {
        Theme::Dark => (&DARK, true),
        Theme::Light => (&LIGHT, false),
    };
    let mut s = Style {
        visuals: visuals(p, dark),
        ..Style::default()
    };
    s.text_styles = [
        (TextStyle::Small, FontId::monospace(LABEL)),
        (TextStyle::Body, FontId::proportional(BODY)),
        (TextStyle::Button, FontId::monospace(12.0)),
        (TextStyle::Heading, FontId::new(18.0, grotesk_medium())),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    s.spacing.item_spacing = egui::vec2(8.0, 4.0);
    s.spacing.button_padding = egui::vec2(10.0, 4.0);
    s.spacing.interact_size.y = 22.0;
    s.spacing.menu_margin = egui::Margin::same(8);
    s.spacing.window_margin = egui::Margin::same(16);
    s.spacing.menu_spacing = 2.0;
    s.animation_time = 0.15;
    s.interaction.selectable_labels = false;
    s.interaction.tooltip_delay = 0.4;
    s
}

/// Installs the fonts and both themes on `ctx`.
pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    ctx.set_style_of(Theme::Dark, style(Theme::Dark));
    ctx.set_style_of(Theme::Light, style(Theme::Light));
}
