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

/// Alpha of the single window-wide glass sheet.
pub fn backdrop_alpha() -> u8 {
    match surface() {
        Surface::Blurred => 168,
        Surface::Solid => 238,
    }
}

/// Alpha of surfaces that float above it: dialogs, popups, the splash card.
pub fn floating_alpha() -> u8 {
    match surface() {
        Surface::Blurred => 214,
        Surface::Solid => 246,
    }
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
/// Everything except the terminal shares this single surface: the title bar,
/// the ribbon, both sidebars and the gutters between them are all the same
/// pane of glass, divided by hairlines rather than by gaps.
pub fn window_backdrop(painter: &egui::Painter, rect: Rect) {
    let cr = CornerRadius::same(WINDOW_RADIUS);
    painter.rect_filled(rect, cr, glass(backdrop_alpha()));

    // A single soft highlight running down from the top edge, so the sheet
    // reads as glass rather than as flat paint.
    let bands = 26;
    let height = (rect.height() * 0.28).min(180.0);
    for i in 0..bands {
        let t0 = i as f32 / bands as f32;
        let t1 = (i + 1) as f32 / bands as f32;
        let falloff = (1.0 - t0).powf(2.2);
        let a = falloff * sheen_strength();
        if a < 0.6 {
            continue;
        }
        let band = Rect::from_min_max(
            Pos2::new(rect.min.x, rect.min.y + height * t0),
            Pos2::new(rect.max.x, rect.min.y + height * t1),
        );
        let mix = falloff * 0.7;
        let color = Color32::from_rgba_unmultiplied(
            lerp_u8(0xFF, ACCENT.r(), mix),
            lerp_u8(0xFF, ACCENT.g(), mix),
            lerp_u8(0xFF, ACCENT.b(), mix),
            a as u8,
        );
        let corner = if i == 0 {
            CornerRadius { nw: WINDOW_RADIUS, ne: WINDOW_RADIUS, sw: 0, se: 0 }
        } else {
            CornerRadius::ZERO
        };
        painter.rect_filled(band, corner, color);
    }

    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x22)),
        egui::StrokeKind::Inside,
    );
}

/// With real blur behind the window the painted sheen is redundant, so it is
/// dialled back; without blur it does the work on its own.
fn sheen_strength() -> f32 {
    match surface() {
        Surface::Blurred => 14.0,
        Surface::Solid => 24.0,
    }
}

/// Paints a floating frosted surface: dialogs, popups, the splash card.
pub fn frost(painter: &egui::Painter, rect: Rect, radius: u8, alpha: u8, tint: Color32) {
    let cr = CornerRadius::same(radius);
    painter.rect_filled(rect, cr, glass(alpha));

    let bands = 24;
    for i in 0..bands {
        let t0 = i as f32 / bands as f32;
        let t1 = (i + 1) as f32 / bands as f32;
        let falloff = (1.0 - t0).powf(2.4);
        let a = falloff * 26.0;
        if a < 0.6 {
            continue;
        }
        let band = Rect::from_min_max(
            Pos2::new(rect.min.x, rect.min.y + rect.height() * t0),
            Pos2::new(rect.max.x, rect.min.y + rect.height() * t1),
        );
        let mix = falloff * 0.75;
        let color = Color32::from_rgba_unmultiplied(
            lerp_u8(0xFF, tint.r(), mix),
            lerp_u8(0xFF, tint.g(), mix),
            lerp_u8(0xFF, tint.b(), mix),
            a as u8,
        );
        let corner = if i == 0 {
            CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 }
        } else {
            CornerRadius::ZERO
        };
        painter.rect_filled(band, corner, color);
    }

    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1E)),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.min.x + radius as f32, rect.min.y + 0.5),
            Pos2::new(rect.max.x - radius as f32, rect.min.y + 0.5),
        ],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x2E)),
    );
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8
}

/// The hairline that divides one region of the glass from the next.
pub fn divider_color() -> Color32 {
    Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1A)
}

/// A vertical divider between two panels.
pub fn divider_v(painter: &egui::Painter, x: f32, y0: f32, y1: f32) {
    painter.line_segment(
        [Pos2::new(x, y0), Pos2::new(x, y1)],
        Stroke::new(1.0, divider_color()),
    );
}

/// A horizontal divider between two panels.
pub fn divider_h(painter: &egui::Painter, y: f32, x0: f32, x1: f32) {
    painter.line_segment(
        [Pos2::new(x0, y), Pos2::new(x1, y)],
        Stroke::new(1.0, divider_color()),
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
