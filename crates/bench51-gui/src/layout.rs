//! Dockable panes: Keil's edit and debug window layouts on `egui_tiles`,
//! with Nothing-style tab bars and flat surface cards.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Rect, Response, Sense, Shape, Stroke, Ui, Vec2,
    WidgetText, vec2,
};
use egui_tiles::{
    Behavior, Container, LinearDir, ResizeState, SimplificationOptions, TabState, Tabs, Tile,
    TileId, Tiles, Tree, UiResponse,
};
use serde::{Deserialize, Serialize};

use crate::panes;
use crate::state::State;
use crate::theme::{self, palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    Project,
    Editor,
    Build,
    Registers,
    Memory,
    Disassembly,
    Symbols,
    Serial,
    Ports,
    Timers,
    Interrupts,
}

impl Pane {
    pub const ALL: [Pane; 11] = [
        Pane::Project,
        Pane::Editor,
        Pane::Build,
        Pane::Registers,
        Pane::Disassembly,
        Pane::Memory,
        Pane::Symbols,
        Pane::Serial,
        Pane::Ports,
        Pane::Timers,
        Pane::Interrupts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Pane::Project => "Project",
            Pane::Editor => "Editor",
            Pane::Build => "Build Output",
            Pane::Registers => "Registers",
            Pane::Memory => "Memory",
            Pane::Disassembly => "Disassembly",
            Pane::Symbols => "Symbols",
            Pane::Serial => "UART #1",
            Pane::Ports => "Ports",
            Pane::Timers => "Timers",
            Pane::Interrupts => "Interrupts",
        }
    }

    /// Panes this one is grouped with by default, to dock it next to them
    /// when it is reopened.
    fn neighbours(self) -> &'static [Pane] {
        match self {
            Pane::Project | Pane::Registers => &[Pane::Registers, Pane::Project],
            Pane::Editor | Pane::Disassembly => &[Pane::Disassembly, Pane::Editor],
            Pane::Build | Pane::Memory | Pane::Symbols => {
                &[Pane::Memory, Pane::Build, Pane::Symbols]
            }
            Pane::Serial | Pane::Ports | Pane::Timers | Pane::Interrupts => &[
                Pane::Serial,
                Pane::Ports,
                Pane::Timers,
                Pane::Interrupts,
                Pane::Memory,
            ],
        }
    }
}

const GAP: f32 = 6.0;
const TAB_BAR: f32 = 30.0;

fn tabs(tiles: &mut Tiles<Pane>, panes: &[Pane]) -> TileId {
    let ids = panes.iter().map(|&p| tiles.insert_pane(p)).collect();
    tiles.insert_tab_tile(ids)
}

fn linear(tiles: &mut Tiles<Pane>, dir: LinearDir, children: &[(TileId, f32)]) -> TileId {
    let ids: Vec<TileId> = children.iter().map(|c| c.0).collect();
    let id = match dir {
        LinearDir::Horizontal => tiles.insert_horizontal_tile(ids),
        LinearDir::Vertical => tiles.insert_vertical_tile(ids),
    };
    if let Some(Tile::Container(Container::Linear(l))) = tiles.get_mut(id) {
        for &(child, share) in children {
            l.shares.set_share(child, share);
        }
    }
    id
}

/// Keil's build-mode layout: project on the left, editor over build output.
pub fn edit_tree() -> Tree<Pane> {
    let mut t = Tiles::default();
    let project = tabs(&mut t, &[Pane::Project]);
    let editor = tabs(&mut t, &[Pane::Editor]);
    let build = tabs(&mut t, &[Pane::Build]);
    let right = linear(&mut t, LinearDir::Vertical, &[(editor, 0.7), (build, 0.3)]);
    let root = linear(
        &mut t,
        LinearDir::Horizontal,
        &[(project, 0.18), (right, 0.82)],
    );
    Tree::new(Id::unique("edit_layout"), root, t)
}

/// Keil's debug-mode layout: registers on the left, source and disassembly
/// in the middle, memory and peripherals along the bottom.
pub fn debug_tree() -> Tree<Pane> {
    let mut t = Tiles::default();
    let regs = tabs(&mut t, &[Pane::Registers, Pane::Project]);
    let editor = tabs(&mut t, &[Pane::Editor]);
    let disasm = tabs(&mut t, &[Pane::Disassembly]);
    let mem = tabs(&mut t, &[Pane::Memory, Pane::Symbols, Pane::Build]);
    let periph = tabs(
        &mut t,
        &[Pane::Serial, Pane::Ports, Pane::Timers, Pane::Interrupts],
    );
    let top = linear(
        &mut t,
        LinearDir::Horizontal,
        &[(editor, 0.62), (disasm, 0.38)],
    );
    let bottom = linear(
        &mut t,
        LinearDir::Horizontal,
        &[(mem, 0.56), (periph, 0.44)],
    );
    let right = linear(&mut t, LinearDir::Vertical, &[(top, 0.6), (bottom, 0.4)]);
    let root = linear(
        &mut t,
        LinearDir::Horizontal,
        &[(regs, 0.19), (right, 0.81)],
    );
    Tree::new(Id::unique("debug_layout"), root, t)
}

/// Brings `pane` to the front, docking it next to its usual neighbours if
/// it was closed.
pub fn show_pane(tree: &mut Tree<Pane>, pane: Pane) {
    if let Some(id) = tree.tiles.find_pane(&pane) {
        tree.make_active(|tid, _| tid == id);
        return;
    }
    let new = tree.tiles.insert_pane(pane);
    let target = pane
        .neighbours()
        .iter()
        .filter_map(|n| tree.tiles.find_pane(n))
        .filter_map(|id| tree.tiles.parent_of(id))
        .find(|&parent| {
            matches!(
                tree.tiles.get(parent),
                Some(Tile::Container(Container::Tabs(_)))
            )
        });
    if let Some(parent) = target
        && let Some(Tile::Container(Container::Tabs(t))) = tree.tiles.get_mut(parent)
    {
        t.add_child(new);
        t.set_active(new);
        return;
    }
    let tabs_id = tree.tiles.insert_container(Tabs::new(vec![new]));
    match tree.root {
        Some(root) => {
            if let Some(Tile::Container(c)) = tree.tiles.get_mut(root) {
                c.add_child(tabs_id);
            } else {
                tree.root = Some(tree.tiles.insert_horizontal_tile(vec![root, tabs_id]));
            }
        }
        None => tree.root = Some(tabs_id),
    }
}

/// Flat surface cards behind every visible tab group: the page shows
/// through the gaps between them.
pub fn card_shapes(tree: &Tree<Pane>, fill: Color32) -> Vec<Shape> {
    tree.tiles
        .iter()
        .filter(|(_, t)| matches!(t, Tile::Container(Container::Tabs(_))))
        .filter_map(|(&id, _)| tree.tiles.rect(id))
        .map(|r| Shape::rect_filled(r, CornerRadius::same(8), fill))
        .collect()
}

pub struct PaneBehavior<'a> {
    pub st: &'a mut State,
    /// Tile of the editor pane, whose group hides its tab bar.
    pub editor_tile: Option<TileId>,
}

impl Behavior<Pane> for PaneBehavior<'_> {
    fn pane_ui(&mut self, ui: &mut Ui, _tile_id: TileId, pane: &mut Pane) -> UiResponse {
        panes::show(*pane, ui, self.st);
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Pane) -> WidgetText {
        pane.title().into()
    }

    fn is_tab_closable(&self, tiles: &Tiles<Pane>, tile_id: TileId) -> bool {
        matches!(tiles.get(tile_id), Some(Tile::Pane(p)) if *p != Pane::Editor)
    }

    fn tab_ui(
        &mut self,
        tiles: &mut Tiles<Pane>,
        ui: &mut Ui,
        id: Id,
        tile_id: TileId,
        state: &TabState,
    ) -> Response {
        let p = palette(ui.ctx());
        let title = match tiles.get(tile_id) {
            Some(Tile::Pane(pane)) => pane.title(),
            _ => "Group",
        };
        let galley = WidgetText::from(p.label(title)).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Small,
        );
        let pad = 12.0;
        let close_w = if state.closable { 16.0 } else { 0.0 };
        let width = galley.size().x + 2.0 * pad + close_w;
        let (_, rect) = ui.allocate_space(vec2(width, ui.available_height()));
        let resp = ui
            .interact(rect, id, Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if ui.is_rect_visible(rect) && !state.is_being_dragged {
            let fg = if state.active {
                p.text_display
            } else if resp.hovered() {
                p.text_secondary
            } else {
                p.text_disabled
            };
            let text_rect = Rect::from_min_max(
                rect.min + vec2(pad, 0.0),
                rect.max - vec2(pad + close_w, 0.0),
            );
            let pos = Align2::LEFT_CENTER
                .align_size_within_rect(galley.size(), text_rect)
                .min;
            let text_w = galley.size().x;
            ui.painter().galley(pos, galley, fg);
            if state.active {
                ui.painter().hline(
                    pos.x..=pos.x + text_w,
                    rect.bottom() - 1.0,
                    Stroke::new(2.0, p.text_display),
                );
            }
            if state.closable {
                let c = Rect::from_center_size(
                    egui::pos2(rect.right() - pad - 4.0, rect.center().y),
                    Vec2::splat(12.0),
                );
                let close = ui
                    .interact(c, id.with("close"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.hovered() || close.hovered() || state.active {
                    let col = if close.hovered() {
                        p.text_display
                    } else {
                        p.text_disabled
                    };
                    let r = c.shrink(3.0);
                    let s = Stroke::new(1.2, col);
                    ui.painter()
                        .line_segment([r.left_top(), r.right_bottom()], s);
                    ui.painter()
                        .line_segment([r.right_top(), r.left_bottom()], s);
                }
                if (close.clicked() || resp.clicked_by(egui::PointerButton::Middle))
                    && self.on_tab_close(tiles, tile_id)
                {
                    tiles.remove(tile_id);
                }
            }
        }
        resp
    }

    fn is_tab_bar_visible(&self, _tile_id: TileId, tabs: &Tabs) -> bool {
        // The editor brings its own document tabs; a second bar above them
        // would only repeat "EDITOR".
        !(tabs.children.len() == 1 && self.editor_tile == Some(tabs.children[0]))
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        TAB_BAR
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        GAP
    }

    fn min_size(&self) -> f32 {
        64.0
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> Color32 {
        Color32::TRANSPARENT
    }

    fn tab_bar_hline_stroke(&self, visuals: &egui::Visuals) -> Stroke {
        let p = if visuals.dark_mode {
            &theme::DARK
        } else {
            &theme::LIGHT
        };
        Stroke::new(1.0, p.border)
    }

    fn resize_stroke(&self, style: &egui::Style, state: ResizeState) -> Stroke {
        let p = if style.visuals.dark_mode {
            &theme::DARK
        } else {
            &theme::LIGHT
        };
        match state {
            ResizeState::Idle => Stroke::NONE,
            ResizeState::Hovering => Stroke::new(1.0, p.text_disabled),
            ResizeState::Dragging => Stroke::new(1.0, p.text_display),
        }
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }

    fn drag_preview_color(&self, visuals: &egui::Visuals) -> Color32 {
        visuals.selection.bg_fill
    }

    fn drag_preview_stroke(&self, visuals: &egui::Visuals) -> Stroke {
        visuals.selection.stroke
    }
}
