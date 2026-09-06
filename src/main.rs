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
            .with_icon(app_icon(64)),
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

/// Renders the application icon at runtime, so no binary asset is needed.
/// A rounded violet tile carrying the `>_` prompt mark.
fn app_icon(size: u32) -> egui::IconData {
    let s = size as f32;
    let radius = s * 0.24;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    /// One stroke of the icon glyph: two endpoints and a half-width, all in
    /// unit space so the icon can be rendered at any size.
    struct Stroke {
        a: (f32, f32),
        b: (f32, f32),
        half_width: f32,
    }

    // The `>` chevron and the `_` bar.
    let strokes = [
        Stroke { a: (0.30, 0.32), b: (0.48, 0.50), half_width: 0.055 },
        Stroke { a: (0.48, 0.50), b: (0.30, 0.68), half_width: 0.055 },
        Stroke { a: (0.56, 0.70), b: (0.74, 0.70), half_width: 0.055 },
    ];

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Rounded-rectangle coverage.
            let dx = (px - s / 2.0).abs() - (s / 2.0 - radius);
            let dy = (py - s / 2.0).abs() - (s / 2.0 - radius);
            let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt()
                + dx.max(dy).min(0.0)
                - radius;
            let tile = (0.5 - outside).clamp(0.0, 1.0);
            if tile <= 0.0 {
                continue;
            }

            // Vertical violet -> teal gradient.
            let t = py / s;
            let base = [
                lerp(0x6B as f32, 0x3B as f32, t),
                lerp(0x4A as f32, 0x86 as f32, t),
                lerp(0xF0 as f32, 0xD8 as f32, t),
            ];

            // Glyph coverage.
            let mut glyph = 0.0f32;
            for stroke in &strokes {
                let d = dist_to_segment(px / s, py / s, stroke.a, stroke.b);
                glyph = glyph.max(((stroke.half_width - d) / (1.5 / s)).clamp(0.0, 1.0));
            }

            let color = [
                lerp(base[0], 255.0, glyph),
                lerp(base[1], 255.0, glyph),
                lerp(base[2], 255.0, glyph),
            ];

            let i = ((y * size + x) * 4) as usize;
            rgba[i] = color[0] as u8;
            rgba[i + 1] = color[1] as u8;
            rgba[i + 2] = color[2] as u8;
            rgba[i + 3] = (tile * 255.0) as u8;
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn dist_to_segment(px: f32, py: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let (wx, wy) = (px - ax, py - ay);
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + vx * t, ay + vy * t);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}
