//! Window chrome: the custom title bar and the ribbon navbar.
//!
//! The window is borderless and transparent, so SSCL draws its own title bar,
//! window controls and resize handles.

use crate::icons;
use crate::theme;
use crate::ui::widgets;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
    ViewportCommand,
};

/// Which ribbon button, if any, was pressed this frame.
#[derive(Default)]
pub struct RibbonActions {
    pub new_connection: bool,
    pub terminate: bool,
    pub open_folder: bool,
    pub toggle_left: bool,
    pub toggle_right: bool,
}

/// State the ribbon needs in order to render.
pub struct RibbonContext<'a> {
    pub session_title: &'a str,
    pub session_subtitle: &'a str,
    pub session_alive: bool,
    pub has_session: bool,
    pub left_open: bool,
    pub right_open: bool,
    pub maximized: bool,
}

/// Draws the whole top bar and returns the actions the user triggered.
pub fn top_bar(ctx: &egui::Context, cx: &RibbonContext<'_>) -> RibbonActions {
    let mut actions = RibbonActions::default();

    egui::TopBottomPanel::top("sscl-top")
        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 10,
            bottom: 4,
        }))
        .show_separator_line(false)
        .exact_height(102.0)
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            theme::frost(ui.painter(), rect, 16, theme::GLASS_BAR, theme::ACCENT);

            let inner = rect.shrink2(Vec2::new(12.0, 9.0));
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            title_row(&mut child, ctx, cx);
            child.add_space(6.0);
            ribbon_row(&mut child, cx, &mut actions);
        });

    actions
}

fn title_row(ui: &mut egui::Ui, ctx: &egui::Context, cx: &RibbonContext<'_>) {
    let row_h = 24.0;
    let full = Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(ui.available_width(), row_h),
    );

    // Anything not covered by a button drags the window.
    let drag = ui.interact(
        full,
        ui.id().with("titlebar-drag"),
        Sense::click_and_drag(),
    );
    if drag.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!cx.maximized));
    }

    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            let (mark_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
            icons::app_mark(ui.painter(), mark_rect);

            let painter = ui.painter().clone();
            let (text_rect, _) = ui.allocate_exact_size(Vec2::new(206.0, row_h), Sense::hover());
            theme::tracked_text(
                &painter,
                Pos2::new(text_rect.min.x + 103.0, text_rect.center().y),
                "SECURE SHELL COMMAND LINE",
                FontId::new(10.5, FontFamily::Proportional),
                theme::TEXT_DIM,
                1.6,
            );

            // Current session, centred-ish in the remaining space.
            if cx.has_session {
                ui.add_space(10.0);
                let dot_color = if cx.session_alive {
                    theme::ACCENT_ALT
                } else {
                    theme::DANGER
                };
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 3.5, dot_color);
                ui.label(
                    egui::RichText::new(cx.session_title)
                        .font(FontId::new(12.0, FontFamily::Proportional))
                        .color(theme::TEXT),
                );
                if !cx.session_subtitle.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("· {}", cx.session_subtitle))
                            .font(FontId::new(11.5, FontFamily::Monospace))
                            .color(theme::TEXT_FAINT),
                    );
                }
            }

            // Window controls, right aligned.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if widgets::window_button(ui, icons::win_close, true, "Close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                if widgets::window_button(
                    ui,
                    icons::win_max,
                    false,
                    if cx.maximized { "Restore" } else { "Maximise" },
                )
                .clicked()
                {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!cx.maximized));
                }
                if widgets::window_button(ui, icons::win_min, false, "Minimise").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            });
        },
    );
}

fn ribbon_row(ui: &mut egui::Ui, cx: &RibbonContext<'_>, actions: &mut RibbonActions) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        if widgets::ribbon_button(
            ui,
            icons::plus,
            "New Connection",
            theme::ACCENT_ALT,
            true,
            "Save a new SSH connection  (Ctrl+Shift+N)",
        )
        .clicked()
        {
            actions.new_connection = true;
        }

        if widgets::ribbon_button(
            ui,
            icons::power,
            "Terminate Session",
            theme::DANGER,
            cx.has_session,
            "Terminate the session on the centre page  (Ctrl+Shift+W)",
        )
        .clicked()
        {
            actions.terminate = true;
        }

        if widgets::ribbon_button(
            ui,
            icons::folder,
            "Open Connections Folder",
            theme::WARN,
            true,
            "Open the app folder in your file browser  (Ctrl+Shift+O)",
        )
        .clicked()
        {
            actions.open_folder = true;
        }

        ui.add_space(4.0);
        divider(ui);
        ui.add_space(4.0);

        if widgets::icon_button(
            ui,
            icons::panel_left,
            32.0,
            theme::TEXT_DIM,
            cx.left_open,
            "Toggle the Session Browser  (Ctrl+Shift+B)",
        )
        .clicked()
        {
            actions.toggle_left = true;
        }
        if widgets::icon_button(
            ui,
            icons::panel_right,
            32.0,
            theme::TEXT_DIM,
            cx.right_open,
            "Toggle Session Info  (Ctrl+Shift+I)",
        )
        .clicked()
        {
            actions.toggle_right = true;
        }
    });
}

fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 22.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x22)),
    );
}

/// Thin invisible strips along the window edges that start a resize.
/// Needed because the window has no decorations of its own.
pub fn resize_handles(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as D;
    const GRIP: f32 = 6.0;

    let screen = ctx.screen_rect();
    let edges: [(&str, Rect, D, egui::CursorIcon); 8] = [
        (
            "n",
            Rect::from_min_max(
                screen.left_top() + Vec2::new(GRIP, 0.0),
                Pos2::new(screen.right() - GRIP, screen.top() + GRIP),
            ),
            D::North,
            egui::CursorIcon::ResizeNorth,
        ),
        (
            "s",
            Rect::from_min_max(
                Pos2::new(screen.left() + GRIP, screen.bottom() - GRIP),
                screen.right_bottom() - Vec2::new(GRIP, 0.0),
            ),
            D::South,
            egui::CursorIcon::ResizeSouth,
        ),
        (
            "w",
            Rect::from_min_max(
                Pos2::new(screen.left(), screen.top() + GRIP),
                Pos2::new(screen.left() + GRIP, screen.bottom() - GRIP),
            ),
            D::West,
            egui::CursorIcon::ResizeWest,
        ),
        (
            "e",
            Rect::from_min_max(
                Pos2::new(screen.right() - GRIP, screen.top() + GRIP),
                Pos2::new(screen.right(), screen.bottom() - GRIP),
            ),
            D::East,
            egui::CursorIcon::ResizeEast,
        ),
        (
            "nw",
            Rect::from_min_size(screen.left_top(), Vec2::splat(GRIP)),
            D::NorthWest,
            egui::CursorIcon::ResizeNorthWest,
        ),
        (
            "ne",
            Rect::from_min_size(
                Pos2::new(screen.right() - GRIP, screen.top()),
                Vec2::splat(GRIP),
            ),
            D::NorthEast,
            egui::CursorIcon::ResizeNorthEast,
        ),
        (
            "sw",
            Rect::from_min_size(
                Pos2::new(screen.left(), screen.bottom() - GRIP),
                Vec2::splat(GRIP),
            ),
            D::SouthWest,
            egui::CursorIcon::ResizeSouthWest,
        ),
        (
            "se",
            Rect::from_min_size(
                Pos2::new(screen.right() - GRIP, screen.bottom() - GRIP),
                Vec2::splat(GRIP),
            ),
            D::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        ),
    ];

    egui::Area::new(egui::Id::new("sscl-resize"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            for (name, rect, dir, cursor) in edges {
                let r = ui.interact(
                    rect,
                    egui::Id::new(("sscl-resize", name)),
                    Sense::click_and_drag(),
                );
                if r.hovered() || r.dragged() {
                    ui.ctx().set_cursor_icon(cursor);
                }
                if r.drag_started() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
                }
            }
        });
}

/// A slim strip listing the open sessions, drawn under the ribbon.
/// Returns the session the user asked to switch to, or close.
pub struct TabStripResult {
    pub activate: Option<u64>,
    pub close: Option<u64>,
}

pub fn tab_strip(
    ui: &mut egui::Ui,
    sessions: &[(u64, String, bool, bool)],
    active: Option<u64>,
) -> TabStripResult {
    let mut result = TabStripResult {
        activate: None,
        close: None,
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for (id, title, alive, remote) in sessions {
            let is_active = active == Some(*id);
            let font = FontId::new(12.0, FontFamily::Proportional);
            let tw = ui
                .ctx()
                .fonts(|f| f.layout_no_wrap(title.clone(), font.clone(), Color32::WHITE).rect.width());
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(tw + 54.0, 26.0), Sense::click());
            let hovered = response.hovered();
            let painter = ui.painter();

            let fill = if is_active {
                theme::ACCENT.gamma_multiply(0.30)
            } else if hovered {
                Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x16)
            } else {
                Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x08)
            };
            painter.rect_filled(rect, CornerRadius::same(8), fill);
            if is_active {
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(8),
                    Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.85)),
                    egui::StrokeKind::Inside,
                );
            }

            let icon_rect =
                Rect::from_center_size(Pos2::new(rect.min.x + 15.0, rect.center().y), Vec2::splat(13.0));
            let tint = if !*alive {
                theme::DANGER
            } else if is_active {
                Color32::WHITE
            } else {
                theme::TEXT_DIM
            };
            if *remote {
                icons::server(painter, icon_rect, tint, 1.4);
            } else {
                icons::terminal(painter, icon_rect, tint, 1.4);
            }
            painter.text(
                Pos2::new(rect.min.x + 26.0, rect.center().y),
                Align2::LEFT_CENTER,
                title,
                font,
                if is_active { theme::TEXT } else { theme::TEXT_DIM },
            );

            // Close affordance on the right of the tab.
            let close_rect =
                Rect::from_center_size(Pos2::new(rect.max.x - 13.0, rect.center().y), Vec2::splat(14.0));
            let close_hover = ui.rect_contains_pointer(close_rect);
            if hovered || is_active {
                icons::win_close(
                    painter,
                    close_rect.shrink(3.0),
                    if close_hover { theme::DANGER } else { theme::TEXT_FAINT },
                    1.4,
                );
            }

            if response.clicked() {
                if close_hover {
                    result.close = Some(*id);
                } else {
                    result.activate = Some(*id);
                }
            }
        }
    });
    result
}
