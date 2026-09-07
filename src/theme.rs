//! Colour palette, glass surfaces and shared drawing helpers for SSCL.
//!
//! The whole application is drawn on a transparent window: every chrome
//! surface (title bar, ribbon, sidebars, dialogs) is a translucent
//! "glass" panel, while the terminal in the centre is an opaque dark grey
//! sheet as required by the blueprint.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Shadow, Stroke, TextStyle,
    Ui, Vec2,
};
use std::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

/// Violet primary accent.
pub const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x5C, 0xFF);
/// Mint secondary accent, used for "live"/success states.
pub const ACCENT_ALT: Color32 = Color32::from_rgb(0x38, 0xE8, 0xC8);
/// Warm red, used for destructive actions and errors.
pub const DANGER: Color32 = Color32::from_rgb(0xFF, 0x5D, 0x73);
/// Amber, used for warnings and "connecting" states.
pub const WARN: Color32 = Color32::from_rgb(0xFF, 0xB3, 0x4D);

/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xF2);
/// Secondary text.
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x97, 0x9C, 0xB8);
/// Tertiary text / placeholders.
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x66, 0x6B, 0x85);

/// The opaque dark grey of the central terminal page (blueprint requirement).
pub const TERMINAL_BG: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x22);

/// Hairline separator drawn on top of glass.
pub const HAIRLINE: Color32 = Color32::from_rgba_premultiplied(0x2A, 0x2C, 0x3A, 0xB0);

/// Base tint of a glass panel. The alpha is what makes the desktop
/// behind the window show through.
pub fn glass(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0x11, 0x12, 0x1B, alpha)
}

/// Corner radius of the window itself.
pub const WINDOW_RADIUS: u8 = 12;
/// Corner radius of the terminal island.
pub const ISLAND_RADIUS: u8 = 10;
/// Space between the island and the chrome around it.
pub const GAP: f32 = 10.0;

/// How much the compositor is helping us.
///
/// When KWin is blurring what is behind the window, the glass can be thin and
/// the desktop reads as a soft wash. When nothing is blurring, the same alpha
/// would let a busy wallpaper show straight through the text, so the glass is
/// made much denser instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// A compositor is blurring the backdrop.
    Blurred,
    /// No blur: darken the glass so text stays legible over anything.
    Solid,
}

static SURFACE: AtomicU8 = AtomicU8::new(1);

/// Chooses the glass density. Call once, before the first frame.
pub fn set_surface(surface: Surface) {
    SURFACE.store(
        match surface {
            Surface::Blurred => 0,
            Surface::Solid => 1,
        },
        Ordering::Relaxed,
    );
}

pub fn surface() -> Surface {
    match SURFACE.load(Ordering::Relaxed) {
        0 => Surface::Blurred,
        _ => Surface::Solid,
    }
}

/// Explicit opacity, 0-100, from `SSCL_OPACITY`. Zero means "not set".
static OPACITY: AtomicU8 = AtomicU8::new(0);

/// Overrides the glass density entirely. `percent` is 1-100.
pub fn set_opacity_percent(percent: u8) {
    OPACITY.store(percent.min(100), Ordering::Relaxed);
}

/// Alpha of the single window-wide glass sheet.
pub fn backdrop_alpha() -> u8 {
    let override_pct = OPACITY.load(Ordering::Relaxed);
    if override_pct > 0 {
        return ((override_pct as u16 * 255) / 100) as u8;
    }
    match surface() {
        Surface::Blurred => 140,
        Surface::Solid => 210,
    }
}

/// Alpha of surfaces that float above it: dialogs, popups, the splash card.
/// Always denser than the backdrop — they sit on top of it and need to read
/// as a separate layer.
pub fn floating_alpha() -> u8 {
    backdrop_alpha().saturating_add(34)
}

/// Fill of the terminal island. Opaque, as the blueprint requires.
pub fn island_fill() -> Color32 {
    TERMINAL_BG
}

/// The island's outline, the one piece of structure in an otherwise flat UI.
pub fn island_stroke() -> Stroke {
    Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x14))
}

// ---------------------------------------------------------------------------
// Global style
// ---------------------------------------------------------------------------

/// Applies the SSCL look to an egui context.
pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.text_styles = [
        (TextStyle::Heading, FontId::new(21.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();

    let v = &mut style.visuals;
    v.dark_mode = true;
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = glass(floating_alpha());
    v.extreme_bg_color = Color32::from_rgba_unmultiplied(0x08, 0x09, 0x11, 0xC8);
    v.faint_bg_color = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x0A);
    // Left unset so egui can derive a properly dimmed colour for hint text
    // and disabled widgets from the palette below.
    v.override_text_color = None;
    v.hyperlink_color = ACCENT_ALT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x24));
    v.window_corner_radius = CornerRadius::same(14);
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 34,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(140),
    };

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    // This is what `Visuals::text_color()` returns, i.e. the default text
    // colour; `weak_text_color()` (hints, disabled) is greyed out from it.
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = CornerRadius::same(9);

    w.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x10);
    w.inactive.bg_fill = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x10);
    w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x18));
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = CornerRadius::same(9);

    w.hovered.weak_bg_fill = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x24);
    w.hovered.bg_fill = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x24);
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.75));
    w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.hovered.corner_radius = CornerRadius::same(9);
    w.hovered.expansion = 0.0;

    w.active.weak_bg_fill = ACCENT.gamma_multiply(0.55);
    w.active.bg_fill = ACCENT.gamma_multiply(0.55);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.active.corner_radius = CornerRadius::same(9);
    w.active.expansion = 0.0;

    w.open.weak_bg_fill = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1C);
    w.open.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = CornerRadius::same(9);

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.window_margin = Margin::same(16);
    style.spacing.interact_size.y = 24.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;

    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Surfaces
// ---------------------------------------------------------------------------

/// Paints the one glass sheet the whole window sits on.
///
/// Deliberately flat and untinted: a single low-opacity neutral wash with a
/// hairline edge, the way JetBrains' newer UI treats its window background.
/// Everything except the terminal island shares it — title bar, ribbon, both
/// sidebars, the status bar and the space between them.
pub fn window_backdrop(painter: &egui::Painter, rect: Rect) {
    let cr = CornerRadius::same(WINDOW_RADIUS);
    painter.rect_filled(rect, cr, glass(backdrop_alpha()));
    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1E)),
        egui::StrokeKind::Inside,
    );
}

/// Paints a floating surface: dialogs, popups, the splash card. Flat, like
/// the backdrop, but denser so it separates from what is behind it.
pub fn frost(painter: &egui::Painter, rect: Rect, radius: u8, alpha: u8) {
    let cr = CornerRadius::same(radius);
    painter.rect_filled(rect, cr, glass(alpha));
    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x24)),
        egui::StrokeKind::Inside,
    );
}

/// Horizontal hairline separator.
pub fn separator(ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    let y = ui.cursor().top() + 4.0;
    ui.painter().line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x14)),
    );
    ui.add_space(9.0);
}

/// A small uppercase section label.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(FontId::new(10.0, FontFamily::Proportional))
            .color(TEXT_FAINT),
    );
    ui.add_space(2.0);
}

/// Draws text one glyph at a time so that letter-spacing can be applied.
/// Returns the total advance width.
pub fn tracked_text(
    painter: &egui::Painter,
    center: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    tracking: f32,
) -> f32 {
    let ctx = painter.ctx();
    let glyphs: Vec<(String, f32)> = text
        .chars()
        .map(|c| {
            let s = c.to_string();
            let w = ctx.fonts(|f| f.layout_no_wrap(s.clone(), font.clone(), color).rect.width());
            (s, w)
        })
        .collect();
    let total: f32 =
        glyphs.iter().map(|(_, w)| w).sum::<f32>() + tracking * (glyphs.len().max(1) - 1) as f32;

    let mut x = center.x - total / 2.0;
    for (s, w) in &glyphs {
        painter.text(
            Pos2::new(x, center.y),
            egui::Align2::LEFT_CENTER,
            s,
            font.clone(),
            color,
        );
        x += w + tracking;
    }
    total
}
