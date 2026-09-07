//! The New Connection / Edit Connection modal, including the drag-and-drop
//! private-key target and a small built-in file browser used as a fallback.

use crate::icons;
use crate::store::{self, Connection, NewConnection};
use crate::theme;
use crate::ui::widgets;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
};
use std::path::{Path, PathBuf};

/// Whether the dialog is creating or editing.
pub enum DialogMode {
    New,
    Edit(Box<Connection>),
}

/// What the dialog asks the application to do when it closes.
pub enum DialogOutcome {
    /// Still open, nothing to do.
    Pending,
    Closed,
    Create(NewConnection),
    Update {
        original: Box<Connection>,
        request: NewConnection,
        new_key: Option<PathBuf>,
    },
    Delete(Box<Connection>),
}

pub struct ConnectionDialog {
    pub mode: DialogMode,
    name: String,
    host: String,
    username: String,
    port: String,
    note: String,
    key_source: Option<PathBuf>,
    key_warning: Option<String>,
    pub error: Option<String>,
    picker: Option<FilePicker>,
    confirm_delete: bool,
}

impl Default for ConnectionDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionDialog {
    pub fn new() -> Self {
        Self {
            mode: DialogMode::New,
            name: String::new(),
            host: String::new(),
            username: String::new(),
            port: "22".into(),
            note: String::new(),
            key_source: None,
            key_warning: None,
            error: None,
            picker: None,
            confirm_delete: false,
        }
    }

    pub fn edit(conn: &Connection) -> Self {
        Self {
            name: conn.name.clone(),
            host: conn.host.clone(),
            username: conn.username.clone(),
            port: conn.port.to_string(),
            note: conn.note.clone(),
            mode: DialogMode::Edit(Box::new(conn.clone())),
            key_source: None,
            key_warning: None,
            error: None,
            picker: None,
            confirm_delete: false,
        }
    }

    fn request(&self) -> NewConnection {
        NewConnection {
            name: self.name.clone(),
            host: self.host.clone(),
            username: self.username.clone(),
            port: self.port.trim().parse().unwrap_or(22),
            note: self.note.clone(),
            key_source: self.key_source.clone(),
        }
    }

    fn accept_key(&mut self, path: PathBuf) {
        self.key_warning = store::looks_like_private_key(&path).map(str::to_string);
        self.key_source = Some(path);
    }

    pub fn show(&mut self, ctx: &egui::Context) -> DialogOutcome {
        // Accept files dropped anywhere over the window while the dialog is up.
        let (hovering, dropped) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect::<Vec<_>>(),
            )
        });
        if let Some(path) = dropped.into_iter().next() {
            self.accept_key(path);
        }

        let mut outcome = DialogOutcome::Pending;
        let editing = matches!(self.mode, DialogMode::Edit(_));
        let title = if editing {
            "Edit Connection"
        } else {
            "New Connection"
        };

        let modal = egui::Modal::new(egui::Id::new("sscl-connection-dialog"))
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
            ui.set_width(470.0);

            // ---- Header ---------------------------------------------------
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                icons::server(ui.painter(), rect, theme::ACCENT_ALT, 1.6);
                ui.label(
                    egui::RichText::new(title)
                        .font(FontId::new(17.0, FontFamily::Proportional))
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::window_button(ui, icons::win_close, false, "Close").clicked() {
                        outcome = DialogOutcome::Closed;
                    }
                });
            });
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(
                    "Saved to its own folder in the app folder, together with the private key.",
                )
                .font(FontId::new(11.0, FontFamily::Proportional))
                .color(theme::TEXT_FAINT),
            );
            ui.add_space(14.0);

            if let Some(picker) = &mut self.picker {
                match picker.show(ui) {
                    PickerOutcome::Pending => {}
                    PickerOutcome::Cancelled => self.picker = None,
                    PickerOutcome::Picked(path) => {
                        self.accept_key(path);
                        self.picker = None;
                    }
                }
                return;
            }

            // ---- Fields ---------------------------------------------------
            widgets::field(ui, "Session name", "Production web server", &mut self.name);
            widgets::field(ui, "Public IP address", "203.0.113.42", &mut self.host);
            ui.horizontal_top(|ui| {
                let full = ui.available_width();
                ui.allocate_ui_with_layout(
                    Vec2::new(full * 0.64, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| widgets::field(ui, "Username", "root", &mut self.username),
                );
                ui.allocate_ui_with_layout(
                    Vec2::new(full * 0.30, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| widgets::field(ui, "Port", "22", &mut self.port),
                );
            });
            widgets::field(ui, "Note (optional)", "Anything worth remembering", &mut self.note);

            // ---- Private key drop target ----------------------------------
            ui.label(
                egui::RichText::new("Private key")
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);
            self.drop_zone(ui, hovering, editing);

            if let Some(warning) = self.key_warning.clone() {
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    ui.label(
                        egui::RichText::new("⚠")
                            .font(FontId::new(12.0, FontFamily::Proportional))
                            .color(theme::WARN),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(warning)
                                .font(FontId::new(11.0, FontFamily::Proportional))
                                .color(theme::WARN),
                        )
                        .wrap(),
                    );
                });
            }

            if let Some(err) = self.error.clone() {
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(err)
                            .font(FontId::new(11.5, FontFamily::Proportional))
                            .color(theme::DANGER),
                    )
                    .wrap(),
                );
            }

            // ---- Command preview -----------------------------------------
            ui.add_space(12.0);
            let preview = self.preview();
            ui.label(
                egui::RichText::new(preview)
                    .font(FontId::new(10.5, FontFamily::Monospace))
                    .color(theme::TEXT_FAINT),
            );

            ui.add_space(14.0);
            theme::separator(ui);
            ui.add_space(4.0);

            // ---- Footer ---------------------------------------------------
            let valid = !self.name.trim().is_empty()
                && !self.host.trim().is_empty()
                && !self.username.trim().is_empty();

            ui.horizontal(|ui| {
                if editing && !self.confirm_delete {
                    if widgets::ghost_button(ui, "Delete", theme::DANGER).clicked() {
                        self.confirm_delete = true;
                    }
                } else if editing {
                    ui.label(
                        egui::RichText::new("Delete the folder and its key?")
                            .font(FontId::new(11.5, FontFamily::Proportional))
                            .color(theme::DANGER),
                    );
                    if widgets::ghost_button(ui, "Yes, delete", theme::DANGER).clicked()
                        && let DialogMode::Edit(original) = &self.mode {
                            outcome = DialogOutcome::Delete(original.clone());
                        }
                    if widgets::ghost_button(ui, "Keep", theme::TEXT_DIM).clicked() {
                        self.confirm_delete = false;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if editing { "Save" } else { "Create" };
                    if widgets::primary_button(ui, label, valid).clicked() && valid {
                        outcome = match &self.mode {
                            DialogMode::New => DialogOutcome::Create(self.request()),
                            DialogMode::Edit(original) => DialogOutcome::Update {
                                original: original.clone(),
                                request: self.request(),
                                new_key: self.key_source.clone(),
                            },
                        };
                    }
                    if widgets::ghost_button(ui, "Cancel", theme::TEXT_DIM).clicked() {
                        outcome = DialogOutcome::Closed;
                    }
                });
            });
        });

        if response.should_close()
            && matches!(outcome, DialogOutcome::Pending) {
                outcome = DialogOutcome::Closed;
            }
        outcome
    }

    fn preview(&self) -> String {
        let user = if self.username.trim().is_empty() {
            "user"
        } else {
            self.username.trim()
        };
        let host = if self.host.trim().is_empty() {
            "host"
        } else {
            self.host.trim()
        };
        let port = self.port.trim().parse::<u16>().unwrap_or(22);
        let mut cmd = String::from("ssh ");
        let key_name = match (&self.key_source, &self.mode) {
            (Some(p), _) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            (None, DialogMode::Edit(c)) => c.key_file.clone(),
            _ => String::new(),
        };
        if !key_name.is_empty() {
            cmd.push_str(&format!("-i <folder>/{key_name} -o IdentitiesOnly=yes "));
        }
        if port != 22 {
            cmd.push_str(&format!("-p {port} "));
        }
        cmd.push_str(&format!("{user}@{host}"));
        cmd
    }

    fn drop_zone(&mut self, ui: &mut egui::Ui, hovering: bool, editing: bool) {
        let height = 78.0;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        let active = hovering || response.hovered();
        let painter = ui.painter();

        painter.rect_filled(
            rect,
            CornerRadius::same(12),
            if hovering {
                theme::ACCENT_ALT.gamma_multiply(0.16)
            } else {
                Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 0x55)
            },
        );
        dashed_border(
            painter,
            rect,
            if active {
                theme::ACCENT_ALT
            } else {
                Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x30)
            },
        );

        let stored_key = match &self.mode {
            DialogMode::Edit(c) if !c.key_file.is_empty() => Some(c.key_file.clone()),
            _ => None,
        };

        let (line1, line2, tint) = match (&self.key_source, stored_key) {
            (Some(path), _) => (
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                format!("from {}", path.parent().unwrap_or(Path::new("")).display()),
                theme::ACCENT_ALT,
            ),
            (None, Some(existing)) => (
                existing,
                "stored in this connection's folder — drop a file to replace".to_string(),
                theme::TEXT_DIM,
            ),
            (None, None) => (
                if hovering {
                    "Release to use this key".to_string()
                } else {
                    "Drag & drop your private key here".to_string()
                },
                "or click to browse".to_string(),
                if hovering { theme::ACCENT_ALT } else { theme::TEXT_DIM },
            ),
        };

        icons::key(
            painter,
            Rect::from_center_size(Pos2::new(rect.min.x + 30.0, rect.center().y), Vec2::splat(20.0)),
            tint,
            1.6,
        );
        painter.text(
            Pos2::new(rect.min.x + 52.0, rect.center().y - 9.0),
            Align2::LEFT_CENTER,
            line1,
            FontId::new(12.5, FontFamily::Monospace),
            tint,
        );
        painter.text(
            Pos2::new(rect.min.x + 52.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            line2,
            FontId::new(10.5, FontFamily::Proportional),
            theme::TEXT_FAINT,
        );

        if response.clicked() {
            self.picker = Some(FilePicker::new());
        }
        let _ = editing;
        let _ = response.on_hover_text("The key is copied into the connection folder and chmod 400");
    }
}

fn dashed_border(painter: &egui::Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    let dash = 7.0;
    let gap = 5.0;
    let corners = [
        (rect.left_top(), rect.right_top()),
        (rect.right_top(), rect.right_bottom()),
        (rect.right_bottom(), rect.left_bottom()),
        (rect.left_bottom(), rect.left_top()),
    ];
    for (a, b) in corners {
        let dir = (b - a).normalized();
        let len = (b - a).length();
        let mut t = 0.0;
        while t < len {
            let end = (t + dash).min(len);
            painter.line_segment([a + dir * t, a + dir * end], stroke);
            t = end + gap;
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in file picker
// ---------------------------------------------------------------------------

enum PickerOutcome {
    Pending,
    Cancelled,
    Picked(PathBuf),
}

/// A deliberately small directory browser, used when a drag-and-drop is
/// inconvenient. It avoids pulling in a native dialog toolkit.
struct FilePicker {
    cwd: PathBuf,
    show_hidden: bool,
}

impl FilePicker {
    fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let ssh = home.join(".ssh");
        Self {
            cwd: if ssh.is_dir() { ssh } else { home },
            show_hidden: true,
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) -> PickerOutcome {
        let mut outcome = PickerOutcome::Pending;

        ui.horizontal(|ui| {
            if widgets::ghost_button(ui, "Up", theme::TEXT_DIM).clicked()
                && let Some(parent) = self.cwd.parent() {
                    self.cwd = parent.to_path_buf();
                }
            if let Some(home) = dirs::home_dir()
                && widgets::ghost_button(ui, "Home", theme::TEXT_DIM).clicked() {
                    self.cwd = home;
                }
            ui.checkbox(&mut self.show_hidden, "Hidden");
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(self.cwd.display().to_string())
                .font(FontId::new(11.0, FontFamily::Monospace))
                .color(theme::TEXT_FAINT),
        );
        ui.add_space(6.0);

        let mut dirs_list: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.cwd) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    dirs_list.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        dirs_list.sort();
        files.sort();

        egui::ScrollArea::vertical()
            .max_height(260.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for dir in &dirs_list {
                    let name = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!("📁  {name}"))
                                    .font(FontId::new(12.0, FontFamily::Proportional))
                                    .color(theme::TEXT_DIM),
                            )
                            .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        self.cwd = dir.clone();
                    }
                }
                for file in &files {
                    let name = file
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!("     {name}"))
                                    .font(FontId::new(12.0, FontFamily::Monospace))
                                    .color(theme::TEXT),
                            )
                            .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        outcome = PickerOutcome::Picked(file.clone());
                    }
                }
            });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::ghost_button(ui, "Cancel", theme::TEXT_DIM).clicked() {
                    outcome = PickerOutcome::Cancelled;
                }
            });
        });

        outcome
    }
}
