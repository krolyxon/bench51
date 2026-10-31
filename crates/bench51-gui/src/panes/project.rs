//! Project window: the target being built, open files, and the sources in
//! its folder.

use std::path::{Path, PathBuf};

use eframe::egui::{self, CursorIcon, FontId, Id, RichText, Sense, Ui};

use crate::state::{Action, State};
use crate::theme::{BODY, palette};
use crate::widgets;

const EXTENSIONS: [&str; 6] = ["asm", "a51", "s", "src", "hex", "ihx"];

/// Source files in `dir`, re-read at most every two seconds.
fn folder_files(ui: &Ui, dir: &Path) -> Vec<PathBuf> {
    let id = Id::unique(("project_dir", dir));
    let now = ui.input(|i| i.time);
    if let Some((t, files)) = ui.data(|d| d.get_temp::<(f64, Vec<PathBuf>)>(id))
        && now - t < 2.0
    {
        return files;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    files.sort();
    ui.data_mut(|d| d.insert_temp(id, (now, files.clone())));
    files
}

fn row(ui: &mut Ui, text: &str, color: egui::Color32, indent: f32) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.add(
            egui::Label::new(
                RichText::new(text)
                    .font(FontId::monospace(BODY - 1.0))
                    .color(color),
            )
            .sense(Sense::click())
            .truncate(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
    })
    .inner
}

pub fn show(ui: &mut Ui, st: &mut State) {
    let p = palette(ui.ctx());
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.label(p.label("Target"));
            ui.add_space(4.0);
            match st.doc() {
                Some(doc) => {
                    ui.label(
                        RichText::new(doc.target())
                            .font(FontId::new(18.0, crate::theme::grotesk_medium()))
                            .color(p.text_display),
                    );
                    let built = if doc.session.build.is_none() {
                        "not built"
                    } else if doc.stale() {
                        "source changed"
                    } else {
                        "built"
                    };
                    ui.label(p.label(built).color(p.text_disabled));
                }
                None => {
                    ui.label(p.label("none").color(p.text_disabled));
                }
            }

            widgets::section(ui, "Open files");
            let mut act = None;
            for (i, doc) in st.docs.iter().enumerate() {
                let color = if i == st.active {
                    p.text_display
                } else {
                    p.text_secondary
                };
                let name = if doc.dirty() {
                    format!("{} •", doc.name())
                } else {
                    doc.name()
                };
                if row(ui, &name, color, 12.0).clicked() {
                    act = Some(Action::Activate(i));
                }
            }

            let dir = st
                .doc()
                .and_then(|d| d.path.as_ref())
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .or_else(|| std::env::current_dir().ok());
            if let Some(dir) = dir {
                let shown = dir
                    .file_name()
                    .map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into());
                widgets::section(ui, &format!("Folder · {shown}"));
                let files = folder_files(ui, &dir);
                if files.is_empty() {
                    ui.label(p.label("no sources").color(p.text_disabled));
                }
                for f in files {
                    let open = st.docs.iter().any(|d| d.path.as_deref() == Some(&f));
                    let name = f
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let color = if open {
                        p.text_primary
                    } else {
                        p.text_secondary
                    };
                    if row(ui, &name, color, 12.0).double_clicked() {
                        act = Some(Action::OpenPath(f.clone()));
                    }
                }
            }
            if let Some(a) = act {
                st.actions.push(a);
            }
        });
}
