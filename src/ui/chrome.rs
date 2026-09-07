//! Window chrome: the custom title bar and the ribbon navbar.
//!
//! The window is borderless and transparent, so ACLI draws its own title bar,
//! window controls and resize handles.

use crate::icons;
use crate::theme;
use crate::ui::widgets;
use eframe::egui::{
    self, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
    ViewportCommand,
};

/// Which ribbon button, if any, was pressed this frame.
#[derive(Default)]
pub struct RibbonActions {
    pub split_right: bool,
    pub split_down: bool,
    pub close_pane: bool,
    pub settings: bool,
}

/// State the ribbon needs in order to render.
pub struct RibbonContext<'a> {
    /// Program running in the focused pane, e.g. `bash`.
    pub shell: &'a str,
    /// How many panes are open.
    pub panes: usize,
    /// Grid size of the focused pane.
    pub grid: Option<(u16, u16)>,
    pub settings_open: bool,
    pub maximized: bool,
}

/// Draws the whole top bar and returns the actions the user triggered.
pub fn top_bar(ctx: &egui::Context, cx: &RibbonContext<'_>) -> RibbonActions {
    let mut actions = RibbonActions::default();

    egui::TopBottomPanel::top("acli-top")
        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 8,
            bottom: 8,
        }))
        .show_separator_line(false)
        .exact_height(88.0)
        .show(ctx, |ui| {
            // No background and no border of its own: the title bar and the
            // ribbon sit directly on the window's glass.
            let rect = ui.max_rect();
            ui.expand_to_include_rect(rect);

            let inner = rect.shrink2(Vec2::new(0.0, 2.0));
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            title_row(&mut child, ctx, cx);
            child.add_space(8.0);
            ribbon_row(&mut child, cx, &mut actions);
        });

    actions
}

fn title_row(ui: &mut egui::Ui, ctx: &egui::Context, cx: &RibbonContext<'_>) {
    let row_h = 24.0;
    let full = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), row_h));

    // Anything not covered by a button drags the window.
    let drag = ui.interact(full, ui.id().with("titlebar-drag"), Sense::click_and_drag());
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
            let (text_rect, _) = ui.allocate_exact_size(Vec2::new(244.0, row_h), Sense::hover());
            theme::tracked_text(
                &painter,
                Pos2::new(text_rect.min.x + 122.0, text_rect.center().y),
                "AMPLIFIED COMMAND LINE INTERFACE",
                FontId::new(10.5, FontFamily::Proportional),
                theme::TEXT_DIM,
                1.6,
            );

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(cx.shell)
                    .font(FontId::new(11.5, FontFamily::Monospace))
                    .color(theme::TEXT_FAINT),
            );
            if let Some((cols, rows)) = cx.grid {
                ui.label(
                    egui::RichText::new(format!("{cols}×{rows}"))
                        .font(FontId::new(11.0, FontFamily::Monospace))
                        .color(theme::TEXT_FAINT),
                );
            }
            if cx.panes > 1 {
                ui.label(
                    egui::RichText::new(format!("· {} panes", cx.panes))
                        .font(FontId::new(11.0, FontFamily::Proportional))
                        .color(theme::TEXT_FAINT),
                );
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
            icons::split_right,
            "Split Right",
            theme::ACCENT_ALT,
            true,
            "Open another terminal beside this one  (Ctrl+Shift+R)",
        )
        .clicked()
        {
            actions.split_right = true;
        }

        if widgets::ribbon_button(
            ui,
            icons::split_down,
            "Split Down",
            theme::ACCENT_ALT,
            true,
            "Open another terminal below this one  (Ctrl+Shift+D)",
        )
        .clicked()
        {
            actions.split_down = true;
        }

        if widgets::ribbon_button(
            ui,
            icons::close_pane,
            "Close Pane",
            theme::DANGER,
            cx.panes > 1,
            "Close the focused pane  (Ctrl+Shift+W)",
        )
        .clicked()
        {
            actions.close_pane = true;
        }

        ui.add_space(4.0);
        divider(ui);
        ui.add_space(4.0);

        if widgets::icon_button(
            ui,
            icons::sliders,
            32.0,
            theme::TEXT_DIM,
            cx.settings_open,
            "Prompt colours  (Ctrl+Shift+,)",
        )
        .clicked()
        {
            actions.settings = true;
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

    egui::Area::new(egui::Id::new("acli-resize"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            for (name, rect, dir, cursor) in edges {
                let r = ui.interact(
                    rect,
                    egui::Id::new(("acli-resize", name)),
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
