//! Left sidebar — the Session Browser.
//!
//! Lists every saved connection. Each entry carries a Start button that opens
//! the session on the centre page, and an Edit button that reopens the
//! New Connection form pre-filled with the stored details.

use crate::icons;
use crate::store::Connection;
use crate::theme;
use crate::ui::widgets;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
};

/// What the user asked the browser to do this frame.
pub enum BrowserAction {
    /// Open a new session for this connection.
    Start(usize),
    /// Edit the stored details of this connection.
    Edit(usize),
    /// Bring an already-running session to the front.
    Focus(u64),
    /// Open the New Connection dialog.
    New,
}

/// Draws the sidebar. `running` maps a connection folder to a live session id.
pub fn show(
    ui: &mut egui::Ui,
    connections: &[Connection],
    running: &dyn Fn(&Connection) -> Option<(u64, bool)>,
    filter: &mut String,
    suggestions: &crate::term::suggest::Suggestions,
) -> Option<BrowserAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Session Browser")
                .font(FontId::new(13.0, FontFamily::Proportional))
                .color(theme::TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icons::plus, 24.0, theme::ACCENT_ALT, false, "New connection")
                .clicked()
            {
                action = Some(BrowserAction::New);
            }
        });
    });
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(format!(
            "{} saved connection{}",
            connections.len(),
            if connections.len() == 1 { "" } else { "s" }
        ))
        .font(FontId::new(10.5, FontFamily::Proportional))
        .color(theme::TEXT_FAINT),
    );
    ui.add_space(8.0);

    if connections.len() > 5 {
        ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text("Filter…")
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(9, 5))
                .background_color(Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0x55)),
        );
        ui.add_space(8.0);
    }

    // Reserve a footer strip, then let the list fill what is left.
    let avail = ui.available_rect_before_wrap();
    let footer_h = 18.0;
    let list_rect = egui::Rect::from_min_max(
        avail.min,
        Pos2::new(avail.right(), (avail.bottom() - footer_h - 8.0).max(avail.top())),
    );
    let footer_rect = egui::Rect::from_min_max(
        Pos2::new(avail.left(), avail.bottom() - footer_h),
        avail.max,
    );

    let needle = filter.to_lowercase();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(list_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
        |ui: &mut egui::Ui| {
            if connections.is_empty() {
                empty_state(ui, &mut action);
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (idx, conn) in connections.iter().enumerate() {
                        if !needle.is_empty() {
                            let hay = format!(
                                "{} {} {} {}",
                                conn.name, conn.host, conn.username, conn.note
                            )
                            .to_lowercase();
                            if !hay.contains(&needle) {
                                continue;
                            }
                        }
                        if let Some(a) = card(ui, idx, conn, running(conn)) {
                            action = Some(a);
                        }
                        ui.add_space(6.0);
                    }
                });
        },
    );

    footer(ui, footer_rect, suggestions);
    action
}

/// A quiet status line describing the autosuggestion database.
fn footer(ui: &mut egui::Ui, rect: Rect, suggestions: &crate::term::suggest::Suggestions) {
    let text = if suggestions.is_scanning() {
        "indexing $PATH…".to_string()
    } else {
        format!(
            "{} cmds · {} history",
            suggestions.command_count(),
            suggestions.history_count()
        )
    };
    let font = FontId::new(10.0, FontFamily::Proportional);
    let text = elide(ui.ctx(), &text, font.clone(), rect.width());
    let painter = ui.painter();
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.top() - 5.0),
            Pos2::new(rect.right(), rect.top() - 5.0),
        ],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x12)),
    );
    painter.text(
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        theme::TEXT_FAINT,
    );
    if suggestions.is_scanning() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
    }
}

fn empty_state(ui: &mut egui::Ui, action: &mut Option<BrowserAction>) {
    ui.add_space(18.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 76.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x18)),
        egui::StrokeKind::Inside,
    );
    icons::server(
        painter,
        Rect::from_center_size(Pos2::new(rect.center().x, rect.min.y + 24.0), Vec2::splat(22.0)),
        theme::TEXT_FAINT,
        1.5,
    );
    painter.text(
        Pos2::new(rect.center().x, rect.max.y - 22.0),
        Align2::CENTER_CENTER,
        "No saved connections yet",
        FontId::new(12.0, FontFamily::Proportional),
        theme::TEXT_DIM,
    );
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        if widgets::ghost_button(ui, "New Connection", theme::ACCENT_ALT).clicked() {
            *action = Some(BrowserAction::New);
        }
    });
}

fn card(
    ui: &mut egui::Ui,
    idx: usize,
    conn: &Connection,
    running: Option<(u64, bool)>,
) -> Option<BrowserAction> {
    let mut action = None;
    let width = ui.available_width();
    let height = 76.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

    let hovered = response.hovered();
    let live = running.map(|(_, alive)| alive).unwrap_or(false);
    let painter = ui.painter().clone();

    painter.rect_filled(
        rect,
        CornerRadius::same(11),
        if hovered {
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x18)
        } else {
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x0A)
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(11),
        Stroke::new(
            1.0,
            if live {
                theme::ACCENT_ALT.gamma_multiply(0.6)
            } else {
                Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x14)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if live {
        // Accent bar down the left edge plus a dot, so a running session is
        // obvious without stealing room from the text.
        painter.rect_filled(
            Rect::from_min_size(rect.min + Vec2::new(0.0, 12.0), Vec2::new(3.0, height - 24.0)),
            CornerRadius::same(2),
            theme::ACCENT_ALT,
        );
        painter.circle_filled(
            Pos2::new(rect.max.x - 13.0, rect.min.y + 17.0),
            3.5,
            theme::ACCENT_ALT,
        );
    }

    // --- Rows 1 and 2: identity, using the full card width ------------------
    let text_left = rect.min.x + 38.0;
    icons::server(
        &painter,
        Rect::from_center_size(Pos2::new(rect.min.x + 22.0, rect.min.y + 25.0), Vec2::splat(16.0)),
        if live { theme::ACCENT_ALT } else { theme::TEXT_DIM },
        1.5,
    );
    let name_font = FontId::new(13.0, FontFamily::Proportional);
    painter.text(
        Pos2::new(text_left, rect.min.y + 17.0),
        Align2::LEFT_CENTER,
        elide(
            ui.ctx(),
            &conn.name,
            name_font.clone(),
            (rect.max.x - if live { 26.0 } else { 12.0 } - text_left).max(20.0),
        ),
        name_font,
        theme::TEXT,
    );
    let target_font = FontId::new(11.0, FontFamily::Monospace);
    painter.text(
        Pos2::new(text_left, rect.min.y + 35.0),
        Align2::LEFT_CENTER,
        elide(
            ui.ctx(),
            &format!("{}@{}", conn.username, conn.host),
            target_font.clone(),
            (rect.max.x - 12.0 - text_left).max(20.0),
        ),
        target_font,
        theme::TEXT_FAINT,
    );

    // --- Row 3: stored key on the left, Start and Edit on the right ---------
    let start_font = FontId::new(11.0, FontFamily::Proportional);
    let start_label_w = ui.ctx().fonts(|f| {
        f.layout_no_wrap("Start".to_string(), start_font.clone(), Color32::WHITE)
            .rect
            .width()
    });
    let compact = width < 200.0;
    let start_w = if compact { 26.0 } else { start_label_w + 28.0 };

    let mut x = rect.max.x - 9.0;
    let mut slot = |w: f32| -> Rect {
        x -= w;
        let r = Rect::from_min_size(Pos2::new(x, rect.max.y - 32.0), Vec2::new(w, 26.0));
        x -= 5.0;
        r
    };
    let edit_rect = slot(26.0);
    let start_rect = slot(start_w);
    let buttons_left = x;

    let edit_resp = ui.interact(edit_rect, ui.id().with(("conn-edit", idx)), Sense::click());
    let start_resp = ui.interact(start_rect, ui.id().with(("conn-start", idx)), Sense::click());

    if conn.key_file.is_empty() {
        painter.text(
            Pos2::new(rect.min.x + 14.0, rect.max.y - 19.0),
            Align2::LEFT_CENTER,
            "no stored key",
            FontId::new(10.0, FontFamily::Proportional),
            theme::WARN.gamma_multiply(0.85),
        );
    } else {
        icons::key(
            &painter,
            Rect::from_center_size(
                Pos2::new(rect.min.x + 20.0, rect.max.y - 19.0),
                Vec2::splat(12.0),
            ),
            theme::TEXT_FAINT,
            1.2,
        );
        let key_font = FontId::new(10.0, FontFamily::Monospace);
        let key_left = rect.min.x + 30.0;
        painter.text(
            Pos2::new(key_left, rect.max.y - 19.0),
            Align2::LEFT_CENTER,
            elide(
                ui.ctx(),
                &conn.key_file,
                key_font.clone(),
                (buttons_left - 8.0 - key_left).max(16.0),
            ),
            key_font,
            theme::TEXT_FAINT,
        );
    }

    draw_mini(&painter, edit_rect, icons::pencil, theme::TEXT_DIM, edit_resp.hovered());
    let start_tint = if live { theme::ACCENT_ALT } else { theme::ACCENT };
    if compact {
        draw_mini(&painter, start_rect, icons::play, start_tint, start_resp.hovered());
    } else {
        draw_labelled(
            &painter,
            start_rect,
            icons::play,
            "Start",
            start_font,
            start_tint,
            start_resp.hovered(),
        );
    }

    if edit_resp.clicked() {
        action = Some(BrowserAction::Edit(idx));
    } else if start_resp.clicked() {
        action = Some(BrowserAction::Start(idx));
    } else if response.clicked() {
        action = match running {
            Some((id, true)) => Some(BrowserAction::Focus(id)),
            _ => Some(BrowserAction::Start(idx)),
        };
    }

    let _ = edit_resp.on_hover_text("Edit connection details");
    let _ = start_resp.on_hover_text(format!(
        "Start{}\n{}",
        if live { " another session" } else { "" },
        conn.command_preview()
    ));

    action
}

/// Shortens `text` with an ellipsis so it fits inside `max_width`.
fn elide(ctx: &egui::Context, text: &str, font: FontId, max_width: f32) -> String {
    let width = |s: &str| {
        ctx.fonts(|f| {
            f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
                .rect
                .width()
        })
    };
    if width(text) <= max_width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if width(&candidate) <= max_width {
            return candidate;
        }
    }
    String::new()
}

/// A pill button carrying both an icon and a label.
fn draw_labelled(
    painter: &egui::Painter,
    rect: Rect,
    icon: crate::icons::Icon,
    label: &str,
    font: FontId,
    tint: Color32,
    hovered: bool,
) {
    painter.rect_filled(
        rect,
        CornerRadius::same(8),
        if hovered {
            tint.gamma_multiply(0.42)
        } else {
            tint.gamma_multiply(0.22)
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, tint.gamma_multiply(if hovered { 0.95 } else { 0.5 })),
        egui::StrokeKind::Inside,
    );
    let fg = if hovered { Color32::WHITE } else { tint };
    icon(
        painter,
        Rect::from_center_size(Pos2::new(rect.min.x + 13.0, rect.center().y), Vec2::splat(11.0)),
        fg,
        1.4,
    );
    painter.text(
        Pos2::new(rect.min.x + 22.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        fg,
    );
}

fn draw_mini(
    painter: &egui::Painter,
    rect: Rect,
    icon: crate::icons::Icon,
    tint: Color32,
    hovered: bool,
) {
    if hovered {
        painter.rect_filled(rect, CornerRadius::same(7), tint.gamma_multiply(0.3));
    } else {
        painter.rect_filled(
            rect,
            CornerRadius::same(7),
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x10),
        );
    }
    icon(
        painter,
        rect.shrink(7.0),
        if hovered { Color32::WHITE } else { tint },
        1.5,
    );
}
