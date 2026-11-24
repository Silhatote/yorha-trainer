//! YoRHa Trainer — a single-player trainer for NieR:Automata (Steam).
//!
//! © 2026 Oz Vessalius (@Silhatote). MIT license. Not affiliated with Square Enix or
//! PlatinumGames.

// No console window in the release executable.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod game;
mod picker;
mod scene;
mod settings;
mod sprites;
mod theme;
mod trainer;
mod ui;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    let icon = ui::icon_image();
    let viewport = egui::ViewportBuilder::default()
        .with_title("YoRHa Trainer")
        .with_app_id("yorha-trainer")
        .with_inner_size([1120.0, 730.0])
        .with_min_inner_size([920.0, 620.0])
        .with_decorations(false)
        .with_icon(icon);
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "YoRHa Trainer",
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
    )
}
