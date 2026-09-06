//! Small custom widgets shared by the chrome, sidebars and dialogs.

use crate::icons::Icon;
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui,
    Vec2,
};

/// A ribbon button: icon on the left, label on the right.
pub fn ribbon_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    tint: Color32,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let font = FontId::new(12.5, FontFamily::Proportional);
    let text_w = ui
        .ctx()
        .fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), tint).rect.width());
    let size = Vec2::new(text_w + 44.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = if enabled {
        response
    } else {
        // Keep the layout identical but swallow the click.
        response.on_disabled_hover_text(tooltip)
    };

    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let painter = ui.painter();

    let bg = if pressed {
        tint.gamma_multiply(0.34)
    } else if hovered {
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1E)
    } else {
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x0C)
    };
    painter.rect_filled(rect, CornerRadius::same(9), bg);
    painter.rect_stroke(
        rect,
        CornerRadius::same(9),
        Stroke::new(
            1.0,
            if hovered {
                tint.gamma_multiply(0.8)
            } else {
                Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x14)
            },
        ),
        egui::StrokeKind::Inside,
    );

    let fg = if enabled {
        if hovered { Color32::WHITE } else { tint }
    } else {
        theme::TEXT_FAINT
    };
    let icon_rect = Rect::from_center_size(
        egui::Pos2::new(rect.min.x + 19.0, rect.center().y),
        Vec2::splat(15.0),
    );
    icon(painter, icon_rect, fg, 1.6);
    painter.text(
        egui::Pos2::new(rect.min.x + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        fg,
    );

    if enabled && !tooltip.is_empty() {
        response.clone().on_hover_text(tooltip)
    } else {
        response
    }
}

/// A square icon-only button.
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    size: f32,
    tint: Color32,
    active: bool,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();

    let bg = if response.is_pointer_button_down_on() {
        tint.gamma_multiply(0.36)
    } else if active {
        tint.gamma_multiply(0.24)
    } else if hovered {
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1C)
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        painter.rect_filled(rect, CornerRadius::same(8), bg);
    }
    if active {
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0, tint.gamma_multiply(0.8)),
            egui::StrokeKind::Inside,
        );
    }
    let fg = if hovered || active {
        Color32::WHITE
    } else {
        tint
    };
    icon(painter, rect.shrink(size * 0.21), fg, 1.6);

    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A window-control button (minimise / maximise / close).
pub fn window_button(ui: &mut Ui, icon: Icon, danger: bool, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 22.0), Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(
            rect,
            CornerRadius::same(6),
            if danger {
                theme::DANGER.gamma_multiply(0.8)
            } else {
                Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x22)
            },
        );
    }
    let fg = if hovered { Color32::WHITE } else { theme::TEXT_DIM };
    icon(painter, rect.shrink2(Vec2::new(9.0, 5.0)), fg, 1.4);
    response.on_hover_text(tooltip)
}

/// A labelled text field used by the connection dialogs.
pub fn field(ui: &mut Ui, label: &str, hint: &str, value: &mut String) {
    ui.label(
        egui::RichText::new(label)
            .font(FontId::new(11.0, FontFamily::Proportional))
            .color(theme::TEXT_DIM),
    );
    ui.add_space(3.0);
    let edit = egui::TextEdit::singleline(value)
        .hint_text(hint)
        .desired_width(f32::INFINITY)
        .margin(egui::Margin::symmetric(10, 7))
        .background_color(Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0x66));
    ui.add(edit);
    ui.add_space(10.0);
}

/// A pill-shaped status badge.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    let font = FontId::new(10.5, FontFamily::Proportional);
    let galley = ui
        .ctx()
        .fonts(|f| f.layout_no_wrap(text.to_string(), font, color));
    let size = galley.rect.size() + Vec2::new(16.0, 7.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), color.gamma_multiply(0.18));
    painter.rect_stroke(
        rect,
        CornerRadius::same(255),
        Stroke::new(1.0, color.gamma_multiply(0.5)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.center() - galley.rect.size() / 2.0, galley, color);
}

/// A primary (filled) action button.
pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let font = FontId::new(13.0, FontFamily::Proportional);
    let w = ui
        .ctx()
        .fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE).rect.width());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(w + 34.0, 32.0), Sense::click());
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    let fill = if !enabled {
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x12)
    } else if response.is_pointer_button_down_on() {
        theme::ACCENT.gamma_multiply(0.75)
    } else if hovered {
        theme::ACCENT
    } else {
        theme::ACCENT.gamma_multiply(0.88)
    };
    painter.rect_filled(rect, CornerRadius::same(9), fill);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        if enabled { Color32::WHITE } else { theme::TEXT_FAINT },
    );
    response
}

/// A secondary (outlined) action button.
pub fn ghost_button(ui: &mut Ui, label: &str, tint: Color32) -> Response {
    let font = FontId::new(13.0, FontFamily::Proportional);
    let w = ui
        .ctx()
        .fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), tint).rect.width());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(w + 28.0, 32.0), Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, CornerRadius::same(9), tint.gamma_multiply(0.22));
    }
    painter.rect_stroke(
        rect,
        CornerRadius::same(9),
        Stroke::new(1.0, tint.gamma_multiply(if hovered { 0.9 } else { 0.45 })),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        if hovered { Color32::WHITE } else { tint },
    );
    response
}

/// A key/value row for the Session Info sidebar.
///
/// Long values (identification strings, fingerprints) drop onto their own
/// full-width line instead of being squeezed into a narrow column.
pub fn info_row(ui: &mut Ui, key: &str, value: &str, mono: bool) {
    const KEY_W: f32 = 88.0;
    const GAP: f32 = 8.0;

    let key_font = FontId::new(11.0, FontFamily::Proportional);
    let value_font = if mono {
        FontId::new(11.5, FontFamily::Monospace)
    } else {
        FontId::new(12.0, FontFamily::Proportional)
    };

    let avail = ui.available_width();
    let value_w = ui.ctx().fonts(|f| {
        f.layout_no_wrap(value.to_string(), value_font.clone(), Color32::WHITE)
            .rect
            .width()
    });

    if value_w <= avail - KEY_W - GAP {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.allocate_ui_with_layout(
                Vec2::new(KEY_W, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.label(
                        egui::RichText::new(key)
                            .font(key_font.clone())
                            .color(theme::TEXT_FAINT),
                    );
                },
            );
            ui.label(
                egui::RichText::new(value)
                    .font(value_font.clone())
                    .color(theme::TEXT),
            );
        });
    } else {
        ui.label(
            egui::RichText::new(key)
                .font(key_font)
                .color(theme::TEXT_FAINT),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(value)
                    .font(value_font)
                    .color(theme::TEXT),
            )
            .wrap(),
        );
    }
    ui.add_space(5.0);
}
