// No console window behind the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod chrome;
mod editor;
mod icons;
mod layout;
mod modals;
mod panes;
mod state;
mod theme;
mod vim;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use eframe::egui;

/// Keil µVision-style desktop IDE for the Intel 8051, built on bench51.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Assembly sources (.asm, .a51) or Intel HEX files (.hex, .ihx) to open.
    files: Vec<PathBuf>,

    /// Oscillator frequency in MHz, used to show elapsed time.
    #[arg(long)]
    clock: Option<f64>,

    /// Colour theme, overriding the saved choice.
    #[arg(long, value_enum)]
    theme: Option<state::ThemePref>,

    /// Editor keybindings, overriding the saved choice.
    #[arg(long, value_enum)]
    keymap: Option<state::Keymap>,
}

fn main() -> eframe::Result {
    let args = Args::parse();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("bench51")
            .with_app_id("bench51-gui")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(Arc::new(icons::app_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "bench51-gui",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::GuiApp::new(
                cc,
                args.files,
                args.clock,
                args.theme,
                args.keymap,
            )))
        }),
    )
}
