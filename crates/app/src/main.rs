//! Volt — a lightweight Roblox performance app for Windows.
//!
//! Volt reads what Windows and Roblox expose, applies a small set of reversible
//! tweaks and reports measured numbers only. It has no background service, no
//! updater and no telemetry.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod icon;
mod system;
mod theme;
mod views;
mod widgets;

use app::VoltApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Volt — Roblox Performance")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([980.0, 640.0])
            .with_icon(icon::window_icon()),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Volt",
        options,
        Box::new(|cc| Ok(Box::new(VoltApp::new(cc)))),
    )
}
