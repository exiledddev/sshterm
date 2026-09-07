//! Application state and layout.
//!
//! ACLI is a grid of terminal panes on one sheet of glass. There is no splash
//! screen and nothing to load, so the first frame draws a live shell.

use crate::layout::{Dir, Nav, Node};
use crate::platform::kwin_blur::Blur;
use crate::term::{self, TerminalState, pty::PtySession, suggest::Suggestions};
use crate::theme;
use crate::ui::chrome::{self, RibbonContext};
use crate::ui::settings::{SettingsOutcome, SettingsWindow};
use eframe::egui::{self, Color32};
use raw_window_handle::HasWindowHandle;
use std::collections::HashMap;

/// Ids for splits, taken from the same counter as session ids so the two can
/// never collide inside the layout tree.
static NEXT_SPLIT_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1 << 32);

fn next_split_id() -> u64 {
    NEXT_SPLIT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub struct AcliApp {
    sessions: HashMap<u64, PtySession>,
    layout: Option<Node>,
    active: Option<u64>,
    term_states: HashMap<u64, TerminalState>,
    suggestions: Suggestions,

    settings: Option<SettingsWindow>,
    font_size: f32,
    toast: Option<Toast>,

    /// Divider currently being dragged, and the rect it lives in.
    dragging: Option<u64>,
    /// Pane rectangles from the last frame, for focus navigation.
    pane_rects: Vec<(u64, egui::Rect)>,
    content_rect: egui::Rect,

    blur: Option<Blur>,
    blur_ready: bool,
    /// Set once a shell has run, so a start-up failure does not close the app.
    had_session: bool,
    startup_error: Option<String>,
}

struct Toast {
    text: String,
    color: Color32,
    born: f64,
}

impl AcliApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            sessions: HashMap::new(),
            layout: None,
            active: None,
            term_states: HashMap::new(),
            suggestions: Suggestions::new(),
            settings: None,
            font_size: 13.5,
            toast: None,
            dragging: None,
            pane_rects: Vec::new(),
            content_rect: egui::Rect::NOTHING,
            blur: None,
            blur_ready: false,
            had_session: false,
            startup_error: None,
        }
    }

    // -- panes ------------------------------------------------------------

    fn spawn_session(&mut self, ctx: &egui::Context) -> Option<u64> {
        match PtySession::local(ctx, 100, 30) {
            Ok(session) => {
                let id = session.id;
                self.sessions.insert(id, session);
                self.had_session = true;
                self.startup_error = None;
                Some(id)
            }
            Err(e) => {
                self.startup_error = Some(e.clone());
                self.notify(ctx, e, theme::DANGER);
                None
            }
        }
    }

    fn ensure_first_pane(&mut self, ctx: &egui::Context) {
        if self.layout.is_some() || self.startup_error.is_some() {
            return;
        }
        if let Some(id) = self.spawn_session(ctx) {
            self.layout = Some(Node::leaf(id));
            self.active = Some(id);
        }
    }

    fn split(&mut self, ctx: &egui::Context, dir: Dir) {
        let Some(target) = self.active else { return };
        let Some(new_id) = self.spawn_session(ctx) else {
            return;
        };
        let split_id = next_split_id();
        let placed = self
            .layout
            .as_mut()
            .is_some_and(|tree| tree.split(target, dir, split_id, new_id));
        if placed {
            self.active = Some(new_id);
        } else {
            // Nothing to attach it to; do not leak the shell we just started.
            self.close_session(new_id);
        }
    }

    /// Removes a pane and its shell. Returns true when the pane went away.
    fn close_pane(&mut self, id: u64) -> bool {
        let removed = self
            .layout
            .as_mut()
            .is_some_and(|tree| tree.remove(id));
        if !removed {
            return false;
        }
        self.close_session(id);
        if self.active == Some(id) {
            self.active = self.layout.as_ref().and_then(|t| t.leaves().first().copied());
        }
        true
    }

    fn close_session(&mut self, id: u64) {
        if let Some(mut session) = self.sessions.remove(&id) {
            session.terminate();
        }
        self.term_states.remove(&id);
    }

    /// Reaps panes whose shell has exited. When the last one goes, so does
    /// the window — the same as any other terminal.
    fn reap_dead_panes(&mut self, ctx: &egui::Context) {
        let dead: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, s)| !s.is_alive())
            .map(|(id, _)| *id)
            .collect();
        if dead.is_empty() {
            return;
        }
        for id in dead {
            if !self.close_pane(id) {
                // The last pane: its shell exited, so the app is done.
                self.close_session(id);
                self.layout = None;
                self.active = None;
                if self.had_session {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn active_session(&self) -> Option<&PtySession> {
        self.sessions.get(&self.active?)
    }

    fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>, color: Color32) {
        self.toast = Some(Toast {
            text: text.into(),
            color,
            born: ctx.input(|i| i.time),
        });
    }

    /// Re-sources the generated prompt in every pane that is sitting at an
    /// empty prompt, so a colour change shows up without opening a new pane.
    fn reload_prompts(&mut self, ctx: &egui::Context) {
        let Some(command) = crate::term::pty::reload_prompt_command() else {
            self.notify(
                ctx,
                "This shell has no ACLI prompt to reload.",
                theme::WARN,
            );
            return;
        };
        let mut applied = 0usize;
        let mut skipped = 0usize;
        for session in self.sessions.values_mut() {
            if session.line.at_fresh_prompt() {
                session.write(command.as_bytes());
                applied += 1;
            } else {
                skipped += 1;
            }
        }
        let text = if skipped == 0 {
            format!("Reloaded the prompt in {applied} pane(s).")
        } else {
            format!("Reloaded {applied}; left {skipped} busy pane(s) alone.")
        };
        self.notify(ctx, text, theme::ACCENT_ALT);
    }
}

impl eframe::App for AcliApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.sync_blur(ctx, frame);
        self.ensure_first_pane(ctx);
        self.shortcuts(ctx);
        self.main_ui(ctx);
        self.settings_ui(ctx);
        self.toast_ui(ctx);
        self.reap_dead_panes(ctx);
    }
}

impl AcliApp {
    /// Asks KWin to blur behind the window, and keeps the blurred region
    /// matching it as it is resized. See `platform::kwin_blur`.
    fn sync_blur(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.blur_ready {
            self.blur_ready = true;
            self.blur = frame
                .window_handle()
                .ok()
                .and_then(|h| Blur::new(&h.as_raw()));

            let forced = std::env::var("ACLI_BLUR").unwrap_or_default().to_lowercase();
            let blurred = match forced.as_str() {
                "on" | "1" | "true" => true,
                "off" | "0" | "false" => false,
                _ => self.blur.as_ref().is_some_and(Blur::announced),
            };
            theme::set_surface(if blurred {
                theme::Surface::Blurred
            } else {
                theme::Surface::Solid
            });

            if let Some(pct) = std::env::var("ACLI_OPACITY")
                .ok()
                .and_then(|v| v.trim().parse::<u8>().ok())
                .filter(|p| (1..=100).contains(p))
            {
                theme::set_opacity_percent(pct);
            }
        }

        if let Some(blur) = self.blur.as_mut() {
            let size = ctx.screen_rect().size() * ctx.pixels_per_point();
            blur.apply(
                size.x.round().max(0.0) as u32,
                size.y.round().max(0.0) as u32,
                theme::WINDOW_RADIUS,
            );
        }
    }

    /// Global shortcuts, consumed here so the terminal never sees them.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let cs = Modifiers::CTRL | Modifiers::SHIFT;
        let hit = |ctx: &egui::Context, mods: Modifiers, key: Key| -> bool {
            ctx.input_mut(|i| i.consume_key(mods, key))
        };

        if hit(ctx, cs, Key::R) {
            self.split(ctx, Dir::Row);
        }
        if hit(ctx, cs, Key::D) {
            self.split(ctx, Dir::Column);
        }
        if hit(ctx, cs, Key::W) {
            if let Some(id) = self.active {
                if !self.close_pane(id) {
                    self.notify(ctx, "That is the only pane.", theme::WARN);
                }
            }
        }
        if hit(ctx, cs, Key::Comma) && self.settings.is_none() {
            self.settings = Some(SettingsWindow::new());
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

        // Focus movement between panes.
        for (key, nav) in [
            (Key::ArrowLeft, Nav::Left),
            (Key::ArrowRight, Nav::Right),
            (Key::ArrowUp, Nav::Up),
            (Key::ArrowDown, Nav::Down),
        ] {
            if hit(ctx, cs, key) {
                self.move_focus(ctx, nav);
            }
        }
    }

    fn move_focus(&mut self, ctx: &egui::Context, nav: Nav) {
        let (Some(tree), Some(from)) = (self.layout.as_ref(), self.active) else {
            return;
        };
        if let Some(to) = tree.neighbour(from, nav, self.content_rect, theme::GAP) {
            self.active = Some(to);
            // Hand egui's keyboard focus over as well.
            ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(egui::Id::NULL)));
        }
    }

    fn main_ui(&mut self, ctx: &egui::Context) {
        theme::window_backdrop(&ctx.layer_painter(egui::LayerId::background()), ctx.screen_rect());
        chrome::resize_handles(ctx);

        let panes = self.layout.as_ref().map(Node::pane_count).unwrap_or(0);
        let (shell, grid) = match self.active_session() {
            Some(s) => (s.shell.clone(), Some((s.cols, s.rows))),
            None => (String::new(), None),
        };
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

        let actions = chrome::top_bar(
            ctx,
            &RibbonContext {
                shell: &shell,
                panes,
                grid,
                settings_open: self.settings.is_some(),
                maximized,
            },
        );
        if actions.split_right {
            self.split(ctx, Dir::Row);
        }
        if actions.split_down {
            self.split(ctx, Dir::Column);
        }
        if actions.close_pane {
            if let Some(id) = self.active {
                if !self.close_pane(id) {
                    self.notify(ctx, "That is the only pane.", theme::WARN);
                }
            }
        }
        if actions.settings && self.settings.is_none() {
            self.settings = Some(SettingsWindow::new());
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: theme::GAP as i8,
                right: theme::GAP as i8,
                top: theme::GAP as i8,
                bottom: theme::GAP as i8,
            }))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.expand_to_include_rect(rect);
                self.content_rect = rect;

                let Some(tree) = self.layout.clone() else {
                    self.no_panes_ui(ui, rect);
                    return;
                };

                self.pane_rects = tree.pane_rects(rect, theme::GAP);
                self.dividers_ui(ui, &tree, rect);

                let (font_size, active) = (self.font_size, self.active);
                let mut focus_changed = None;
                for (id, pane_rect) in self.pane_rects.clone() {
                    let state = self.term_states.entry(id).or_default();
                    let Some(session) = self.sessions.get_mut(&id) else {
                        continue;
                    };
                    let mut pane_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(pane_rect)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );
                    pane_ui.set_clip_rect(pane_rect);
                    let status = term::show(
                        &mut pane_ui,
                        session,
                        &mut self.suggestions,
                        state,
                        font_size,
                        active == Some(id),
                    );
                    if status.focused && active != Some(id) {
                        focus_changed = Some(id);
                    }
                }
                if let Some(id) = focus_changed {
                    self.active = Some(id);
                }
            });
    }

    /// Draggable strips in the gaps between panes.
    fn dividers_ui(&mut self, ui: &mut egui::Ui, tree: &Node, rect: egui::Rect) {
        for (split_id, dir, strip) in tree.divider_rects(rect, theme::GAP) {
            // Widen the grab area a little beyond the visible gap.
            let grab = match dir {
                Dir::Row => strip.expand2(egui::vec2(2.0, 0.0)),
                Dir::Column => strip.expand2(egui::vec2(0.0, 2.0)),
            };
            let response = ui.interact(
                grab,
                ui.id().with(("acli-divider", split_id)),
                egui::Sense::drag(),
            );
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(match dir {
                    Dir::Row => egui::CursorIcon::ResizeHorizontal,
                    Dir::Column => egui::CursorIcon::ResizeVertical,
                });
            }
            if response.drag_started() {
                self.dragging = Some(split_id);
            }
            if response.dragged() && self.dragging == Some(split_id) {
                if let Some(pos) = response.interact_pointer_pos() {
                    // Work the ratio out against the branch's own area, which
                    // is the union of its two children plus the gap.
                    let ratio = match dir {
                        Dir::Row => (pos.x - rect.min.x) / (rect.width() - theme::GAP).max(1.0),
                        Dir::Column => (pos.y - rect.min.y) / (rect.height() - theme::GAP).max(1.0),
                    };
                    if let Some(tree) = self.layout.as_mut() {
                        tree.set_ratio(split_id, ratio);
                    }
                }
            }
            if response.drag_stopped() {
                self.dragging = None;
            }

            // A faint grip so the divider is discoverable.
            if response.hovered() || response.dragged() {
                let mid = strip.center();
                let along = match dir {
                    Dir::Row => egui::vec2(0.0, 10.0),
                    Dir::Column => egui::vec2(10.0, 0.0),
                };
                ui.painter().line_segment(
                    [mid - along, mid + along],
                    egui::Stroke::new(2.0, theme::ACCENT_ALT.gamma_multiply(0.8)),
                );
            }
        }
    }

    fn no_panes_ui(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(theme::ISLAND_RADIUS),
            theme::island_fill(),
        );
        let text = self
            .startup_error
            .clone()
            .unwrap_or_else(|| "No shell is running.".to_string());
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::new(12.5, egui::FontFamily::Proportional),
            theme::DANGER,
        );
    }

    fn settings_ui(&mut self, ctx: &egui::Context) {
        let Some(window) = self.settings.as_mut() else {
            return;
        };
        match window.show(ctx) {
            SettingsOutcome::Pending => {}
            SettingsOutcome::Closed => self.settings = None,
            SettingsOutcome::ApplyToOpenPanes => self.reload_prompts(ctx),
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
        egui::Area::new(egui::Id::new("acli-toast"))
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::Pos2::new(screen.center().x, screen.max.y - 56.0))
            .pivot(egui::Align2::CENTER_CENTER)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::glass(theme::floating_alpha()).gamma_multiply(alpha))
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
