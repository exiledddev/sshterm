//! Start-up splash screen.
//!
//! Shown for two seconds on a fully transparent window, so the card appears
//! to float above whatever is on the desktop, then cross-fades into the app.

use crate::icons;
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Stroke, Vec2,
};

/// How long the card is shown before the fade begins.
pub const HOLD: f32 = 2.0;
/// Length of the cross-fade into the main window.
pub const FADE: f32 = 0.45;

/// Total lifetime of the splash.
pub fn total() -> f32 {
    HOLD + FADE
}

/// Draws the splash. `elapsed` is seconds since the app started.
pub fn draw(ctx: &egui::Context, elapsed: f32) {
    let fade = if elapsed <= HOLD {
        1.0
    } else {
        (1.0 - (elapsed - HOLD) / FADE).clamp(0.0, 1.0)
    };
    // Ease the card out with a slight upward drift and shrink.
    let ease = 1.0 - (1.0 - fade) * (1.0 - fade);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            let screen = ui.max_rect();
            let painter = ui.painter();

            let size = Vec2::new(620.0, 340.0);
            let mut card = Rect::from_center_size(screen.center(), size * (0.96 + 0.04 * ease));
            card = card.translate(Vec2::new(0.0, -12.0 * (1.0 - ease)));

            // Drop shadow.
            painter.rect_filled(
                card.translate(Vec2::new(0.0, 16.0)).expand(6.0),
                CornerRadius::same(28),
                Color32::from_black_alpha((110.0 * fade) as u8),
            );
            theme::frost(
                painter,
                card,
                24,
                (theme::floating_alpha() as f32 * fade) as u8,
                theme::ACCENT.gamma_multiply(fade),
            );

            let fade_c = |c: Color32| c.gamma_multiply(fade);

            // Mark.
            let mark = Rect::from_center_size(
                Pos2::new(card.center().x, card.min.y + 74.0),
                Vec2::splat(58.0),
            );
            icons::app_mark(painter, mark);

            // Title.
            theme::tracked_text(
                painter,
                Pos2::new(card.center().x, card.min.y + 148.0),
                "SECURE SHELL COMMAND LINE",
                FontId::new(27.0, FontFamily::Proportional),
                fade_c(theme::TEXT),
                5.5,
            );
            theme::tracked_text(
                painter,
                Pos2::new(card.center().x, card.min.y + 178.0),
                "S S C L",
                FontId::new(11.0, FontFamily::Proportional),
                fade_c(theme::ACCENT_ALT),
                6.0,
            );

            // Hairline.
            painter.line_segment(
                [
                    Pos2::new(card.center().x - 130.0, card.min.y + 204.0),
                    Pos2::new(card.center().x + 130.0, card.min.y + 204.0),
                ],
                Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, (0x22 as f32 * fade) as u8),
                ),
            );

            // Spinner.
            let t = ui.input(|i| i.time) as f32;
            spinner(
                painter,
                Pos2::new(card.center().x, card.min.y + 248.0),
                17.0,
                t,
                fade,
            );

            painter.text(
                Pos2::new(card.center().x, card.max.y - 34.0),
                Align2::CENTER_CENTER,
                "starting session manager",
                FontId::new(11.5, FontFamily::Proportional),
                fade_c(theme::TEXT_FAINT),
            );
            painter.text(
                Pos2::new(card.max.x - 20.0, card.max.y - 14.0),
                Align2::RIGHT_BOTTOM,
                concat!("v", env!("CARGO_PKG_VERSION")),
                FontId::new(10.0, FontFamily::Proportional),
                fade_c(theme::TEXT_FAINT),
            );

            ctx.request_repaint();
        });
}

/// A modern indeterminate ring: a single line whose sweep grows and shrinks
/// as it rotates, the way the Windows 11 boot spinner behaves.
pub fn spinner(painter: &egui::Painter, center: Pos2, radius: f32, time: f32, alpha: f32) {
    // Faint track.
    ring(
        painter,
        center,
        radius,
        0.0,
        std::f32::consts::TAU,
        Stroke::new(
            2.5,
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, (0x18 as f32 * alpha) as u8),
        ),
    );

    let period = 1.4;
    let phase = (time / period).fract();
    // Ease the head and the tail with offset curves so the arc breathes.
    let head = ease_in_out(phase.min(1.0));
    let tail = ease_in_out((phase - 0.32).max(0.0) / 0.68);
    let spin = time * 2.6;

    let start = spin + tail * std::f32::consts::TAU;
    let end = spin + head * std::f32::consts::TAU;
    let (start, end) = if end < start {
        (end, start)
    } else {
        (start, end)
    };
    // Never let the arc vanish completely.
    let end = if end - start < 0.35 { start + 0.35 } else { end };

    ring(
        painter,
        center,
        radius,
        start,
        end,
        Stroke::new(2.8, theme::ACCENT_ALT.gamma_multiply(alpha)),
    );
    ring(
        painter,
        center,
        radius,
        start,
        start + (end - start) * 0.45,
        Stroke::new(2.8, theme::ACCENT.gamma_multiply(alpha)),
    );
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

fn ring(painter: &egui::Painter, center: Pos2, radius: f32, from: f32, to: f32, stroke: Stroke) {
    let sweep = to - from;
    if sweep <= 0.0 {
        return;
    }
    let steps = ((sweep / 0.12).ceil() as usize).clamp(2, 128);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = from + sweep * i as f32 / steps as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, stroke));
}
