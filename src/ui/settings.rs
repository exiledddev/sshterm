//! The Settings window: colours for the generated shell prompt.

use crate::prompt::{self, PromptTheme, Shell};
use crate::theme;
use crate::ui::widgets;
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
};

/// What the window asks the application to do.
pub enum SettingsOutcome {
    /// Still open, nothing to do.
    Pending,
    Closed,
    /// Re-source the prompt in any pane sitting at a fresh prompt.
    ApplyToOpenPanes,
}

pub struct SettingsWindow {
    theme: PromptTheme,
    /// The theme as it was when the window opened, for Revert.
    original: PromptTheme,
    status: Option<String>,
    error: Option<String>,
}

impl SettingsWindow {
    pub fn new() -> Self {
        let (theme, error) = PromptTheme::load();
        Self {
            theme,
            original: theme,
            status: None,
            error,
        }
    }

    pub fn theme(&self) -> PromptTheme {
        self.theme
    }

    /// Saves the theme and rewrites every generated rc file, so the next pane
    /// opened picks the colours up.
    fn persist(&mut self) {
        match self.theme.save() {
            Ok(()) => {
                self.error = None;
                self.status = Some("Saved. New panes use these colours.".into());
            }
            Err(e) => {
                self.error = Some(e);
                return;
            }
        }
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            if let Err(e) = self.theme.write_rc(shell) {
                self.error = Some(format!("Cannot write the {} rc file: {e}", shell.rc_name()));
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> SettingsOutcome {
        let mut outcome = SettingsOutcome::Pending;

        let modal = egui::Modal::new(egui::Id::new("acli-settings"))
            .backdrop_color(Color32::from_black_alpha(150))
            .frame(
                egui::Frame::new()
                    .fill(theme::glass(theme::floating_alpha()))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x28),
                    ))
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(egui::Margin::same(22))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 18],
                        blur: 48,
                        spread: 0,
                        color: Color32::from_black_alpha(170),
                    }),
            );

        let response = modal.show(ctx, |ui| {
            ui.set_width(440.0);

            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                icons_sliders(ui, rect);
                ui.label(
                    egui::RichText::new("Prompt colours")
                        .font(FontId::new(17.0, FontFamily::Proportional))
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::window_button(ui, crate::icons::win_close, false, "Close").clicked()
                    {
                        outcome = SettingsOutcome::Closed;
                    }
                });
            });
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(
                    "ACLI writes its own shell rc file, which sources yours first. \
                     Your dotfiles are never modified.",
                )
                .font(FontId::new(11.0, FontFamily::Proportional))
                .color(theme::TEXT_FAINT),
            );

            ui.add_space(14.0);
            self.preview(ui);
            ui.add_space(16.0);

            // ---- Swatches, two per row ---------------------------------
            let mut changed = false;
            let fields = prompt::FIELDS;
            for pair in fields.chunks(2) {
                ui.horizontal(|ui| {
                    let half = (ui.available_width() - 8.0) / 2.0;
                    for (label, get) in pair {
                        ui.allocate_ui_with_layout(
                            Vec2::new(half, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let rgb = get(&mut self.theme);
                                if ui.color_edit_button_srgb(rgb).changed() {
                                    changed = true;
                                }
                                ui.label(
                                    egui::RichText::new(*label)
                                        .font(FontId::new(12.0, FontFamily::Proportional))
                                        .color(theme::TEXT_DIM),
                                );
                            },
                        );
                    }
                });
                ui.add_space(6.0);
            }
            if changed {
                self.persist();
            }

            if let Some(error) = self.error.clone() {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(error)
                            .font(FontId::new(11.5, FontFamily::Proportional))
                            .color(theme::DANGER),
                    )
                    .wrap(),
                );
            } else if let Some(status) = self.status.clone() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(status)
                        .font(FontId::new(11.0, FontFamily::Proportional))
                        .color(theme::TEXT_FAINT),
                );
            }

            ui.add_space(12.0);
            theme::separator(ui);
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, "Reset to defaults", theme::TEXT_DIM).clicked() {
                    self.theme = PromptTheme::default();
                    self.persist();
                }
                if self.theme != self.original
                    && widgets::ghost_button(ui, "Revert", theme::TEXT_DIM).clicked()
                {
                    self.theme = self.original;
                    self.persist();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Done", true).clicked() {
                        outcome = SettingsOutcome::Closed;
                    }
                    if widgets::ghost_button(ui, "Apply to open panes", theme::ACCENT_ALT)
                        .on_hover_text(
                            "Re-sources the prompt in every pane that is sitting at an \
                             empty prompt. Panes running something are left alone.",
                        )
                        .clicked()
                    {
                        outcome = SettingsOutcome::ApplyToOpenPanes;
                    }
                });
            });
        });

        if response.should_close() && matches!(outcome, SettingsOutcome::Pending) {
            outcome = SettingsOutcome::Closed;
        }
        outcome
    }

    /// Draws the prompt as it will look, using the colours chosen above.
    fn preview(&self, ui: &mut egui::Ui) {
        let font = FontId::new(12.5, FontFamily::Monospace);
        let line_h = ui.ctx().fonts(|f| f.row_height(&font));
        let height = line_h * 2.0 + 22.0;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());

        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(10), theme::TERMINAL_BG);
        painter.rect_stroke(
            rect,
            CornerRadius::same(10),
            theme::island_stroke(),
            egui::StrokeKind::Inside,
        );

        let t = &self.theme;
        let col = |[r, g, b]: [u8; 3]| Color32::from_rgb(r, g, b);
        let mut x = rect.min.x + 11.0;
        let y1 = rect.min.y + 11.0 + line_h / 2.0;
        let y2 = y1 + line_h;

        let put = |painter: &egui::Painter, x: &mut f32, y: f32, text: &str, c: Color32| {
            let galley = painter
                .ctx()
                .fonts(|f| f.layout_no_wrap(text.to_string(), font.clone(), c));
            painter.galley(Pos2::new(*x, y - galley.rect.height() / 2.0), galley.clone(), c);
            *x += galley.rect.width();
        };

        put(painter, &mut x, y1, "╭─ ", col(t.frame));
        put(painter, &mut x, y1, "● ", col(t.ok));
        put(painter, &mut x, y1, "you", col(t.user));
        put(painter, &mut x, y1, "@", col(t.frame));
        put(painter, &mut x, y1, "host", col(t.host));
        put(painter, &mut x, y1, " ─ ", col(t.frame));
        put(painter, &mut x, y1, "~/projects/acli", col(t.path));
        put(painter, &mut x, y1, " ─ ", col(t.frame));
        put(painter, &mut x, y1, "main*", col(t.git));

        let mut x2 = rect.min.x + 11.0;
        put(painter, &mut x2, y2, "╰─", col(t.frame));
        put(painter, &mut x2, y2, "❯ ", col(t.arrow));
        put(painter, &mut x2, y2, "cargo build", Color32::from_rgb(0xD2, 0xD5, 0xE2));

        // The failure state, right-aligned so both are visible at once.
        let fail = "● [1] ❯";
        let galley = painter
            .ctx()
            .fonts(|f| f.layout_no_wrap(fail.to_string(), font.clone(), col(t.err)));
        painter.galley(
            Pos2::new(rect.max.x - 11.0 - galley.rect.width(), y2 - galley.rect.height() / 2.0),
            galley,
            col(t.err),
        );
    }
}

impl Default for SettingsWindow {
    fn default() -> Self {
        Self::new()
    }
}

fn icons_sliders(ui: &mut egui::Ui, rect: Rect) {
    crate::icons::sliders(ui.painter(), rect, theme::ACCENT_ALT, 1.6);
}
