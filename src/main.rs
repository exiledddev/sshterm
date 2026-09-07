//! ACLI — Amplified Command Line Interface.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use acli::app;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Amplified Command Line Interface")
            .with_app_id("acli")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([520.0, 360.0])
            // Borderless + transparent: the desktop shows through the glass.
            .with_decorations(false)
            .with_transparent(true)
            .with_icon(egui::IconData {
                rgba: acli::icons::app_icon_rgba(64),
                width: 64,
                height: 64,
            }),
        // A transparent framebuffer needs an alpha channel in the GL surface.
        depth_buffer: 0,
        multisampling: 0,
        ..Default::default()
    };

    eframe::run_native(
        "ACLI",
        options,
        Box::new(|cc| Ok(Box::new(app::AcliApp::new(cc)))),
    )
}
