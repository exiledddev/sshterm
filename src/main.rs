//! SSCL — Secure Shell Command Line.
//!
//! A terminal application for opening and remembering SSH connections.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use sscl::app;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Secure Shell Command Line")
            .with_app_id("sscl")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([880.0, 520.0])
            // Borderless + transparent: the desktop shows through the glass.
            .with_decorations(false)
            .with_transparent(true)
            .with_icon(egui::IconData {
                rgba: sscl::icons::app_icon_rgba(64),
                width: 64,
                height: 64,
            }),
        // A transparent framebuffer needs an alpha channel in the GL surface.
        depth_buffer: 0,
        multisampling: 0,
        ..Default::default()
    };

    eframe::run_native(
        "SSCL",
        options,
        Box::new(|cc| Ok(Box::new(app::SsclApp::new(cc)))),
    )
}
