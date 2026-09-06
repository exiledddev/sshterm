//! Application state and top-level layout.

use crate::store::{self, Connection};
use crate::term::{self, TerminalState, pty::PtySession, suggest::Suggestions};
use crate::theme;
use crate::ui::chrome::{self, RibbonContext};
use crate::ui::dialogs::{ConnectionDialog, DialogOutcome};
use crate::ui::sidebar_left::{self, BrowserAction};
use crate::ui::sidebar_right::{self, InfoAction};
use crate::ui::splash;
use eframe::egui::{self, Color32};
use std::collections::HashMap;
use std::time::Instant;

const MIN_SIDEBAR: f32 = 210.0;
const MAX_SIDEBAR: f32 = 460.0;

pub struct SsclApp {
    started: Instant,
    splash_finished: bool,

    connections: Vec<Connection>,
    sessions: Vec<PtySession>,
    active: Option<u64>,
    term_states: HashMap<u64, TerminalState>,
    suggestions: Suggestions,

    dialog: Option<ConnectionDialog>,
    left_open: bool,
    right_open: bool,
    filter: String,
    font_size: f32,
    toast: Option<Toast>,
}

struct Toast {
    text: String,
    color: Color32,
    born: f64,
}

impl SsclApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let _ = store::ensure_dirs();
        Self {
            started: Instant::now(),
            splash_finished: false,
            connections: store::load_all(),
            sessions: Vec::new(),
            active: None,
            term_states: HashMap::new(),
            suggestions: Suggestions::new(),
            dialog: None,
            left_open: true,
            right_open: true,
            filter: String::new(),
            font_size: 13.5,
            toast: None,
        }
    }

    // -- helpers ----------------------------------------------------------

    fn active_session(&self) -> Option<&PtySession> {
        let id = self.active?;
        self.sessions.iter().find(|s| s.id == id)
    }

    fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>, color: Color32) {
        self.toast = Some(Toast {
            text: text.into(),
            color,
            born: ctx.input(|i| i.time),
        });
    }

    fn ensure_local_session(&mut self, ctx: &egui::Context) {
        if !self.sessions.is_empty() {
            return;
        }
        match PtySession::local(ctx, 100, 30) {
            Ok(session) => {
                self.active = Some(session.id);
                self.sessions.push(session);
            }
            Err(e) => self.notify(ctx, e, theme::DANGER),
        }
    }

    fn open_local(&mut self, ctx: &egui::Context) {
        match PtySession::local(ctx, 100, 30) {
            Ok(session) => {
                self.active = Some(session.id);
                self.sessions.push(session);
            }
            Err(e) => self.notify(ctx, e, theme::DANGER),
        }
    }

    fn start_connection(&mut self, ctx: &egui::Context, index: usize) {
        let Some(conn) = self.connections.get(index).cloned() else {
            return;
        };
        match PtySession::remote(ctx, &conn, 100, 30) {
            Ok(session) => {
                self.notify(
                    ctx,
                    format!("Connecting to {}…", conn.target()),
                    theme::ACCENT_ALT,
                );
                // Switch the centre page to the new session, as specified.
                self.active = Some(session.id);
                self.sessions.push(session);
            }
            Err(e) => self.notify(ctx, e, theme::DANGER),
        }
    }

    /// Terminates the current session; SSH sessions are then closed and the
    /// centre page falls back to another open session.
    fn terminate_active(&mut self, ctx: &egui::Context) {
        let Some(id) = self.active else { return };
        let Some(pos) = self.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        let remote = self.sessions[pos].kind.is_remote();
        self.sessions[pos].terminate();

        if remote {
            self.close_session(ctx, id);
            self.notify(ctx, "Session terminated.", theme::WARN);
        } else {
            // The local shell is the app's default page: give it a fresh one.
            self.sessions.remove(pos);
            self.term_states.remove(&id);
            self.active = None;
            self.open_local(ctx);
            self.notify(ctx, "Local shell restarted.", theme::WARN);
        }
    }

    fn close_session(&mut self, ctx: &egui::Context, id: u64) {
        let Some(pos) = self.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        self.sessions[pos].terminate();
        self.sessions.remove(pos);
        self.term_states.remove(&id);
        if self.active == Some(id) {
            self.active = self.sessions.last().map(|s| s.id);
            if self.sessions.is_empty() {
                self.open_local(ctx);
            }
        }
    }

    fn reload_connections(&mut self) {
        self.connections = store::load_all();
    }
}

impl eframe::App for SsclApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent: the desktop shows through everything but the
        // terminal sheet, which paints its own opaque background.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let elapsed = self.started.elapsed().as_secs_f32();

        // ---- Splash ------------------------------------------------------
        if !self.splash_finished {
            if elapsed < splash::total() {
                // The real UI fades in underneath the splash card.
                if elapsed > splash::HOLD {
                    self.ensure_local_session(ctx);
                    self.main_ui(ctx);
                }
                splash::draw(ctx, elapsed);
                return;
            }
            self.splash_finished = true;
        }

        self.ensure_local_session(ctx);
        self.shortcuts(ctx);
        self.main_ui(ctx);
        self.toast_ui(ctx);
    }
}

impl SsclApp {
    /// Global keyboard shortcuts. These are consumed here so the terminal
    /// never sees them.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let cs = Modifiers::CTRL | Modifiers::SHIFT;

        let hit = |ctx: &egui::Context, mods: Modifiers, key: Key| -> bool {
            ctx.input_mut(|i| i.consume_key(mods, key))
        };

        if hit(ctx, cs, Key::N) && self.dialog.is_none() {
            self.dialog = Some(ConnectionDialog::new());
        }
        if hit(ctx, cs, Key::W) {
            self.terminate_active(ctx);
        }
        if hit(ctx, cs, Key::O) {
            self.open_connections_folder(ctx);
        }
        if hit(ctx, cs, Key::B) {
            self.left_open = !self.left_open;
        }
        if hit(ctx, cs, Key::I) {
            self.right_open = !self.right_open;
        }
        if hit(ctx, cs, Key::T) {
            self.open_local(ctx);
        }
        if hit(ctx, Modifiers::CTRL, Key::Plus) || hit(ctx, Modifiers::CTRL, Key::Equals) {
            self.font_size = (self.font_size + 0.5).min(28.0);
        }
        if hit(ctx, Modifiers::CTRL, Key::Minus) {
            self.font_size = (self.font_size - 0.5).max(8.0);
        }
        if hit(ctx, Modifiers::CTRL, Key::Num0) {
            self.font_size = 13.5;
        }
    }

    fn open_connections_folder(&mut self, ctx: &egui::Context) {
        let dir = store::connections_dir();
        match store::open_in_file_browser(&dir) {
            Ok(()) => self.notify(ctx, format!("Opened {}", dir.display()), theme::ACCENT_ALT),
            Err(e) => self.notify(ctx, e, theme::DANGER),
        }
    }

    fn main_ui(&mut self, ctx: &egui::Context) {
        chrome::resize_handles(ctx);

        // ---- Top bar -----------------------------------------------------
        let (title, subtitle, alive) = match self.active_session() {
            Some(s) => (s.title.clone(), s.subtitle.clone(), s.is_alive()),
            None => (String::new(), String::new(), false),
        };
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let actions = chrome::top_bar(
            ctx,
            &RibbonContext {
                session_title: &title,
                session_subtitle: &subtitle,
                session_alive: alive,
                has_session: self.active.is_some(),
                left_open: self.left_open,
                right_open: self.right_open,
                maximized,
            },
        );

        if actions.new_connection && self.dialog.is_none() {
            self.dialog = Some(ConnectionDialog::new());
        }
        if actions.terminate {
            self.terminate_active(ctx);
        }
        if actions.open_folder {
            self.open_connections_folder(ctx);
        }
        if actions.toggle_left {
            self.left_open = !self.left_open;
        }
        if actions.toggle_right {
            self.right_open = !self.right_open;
        }

        // ---- Left sidebar -------------------------------------------------
        let mut browser_action = None;
        if self.left_open {
            egui::SidePanel::left("sscl-left")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                    left: 10,
                    right: 5,
                    top: 4,
                    bottom: 10,
                }))
                .resizable(true)
                .default_width(264.0)
                .width_range(MIN_SIDEBAR..=MAX_SIDEBAR)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    let rect = ui.max_rect();
                    // Claim the whole panel: the contents are drawn into a
                    // child Ui, which would otherwise let the resizable panel
                    // collapse to its minimum width on the next frame.
                    ui.expand_to_include_rect(rect);
                    theme::frost(ui.painter(), rect, 16, theme::GLASS_PANEL, theme::ACCENT);
                    let content = rect.shrink(14.0);
                    let mut inner = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(content)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );
                    inner.set_clip_rect(content);
                    inner.set_max_width(content.width());
                    let sessions = &self.sessions;
                    let running = |conn: &Connection| -> Option<(u64, bool)> {
                        sessions
                            .iter()
                            .find(|s| s.connection().map(|c| c.dir == conn.dir).unwrap_or(false))
                            .map(|s| (s.id, s.is_alive()))
                    };
                    browser_action = sidebar_left::show(
                        &mut inner,
                        &self.connections,
                        &running,
                        &mut self.filter,
                        &self.suggestions,
                    );
                });
        }

        // ---- Right sidebar ------------------------------------------------
        let mut info_action = None;
        if self.right_open {
            egui::SidePanel::right("sscl-right")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                    left: 5,
                    right: 10,
                    top: 4,
                    bottom: 10,
                }))
                .resizable(true)
                .default_width(300.0)
                .width_range(MIN_SIDEBAR..=MAX_SIDEBAR)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    let rect = ui.max_rect();
                    ui.expand_to_include_rect(rect);
                    theme::frost(ui.painter(), rect, 16, theme::GLASS_PANEL, theme::ACCENT_ALT);
                    let content = rect.shrink(14.0);
                    let mut inner = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(content)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );
                    inner.set_clip_rect(content);
                    inner.set_max_width(content.width());
                    info_action = sidebar_right::show(&mut inner, self.active_session());
                });
        }

        // ---- Centre page --------------------------------------------------
        let dialog_open = self.dialog.is_some();
        let mut tab_result = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 5,
                right: 5,
                top: 4,
                bottom: 10,
            }))
            .show(ctx, |ui| {
                if self.sessions.len() > 1 || self.sessions.iter().any(|s| s.kind.is_remote()) {
                    let tabs: Vec<(u64, String, bool, bool)> = self
                        .sessions
                        .iter()
                        .map(|s| (s.id, s.title.clone(), s.is_alive(), s.kind.is_remote()))
                        .collect();
                    tab_result = Some(chrome::tab_strip(ui, &tabs, self.active));
                    ui.add_space(6.0);
                }

                let id = self.active;
                let font_size = self.font_size;
                if let Some(id) = id {
                    let state = self.term_states.entry(id).or_default();
                    if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                        term::show(ui, session, &mut self.suggestions, state, font_size, !dialog_open);
                    }
                }
            });

        // ---- Act on what the panels reported ------------------------------
        if let Some(result) = tab_result {
            if let Some(id) = result.activate {
                self.active = Some(id);
            }
            if let Some(id) = result.close {
                self.close_session(ctx, id);
            }
        }

        match browser_action {
            Some(BrowserAction::Start(i)) => self.start_connection(ctx, i),
            Some(BrowserAction::Edit(i)) => {
                if let Some(conn) = self.connections.get(i) {
                    self.dialog = Some(ConnectionDialog::edit(conn));
                }
            }
            Some(BrowserAction::Focus(id)) => self.active = Some(id),
            Some(BrowserAction::New) => {
                if self.dialog.is_none() {
                    self.dialog = Some(ConnectionDialog::new());
                }
            }
            None => {}
        }

        match info_action {
            Some(InfoAction::Rescan) => {
                if let Some(session) = self.active_session()
                    && let Some(conn) = session.connection() {
                        let (host, port) = (conn.host.clone(), conn.port);
                        let handle = session.probe.clone();
                        if let Ok(mut state) = handle.state.lock() {
                            *state = crate::sshinfo::ProbeState::Idle;
                        }
                        crate::sshinfo::spawn(handle, host, port);
                    }
            }
            Some(InfoAction::Copy(text)) => {
                ctx.copy_text(text);
                self.notify(ctx, "Copied to clipboard.", theme::ACCENT_ALT);
            }
            None => {}
        }

        self.dialog_ui(ctx);
    }

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        match dialog.show(ctx) {
            DialogOutcome::Pending => {}
            DialogOutcome::Closed => self.dialog = None,
            DialogOutcome::Create(request) => match store::create(&request) {
                Ok(conn) => {
                    let name = conn.name.clone();
                    self.dialog = None;
                    self.reload_connections();
                    self.notify(ctx, format!("Saved \"{name}\"."), theme::ACCENT_ALT);
                }
                Err(e) => dialog.error = Some(e),
            },
            DialogOutcome::Update {
                original,
                request,
                new_key,
            } => match store::update(&original, &request, new_key.as_deref()) {
                Ok(conn) => {
                    let name = conn.name.clone();
                    self.dialog = None;
                    self.reload_connections();
                    self.notify(ctx, format!("Updated \"{name}\"."), theme::ACCENT_ALT);
                }
                Err(e) => dialog.error = Some(e),
            },
            DialogOutcome::Delete(conn) => match store::delete(&conn) {
                Ok(()) => {
                    let name = conn.name.clone();
                    self.dialog = None;
                    self.reload_connections();
                    self.notify(ctx, format!("Deleted \"{name}\"."), theme::WARN);
                }
                Err(e) => dialog.error = Some(e),
            },
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else { return };
        let now = ctx.input(|i| i.time);
        let age = now - toast.born;
        if age > 3.6 {
            self.toast = None;
            return;
        }
        let alpha = if age > 3.0 {
            (1.0 - (age - 3.0) / 0.6) as f32
        } else {
            1.0
        };
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("sscl-toast"))
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::Pos2::new(screen.center().x, screen.max.y - 74.0))
            .pivot(egui::Align2::CENTER_CENTER)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::glass(230).gamma_multiply(alpha))
                    .stroke(egui::Stroke::new(
                        1.0,
                        toast.color.gamma_multiply(0.5 * alpha),
                    ))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(16, 9))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&toast.text)
                                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                .color(toast.color.gamma_multiply(alpha)),
                        );
                    });
            });
        ctx.request_repaint();
    }
}
