//! The centre page: a real terminal emulator drawn with egui.

pub mod pty;
pub mod suggest;

use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, EventFilter, FontFamily, FontId, Key, Modifiers, Pos2,
    Rect, Sense, Stroke, Ui, Vec2,
};
use pty::PtySession;
use suggest::{Candidate, Suggestions};
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

/// Padding between the terminal sheet and the character grid.
const PAD: f32 = 12.0;
/// Extra leading between rows, in pixels.
const LINE_GAP: f32 = 1.0;
/// How many suggestions the popup shows at most.
const MAX_CANDIDATES: usize = 6;

/// Per-session view state (selection, suggestion popup).
#[derive(Default)]
pub struct TerminalState {
    anchor: Option<(u16, u16)>,
    head: Option<(u16, u16)>,
    dragging: bool,
    suggestion: Option<String>,
    candidates: Vec<Candidate>,
    popup_dismissed: bool,
    /// Set when the user copies, so the UI can flash a hint.
    pub last_copy: Option<f64>,
}

impl TerminalState {
    fn clear_selection(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    fn ordered_selection(&self) -> Option<((u16, u16), (u16, u16))> {
        let (a, b) = (self.anchor?, self.head?);
        if a == b {
            return None;
        }
        Some(if (a.0, a.1) <= (b.0, b.1) { (a, b) } else { (b, a) })
    }
}

/// Font metrics of the terminal grid.
struct Metrics {
    cell_w: f32,
    cell_h: f32,
    font: FontId,
}

fn metrics(ctx: &egui::Context, size: f32) -> Metrics {
    let font = FontId::new(size, FontFamily::Monospace);
    let (cell_w, row_h) = ctx.fonts(|f| {
        let w = f
            .layout_no_wrap("M".to_string(), font.clone(), Color32::WHITE)
            .rect
            .width();
        (w, f.row_height(&font))
    });
    Metrics {
        cell_w: cell_w.max(1.0),
        cell_h: (row_h + LINE_GAP).max(1.0),
        font,
    }
}

/// What the status bar should say about this terminal.
#[derive(Default)]
pub struct TerminalStatus {
    pub cols: u16,
    pub rows: u16,
    pub scrollback: usize,
    pub exit_note: Option<String>,
    pub focused: bool,
    pub copied: bool,
}

/// Draws the terminal page and handles all of its input.
pub fn show(
    ui: &mut Ui,
    session: &mut PtySession,
    suggestions: &mut Suggestions,
    state: &mut TerminalState,
    font_size: f32,
    enabled: bool,
) -> TerminalStatus {
    let rect = ui.max_rect();
    let id = ui.make_persistent_id(("sscl-terminal", session.id));
    let response = ui.interact(rect, id, Sense::click_and_drag());

    // The terminal must receive Tab, arrows and Escape itself.
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        );
    });
    if enabled && (response.clicked() || response.drag_started()) {
        response.request_focus();
    }
    let focused = enabled && response.has_focus();

    // The opaque dark-grey island the blueprint requires, inset from the
    // surrounding glass and outlined, like an editor pane.
    let painter = ui.painter_at(rect);
    let island = CornerRadius::same(theme::ISLAND_RADIUS);
    painter.rect_filled(rect, island, theme::island_fill());
    painter.rect_stroke(rect, island, theme::island_stroke(), egui::StrokeKind::Inside);

    let m = metrics(ui.ctx(), font_size);
    let grid_origin = rect.min + Vec2::splat(PAD);
    let cols = (((rect.width() - PAD * 2.0) / m.cell_w).floor() as i64).clamp(8, 1000) as u16;
    let rows = (((rect.height() - PAD * 2.0) / m.cell_h).floor() as i64).clamp(2, 500) as u16;
    session.resize(cols, rows);

    // ---- 1. Suggestions, computed from last frame's mirrored line --------
    refresh_suggestions(session, suggestions, state);

    // ---- 2. Draw the screen ---------------------------------------------
    let screen_info = {
        let Ok(parser) = session.parser.lock() else {
            return TerminalStatus::default();
        };
        let screen = parser.screen();
        draw_screen(&painter, screen, &m, grid_origin, state);
        ScreenInfo {
            cursor: screen.cursor_position(),
            hide_cursor: screen.hide_cursor(),
            app_cursor: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            mouse_mode: screen.mouse_protocol_mode(),
            mouse_encoding: screen.mouse_protocol_encoding(),
            alternate: screen.alternate_screen(),
            cursor_prefix: {
                let (r, c) = screen.cursor_position();
                screen.contents_between(r, 0, r, c)
            },
        }
    };

    draw_cursor(ui, &painter, &screen_info, &m, grid_origin, focused);

    // The ghost suggestion is only drawn when the visible line really does
    // end with the text we think the user typed.
    let ghost_ok = session.line.can_suggest()
        && screen_info
            .cursor_prefix
            .trim_end()
            .ends_with(session.line.line().trim_end())
        && !session.line.line().trim().is_empty();

    if ghost_ok {
        if let Some(full) = state.suggestion.clone() {
            draw_ghost(&painter, &screen_info, &m, grid_origin, &session.line.line(), &full, cols);
        }
        if !state.popup_dismissed && state.candidates.len() > 1 {
            draw_candidates(ui, &painter, &screen_info, &m, grid_origin, state, rect);
        }
    }

    // ---- 3. Input, applied after drawing so the mirror stays in step -----
    if focused {
        handle_keyboard(ui, session, suggestions, state, &screen_info);
    }
    if enabled {
        handle_mouse(ui, &response, session, state, &m, grid_origin, &screen_info, cols, rows);
    }

    if focused && !screen_info.hide_cursor {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }

    TerminalStatus {
        cols,
        rows,
        scrollback: session.scroll,
        exit_note: if session.is_alive() {
            None
        } else {
            Some(
                session
                    .exit_note()
                    .unwrap_or_else(|| "Session ended.".to_string()),
            )
        },
        focused,
        copied: state
            .last_copy
            .is_some_and(|t| ui.input(|i| i.time) - t < 1.6),
    }
}

struct ScreenInfo {
    cursor: (u16, u16),
    hide_cursor: bool,
    app_cursor: bool,
    bracketed_paste: bool,
    mouse_mode: MouseProtocolMode,
    mouse_encoding: MouseProtocolEncoding,
    alternate: bool,
    cursor_prefix: String,
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw_screen(
    painter: &egui::Painter,
    screen: &vt100::Screen,
    m: &Metrics,
    origin: Pos2,
    state: &TerminalState,
) {
    let (rows, cols) = screen.size();
    let selection = state.ordered_selection();

    for row in 0..rows {
        let y = origin.y + row as f32 * m.cell_h;

        // Background runs first, so text is never clipped by a later fill.
        let mut col = 0u16;
        while col < cols {
            let Some(cell) = screen.cell(row, col) else {
                col += 1;
                continue;
            };
            let bg = resolve_bg(cell);
            let selected = in_selection(selection, row, col);
            let mut run_end = col + 1;
            while run_end < cols {
                let Some(next) = screen.cell(row, run_end) else {
                    break;
                };
                if resolve_bg(next) != bg || in_selection(selection, row, run_end) != selected {
                    break;
                }
                run_end += 1;
            }
            let fill = if selected {
                theme::ACCENT.gamma_multiply(0.45)
            } else {
                bg
            };
            if fill != theme::TERMINAL_BG || selected {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(origin.x + col as f32 * m.cell_w, y),
                        Vec2::new((run_end - col) as f32 * m.cell_w, m.cell_h),
                    ),
                    CornerRadius::ZERO,
                    fill,
                );
            }
            col = run_end;
        }

        // Then the glyphs, batched into runs sharing one colour.
        let mut col = 0u16;
        let mut run = String::new();
        let mut run_start = 0u16;
        let mut run_color = Color32::TRANSPARENT;
        let mut run_underline = false;

        let flush = |run: &mut String, start: u16, color: Color32, underline: bool| {
            if run.is_empty() {
                return;
            }
            let pos = Pos2::new(origin.x + start as f32 * m.cell_w, y);
            painter.text(pos, Align2::LEFT_TOP, run.as_str(), m.font.clone(), color);
            if underline {
                let uy = y + m.cell_h - 1.5;
                painter.line_segment(
                    [
                        Pos2::new(pos.x, uy),
                        Pos2::new(pos.x + run.chars().count() as f32 * m.cell_w, uy),
                    ],
                    Stroke::new(1.0, color),
                );
            }
            run.clear();
        };

        while col < cols {
            let Some(cell) = screen.cell(row, col) else {
                flush(&mut run, run_start, run_color, run_underline);
                col += 1;
                continue;
            };
            if cell.is_wide_continuation() {
                col += 1;
                continue;
            }
            let text = cell.contents();
            let fg = resolve_fg(cell);
            let underline = cell.underline();

            if text.is_empty() || text == " " {
                flush(&mut run, run_start, run_color, run_underline);
                col += 1;
                continue;
            }
            if cell.is_wide() {
                flush(&mut run, run_start, run_color, run_underline);
                painter.text(
                    Pos2::new(origin.x + col as f32 * m.cell_w, y),
                    Align2::LEFT_TOP,
                    text,
                    m.font.clone(),
                    fg,
                );
                col += 2;
                continue;
            }
            if run.is_empty() {
                run_start = col;
                run_color = fg;
                run_underline = underline;
            } else if fg != run_color || underline != run_underline {
                flush(&mut run, run_start, run_color, run_underline);
                run_start = col;
                run_color = fg;
                run_underline = underline;
            }
            run.push_str(text);
            col += 1;
        }
        flush(&mut run, run_start, run_color, run_underline);
    }
}

fn in_selection(sel: Option<((u16, u16), (u16, u16))>, row: u16, col: u16) -> bool {
    let Some((a, b)) = sel else { return false };
    (row, col) >= a && (row, col) < b
}

fn draw_cursor(
    ui: &Ui,
    painter: &egui::Painter,
    info: &ScreenInfo,
    m: &Metrics,
    origin: Pos2,
    focused: bool,
) {
    if info.hide_cursor {
        return;
    }
    let (row, col) = info.cursor;
    let cell = Rect::from_min_size(
        Pos2::new(origin.x + col as f32 * m.cell_w, origin.y + row as f32 * m.cell_h),
        Vec2::new(m.cell_w, m.cell_h),
    );
    if !focused {
        painter.rect_stroke(
            cell,
            CornerRadius::same(2),
            Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
        return;
    }
    let t = ui.input(|i| i.time);
    let phase = (t * 1.6).sin() as f32;
    let alpha = 0.55 + 0.45 * phase.max(0.0);
    painter.rect_filled(
        cell,
        CornerRadius::same(2),
        theme::ACCENT_ALT.gamma_multiply(alpha),
    );
}

/// Draws the greyed-out remainder of the best suggestion, fish style.
fn draw_ghost(
    painter: &egui::Painter,
    info: &ScreenInfo,
    m: &Metrics,
    origin: Pos2,
    typed: &str,
    full: &str,
    cols: u16,
) {
    let Some(rest) = full.strip_prefix(typed) else {
        return;
    };
    if rest.is_empty() {
        return;
    }
    let (row, col) = info.cursor;
    let room = cols.saturating_sub(col) as usize;
    if room == 0 {
        return;
    }
    let shown: String = rest.chars().take(room).collect();
    painter.text(
        Pos2::new(origin.x + col as f32 * m.cell_w, origin.y + row as f32 * m.cell_h),
        Align2::LEFT_TOP,
        shown,
        m.font.clone(),
        Color32::from_rgb(0x5A, 0x5D, 0x70),
    );
}

/// Floating list of alternative completions.
fn draw_candidates(
    ui: &mut Ui,
    painter: &egui::Painter,
    info: &ScreenInfo,
    m: &Metrics,
    origin: Pos2,
    state: &mut TerminalState,
    bounds: Rect,
) {
    let items: Vec<Candidate> = state.candidates.iter().take(MAX_CANDIDATES).cloned().collect();
    if items.is_empty() {
        return;
    }
    let font = FontId::new(12.0, FontFamily::Monospace);
    let tag_font = FontId::new(10.0, FontFamily::Proportional);
    let row_h = 20.0;
    let width = items
        .iter()
        .map(|c| {
            ui.ctx().fonts(|f| {
                f.layout_no_wrap(c.text.clone(), font.clone(), Color32::WHITE)
                    .rect
                    .width()
            })
        })
        .fold(180.0_f32, f32::max)
        + 96.0;

    let (crow, ccol) = info.cursor;
    let mut top_left = Pos2::new(
        origin.x + ccol as f32 * m.cell_w,
        origin.y + (crow + 1) as f32 * m.cell_h + 4.0,
    );
    let height = items.len() as f32 * row_h + 10.0;
    if top_left.y + height > bounds.max.y - 6.0 {
        top_left.y = origin.y + crow as f32 * m.cell_h - height - 4.0;
    }
    top_left.x = top_left.x.min(bounds.max.x - width - 8.0).max(bounds.min.x + 4.0);
    top_left.y = top_left.y.max(bounds.min.y + 4.0);

    let panel = Rect::from_min_size(top_left, Vec2::new(width, height));
    painter.rect_filled(
        panel.translate(Vec2::new(0.0, 3.0)),
        CornerRadius::same(10),
        Color32::from_black_alpha(90),
    );
    theme::frost(painter, panel, 10, theme::floating_alpha());

    for (i, cand) in items.iter().enumerate() {
        let row_rect = Rect::from_min_size(
            Pos2::new(panel.min.x + 5.0, panel.min.y + 5.0 + i as f32 * row_h),
            Vec2::new(panel.width() - 10.0, row_h),
        );
        let hovered = ui.rect_contains_pointer(row_rect);
        let selected = i == 0;
        if selected || hovered {
            painter.rect_filled(
                row_rect,
                CornerRadius::same(6),
                theme::ACCENT.gamma_multiply(if selected { 0.34 } else { 0.18 }),
            );
        }
        painter.text(
            Pos2::new(row_rect.min.x + 8.0, row_rect.center().y),
            Align2::LEFT_CENTER,
            &cand.text,
            font.clone(),
            if selected { theme::TEXT } else { theme::TEXT_DIM },
        );
        painter.text(
            Pos2::new(row_rect.max.x - 8.0, row_rect.center().y),
            Align2::RIGHT_CENTER,
            cand.source.tag(),
            tag_font.clone(),
            theme::TEXT_FAINT,
        );
    }
}

// ---------------------------------------------------------------------------
// Suggestions
// ---------------------------------------------------------------------------

fn refresh_suggestions(
    session: &mut PtySession,
    suggestions: &Suggestions,
    state: &mut TerminalState,
) {
    if !session.line.can_suggest() {
        state.suggestion = None;
        state.candidates.clear();
        return;
    }
    let prefix = session.line.prefix();
    let candidates = suggestions.candidates(&prefix, MAX_CANDIDATES);
    state.suggestion = candidates.first().map(|c| c.text.clone());
    state.candidates = candidates;
}

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

fn handle_keyboard(
    ui: &mut Ui,
    session: &mut PtySession,
    suggestions: &mut Suggestions,
    state: &mut TerminalState,
    info: &ScreenInfo,
) {
    let events = ui.input(|i| i.events.clone());
    let mut out: Vec<u8> = Vec::new();

    for event in events {
        match event {
            egui::Event::Text(text) => {
                // Skip text that arrives alongside a modifier we translate
                // ourselves; AltGr (ctrl+alt together) still gets through.
                let mods = ui.input(|i| i.modifiers);
                if mods.ctrl != mods.alt && (mods.ctrl || mods.alt) {
                    continue;
                }
                out.extend_from_slice(text.as_bytes());
                session.line.insert_str(&text);
                state.popup_dismissed = false;
            }
            // egui-winit turns ctrl+C/X/V into clipboard events and never
            // emits the key press, so the terminal bindings are recovered
            // here: plain ctrl+C/X/V are control codes, and the shifted
            // forms are the clipboard, exactly as in Konsole or xterm.
            egui::Event::Paste(text) => {
                if ui.input(|i| i.modifiers.shift) {
                    let cleaned: String = text.replace('\r', "\n");
                    if info.bracketed_paste {
                        out.extend_from_slice(b"\x1b[200~");
                        out.extend_from_slice(cleaned.as_bytes());
                        out.extend_from_slice(b"\x1b[201~");
                    } else {
                        out.extend_from_slice(cleaned.as_bytes());
                    }
                    session.line.desync();
                } else {
                    // ctrl+V is "quoted insert" in a terminal, not paste.
                    out.push(0x16);
                    session.line.desync();
                }
            }
            egui::Event::Copy => {
                if ui.input(|i| i.modifiers.shift) {
                    copy_selection(ui, session, state);
                } else {
                    out.push(0x03); // SIGINT
                    session.line.reset();
                }
            }
            egui::Event::Cut => {
                if ui.input(|i| i.modifiers.shift) {
                    copy_selection(ui, session, state);
                } else {
                    out.push(0x18); // ctrl+X, used by nano and friends
                    session.line.desync();
                }
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if handle_shortcut(session, state, key, modifiers, &mut out) {
                    continue;
                }
                if let Some(bytes) = key_to_bytes(key, modifiers, info) {
                    track_key(session, key, modifiers, suggestions);
                    out.extend_from_slice(&bytes);
                }
            }
            _ => {}
        }
    }

    if !out.is_empty() {
        session.write(&out);
    }
}

/// Application-level shortcuts. Returns true when the key was consumed.
fn handle_shortcut(
    session: &mut PtySession,
    state: &mut TerminalState,
    key: Key,
    mods: Modifiers,
    out: &mut Vec<u8>,
) -> bool {
    // Note that ctrl+C/X/V never arrive here: egui-winit turns them into
    // clipboard events, which `handle_keyboard` deals with directly.
    let accepting = session.line.can_suggest() && state.suggestion.is_some();

    // Accept the inline suggestion: Right arrow at end of line, or Ctrl+F.
    if accepting
        && ((key == Key::ArrowRight && mods.is_none())
            || (key == Key::F && mods.ctrl && !mods.shift)
            || (key == Key::End && mods.is_none()))
        && let Some(full) = state.suggestion.clone() {
            let typed = session.line.line();
            if let Some(rest) = full.strip_prefix(typed.as_str()) {
                out.extend_from_slice(rest.as_bytes());
                session.line.insert_str(rest);
                return true;
            }
        }

    // Cycle the popup without touching the shell's own history keys.
    if accepting && state.candidates.len() > 1 && mods.alt && !mods.ctrl {
        match key {
            Key::ArrowDown => {
                state.candidates.rotate_left(1);
                state.suggestion = state.candidates.first().map(|c| c.text.clone());
                state.popup_dismissed = false;
                return true;
            }
            Key::ArrowUp => {
                state.candidates.rotate_right(1);
                state.suggestion = state.candidates.first().map(|c| c.text.clone());
                state.popup_dismissed = false;
                return true;
            }
            _ => {}
        }
    }

    // Escape dismisses the popup first, and only then reaches the shell.
    if key == Key::Escape && mods.is_none() && !state.popup_dismissed && !state.candidates.is_empty()
    {
        state.popup_dismissed = true;
        return true;
    }

    // Shift+PageUp / Shift+PageDown scroll the scrollback.
    if mods.shift && !mods.ctrl {
        match key {
            Key::PageUp => {
                session.scroll_by(session.rows as isize / 2);
                return true;
            }
            Key::PageDown => {
                session.scroll_by(-(session.rows as isize / 2));
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Keeps the mirrored line in step with keys that edit it.
fn track_key(session: &mut PtySession, key: Key, mods: Modifiers, suggestions: &mut Suggestions) {
    let ctrl = mods.ctrl && !mods.alt;
    match key {
        Key::Enter => {
            if let Some(line) = session.line.submit() {
                suggestions.record(&line);
            }
        }
        Key::Backspace => session.line.backspace(),
        Key::Delete => session.line.delete_forward(),
        Key::ArrowLeft if mods.is_none() => session.line.left(),
        Key::ArrowRight if mods.is_none() => session.line.right(),
        Key::Home if mods.is_none() => session.line.home(),
        Key::End if mods.is_none() => session.line.end(),
        // The shell rewrites the line for these; stop guessing until Enter.
        Key::Tab | Key::ArrowUp | Key::ArrowDown => session.line.desync(),
        Key::U if ctrl => session.line.reset(),
        Key::W if ctrl => session.line.delete_word(),
        Key::K if ctrl => session.line.kill_to_end(),
        Key::A if ctrl => session.line.home(),
        Key::E if ctrl => session.line.end(),
        Key::R if ctrl => session.line.desync(),
        Key::L if ctrl => session.line.desync(),
        Key::D if ctrl => session.line.desync(),
        _ => {}
    }
}

fn copy_selection(ui: &mut Ui, session: &PtySession, state: &mut TerminalState) {
    let Some(((r1, c1), (r2, c2))) = state.ordered_selection() else {
        return;
    };
    let text = {
        let Ok(parser) = session.parser.lock() else {
            return;
        };
        // contents_between is end-exclusive on the final column.
        parser.screen().contents_between(r1, c1, r2, c2)
    };
    if !text.is_empty() {
        ui.ctx().copy_text(text);
        state.last_copy = Some(ui.input(|i| i.time));
    }
}

/// Translates a key press into the bytes a VT-compatible program expects.
fn key_to_bytes(key: Key, mods: Modifiers, info: &ScreenInfo) -> Option<Vec<u8>> {
    let ctrl = mods.ctrl;
    let alt = mods.alt;
    let shift = mods.shift;

    // xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl.
    let modifier = 1 + (shift as u8) + 2 * (alt as u8) + 4 * (ctrl as u8);

    let csi = |suffix: char| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{suffix}").into_bytes()
        } else if info.app_cursor {
            format!("\x1bO{suffix}").into_bytes()
        } else {
            format!("\x1b[{suffix}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[{n};{modifier}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };

    let bytes = match key {
        Key::Enter => {
            if alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        Key::Tab => {
            if shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        Key::Backspace => {
            if ctrl {
                b"\x08".to_vec()
            } else if alt {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            }
        }
        Key::Escape => b"\x1b".to_vec(),
        Key::ArrowUp => csi('A'),
        Key::ArrowDown => csi('B'),
        Key::ArrowRight => csi('C'),
        Key::ArrowLeft => csi('D'),
        Key::Home => csi('H'),
        Key::End => csi('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F1 => b"\x1bOP".to_vec(),
        Key::F2 => b"\x1bOQ".to_vec(),
        Key::F3 => b"\x1bOR".to_vec(),
        Key::F4 => b"\x1bOS".to_vec(),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        Key::Space if ctrl => b"\x00".to_vec(),
        Key::OpenBracket if ctrl => b"\x1b".to_vec(),
        Key::Backslash if ctrl => vec![0x1c],
        Key::CloseBracket if ctrl => vec![0x1d],
        Key::Minus if ctrl => vec![0x1f],
        _ => {
            // Ctrl+letter and Alt+letter.
            let letter = key_letter(key)?;
            if ctrl && !alt {
                vec![(letter as u8 - b'a') + 1]
            } else if alt && !ctrl {
                vec![0x1b, letter as u8]
            } else {
                return None;
            }
        }
    };
    Some(bytes)
}

fn key_letter(key: Key) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_none() && c.is_ascii_alphabetic() {
        Some(c.to_ascii_lowercase())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Mouse
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_mouse(
    ui: &mut Ui,
    response: &egui::Response,
    session: &mut PtySession,
    state: &mut TerminalState,
    m: &Metrics,
    origin: Pos2,
    info: &ScreenInfo,
    cols: u16,
    rows: u16,
) {
    let to_cell = |pos: Pos2| -> (u16, u16) {
        let col = (((pos.x - origin.x) / m.cell_w).floor() as i64).clamp(0, cols as i64 - 1) as u16;
        let row = (((pos.y - origin.y) / m.cell_h).floor() as i64).clamp(0, rows as i64 - 1) as u16;
        (row, col)
    };

    let mouse_reporting = info.mouse_mode != MouseProtocolMode::None;
    let shift_held = ui.input(|i| i.modifiers.shift);
    // Holding shift always gives local selection, as in xterm.
    let forward_to_app = mouse_reporting && !shift_held;

    // --- Wheel ---
    let scroll = ui.input(|i| i.raw_scroll_delta.y);
    if response.hovered() && scroll.abs() > 0.0 {
        if forward_to_app || (info.alternate && mouse_reporting) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                let (row, col) = to_cell(pos);
                let button = if scroll > 0.0 { 64 } else { 65 };
                let mut bytes = Vec::new();
                for _ in 0..3 {
                    bytes.extend_from_slice(&encode_mouse(
                        info.mouse_encoding,
                        button,
                        col,
                        row,
                        true,
                    ));
                }
                session.write(&bytes);
            }
        } else {
            let lines = (scroll / m.cell_h).round() as isize;
            if lines != 0 {
                session.scroll_by(lines);
            }
        }
    }

    // --- Buttons / selection ---
    if forward_to_app {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let (row, col) = to_cell(pos);
            let mut bytes = Vec::new();
            for (btn, egui_btn) in [
                (0u8, egui::PointerButton::Primary),
                (1, egui::PointerButton::Middle),
                (2, egui::PointerButton::Secondary),
            ] {
                if ui.input(|i| i.pointer.button_pressed(egui_btn)) {
                    bytes.extend_from_slice(&encode_mouse(
                        info.mouse_encoding,
                        btn,
                        col,
                        row,
                        true,
                    ));
                }
                if ui.input(|i| i.pointer.button_released(egui_btn))
                    && info.mouse_mode != MouseProtocolMode::Press
                {
                    bytes.extend_from_slice(&encode_mouse(
                        info.mouse_encoding,
                        btn,
                        col,
                        row,
                        false,
                    ));
                }
            }
            if !bytes.is_empty() {
                session.write(&bytes);
            }
        }
        return;
    }

    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos() {
            state.anchor = Some(to_cell(pos));
            state.head = Some(to_cell(pos));
            state.dragging = true;
        }
    if state.dragging && response.dragged()
        && let Some(pos) = response.interact_pointer_pos() {
            state.head = Some(to_cell(pos));
        }
    if response.drag_stopped() {
        state.dragging = false;
        // Middle-click paste convention: copy on select, like X11.
        copy_selection(ui, session, state);
    }
    if response.clicked() && !state.dragging {
        state.clear_selection();
    }
    // Middle-click pastes the primary selection, as in most Linux terminals.
    if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Middle)) && response.hovered()
        && let Some(((r1, c1), (r2, c2))) = state.ordered_selection() {
            let text = {
                match session.parser.lock() {
                    Ok(p) => p.screen().contents_between(r1, c1, r2, c2),
                    Err(_) => String::new(),
                }
            };
            if !text.is_empty() {
                session.write(text.as_bytes());
                session.line.desync();
            }
        }
}

/// Encodes a mouse event in the encoding the application asked for.
fn encode_mouse(
    encoding: MouseProtocolEncoding,
    button: u8,
    col: u16,
    row: u16,
    press: bool,
) -> Vec<u8> {
    match encoding {
        MouseProtocolEncoding::Sgr => format!(
            "\x1b[<{};{};{}{}",
            button,
            col + 1,
            row + 1,
            if press { 'M' } else { 'm' }
        )
        .into_bytes(),
        _ => {
            // X10/UTF-8 encoding: everything is offset by 32 and release is
            // reported as button 3.
            let b = if press { button } else { 3 };
            let cx = (col.min(222) + 1 + 32) as u8;
            let cy = (row.min(222) + 1 + 32) as u8;
            vec![0x1b, b'[', b'M', b + 32, cx, cy]
        }
    }
}

// ---------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------

/// Default foreground of the terminal sheet.
const FG_DEFAULT: Color32 = Color32::from_rgb(0xD2, 0xD5, 0xE2);

fn resolve_fg(cell: &vt100::Cell) -> Color32 {
    let mut fg = match cell.fgcolor() {
        vt100::Color::Default => FG_DEFAULT,
        vt100::Color::Idx(i) => ansi_color(if cell.bold() && i < 8 { i + 8 } else { i }),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    };
    let mut bg = match cell.bgcolor() {
        vt100::Color::Default => theme::TERMINAL_BG,
        vt100::Color::Idx(i) => ansi_color(i),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    };
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.dim() {
        fg = fg.gamma_multiply(0.6);
    }
    fg
}

fn resolve_bg(cell: &vt100::Cell) -> Color32 {
    let fg = match cell.fgcolor() {
        vt100::Color::Default => FG_DEFAULT,
        vt100::Color::Idx(i) => ansi_color(if cell.bold() && i < 8 { i + 8 } else { i }),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    };
    let bg = match cell.bgcolor() {
        vt100::Color::Default => theme::TERMINAL_BG,
        vt100::Color::Idx(i) => ansi_color(i),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    };
    if cell.inverse() { fg } else { bg }
}

/// The xterm 256-colour palette, with a modern set of base 16 colours that
/// matches the rest of the application.
fn ansi_color(idx: u8) -> Color32 {
    const BASE: [(u8, u8, u8); 16] = [
        (0x21, 0x22, 0x2A), // 0 black
        (0xFF, 0x5D, 0x73), // 1 red
        (0x5C, 0xE6, 0x8A), // 2 green
        (0xFF, 0xC4, 0x6B), // 3 yellow
        (0x6E, 0x8B, 0xFF), // 4 blue
        (0xC7, 0x7D, 0xFF), // 5 magenta
        (0x38, 0xE8, 0xC8), // 6 cyan
        (0xC7, 0xCA, 0xD9), // 7 white
        (0x56, 0x59, 0x6B), // 8 bright black
        (0xFF, 0x81, 0x94), // 9 bright red
        (0x86, 0xF0, 0xA9), // 10 bright green
        (0xFF, 0xD7, 0x9A), // 11 bright yellow
        (0x93, 0xA9, 0xFF), // 12 bright blue
        (0xDC, 0xA6, 0xFF), // 13 bright magenta
        (0x79, 0xF2, 0xDC), // 14 bright cyan
        (0xFF, 0xFF, 0xFF), // 15 bright white
    ];
    match idx {
        0..=15 => {
            let (r, g, b) = BASE[idx as usize];
            Color32::from_rgb(r, g, b)
        }
        16..=231 => {
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let i = idx as usize - 16;
            Color32::from_rgb(
                LEVELS[i / 36],
                LEVELS[(i / 6) % 6],
                LEVELS[i % 6],
            )
        }
        _ => {
            let v = 8u16 + 10 * (idx as u16 - 232);
            let v = v.min(255) as u8;
            Color32::from_rgb(v, v, v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(app_cursor: bool) -> ScreenInfo {
        ScreenInfo {
            cursor: (0, 0),
            hide_cursor: false,
            app_cursor,
            bracketed_paste: false,
            mouse_mode: MouseProtocolMode::None,
            mouse_encoding: MouseProtocolEncoding::Default,
            alternate: false,
            cursor_prefix: String::new(),
        }
    }

    fn bytes(key: Key, mods: Modifiers, app_cursor: bool) -> Vec<u8> {
        key_to_bytes(key, mods, &info(app_cursor)).expect("key should map to bytes")
    }

    #[test]
    fn maps_the_basic_control_keys() {
        let none = Modifiers::NONE;
        assert_eq!(bytes(Key::Enter, none, false), b"\r");
        assert_eq!(bytes(Key::Tab, none, false), b"\t");
        assert_eq!(bytes(Key::Backspace, none, false), b"\x7f");
        assert_eq!(bytes(Key::Escape, none, false), b"\x1b");
        assert_eq!(bytes(Key::Tab, Modifiers::SHIFT, false), b"\x1b[Z");
    }

    #[test]
    fn arrows_follow_the_application_cursor_mode() {
        let none = Modifiers::NONE;
        assert_eq!(bytes(Key::ArrowUp, none, false), b"\x1b[A");
        assert_eq!(bytes(Key::ArrowUp, none, true), b"\x1bOA");
        assert_eq!(bytes(Key::ArrowLeft, none, false), b"\x1b[D");
        // Modified arrows always use the CSI form with the xterm parameter:
        // 1 + shift + 2*alt + 4*ctrl.
        assert_eq!(bytes(Key::ArrowRight, Modifiers::CTRL, true), b"\x1b[1;5C");
        assert_eq!(bytes(Key::ArrowRight, Modifiers::SHIFT, false), b"\x1b[1;2C");
    }

    #[test]
    fn maps_navigation_and_function_keys() {
        let none = Modifiers::NONE;
        assert_eq!(bytes(Key::Delete, none, false), b"\x1b[3~");
        assert_eq!(bytes(Key::PageUp, none, false), b"\x1b[5~");
        assert_eq!(bytes(Key::Insert, none, false), b"\x1b[2~");
        assert_eq!(bytes(Key::F1, none, false), b"\x1bOP");
        assert_eq!(bytes(Key::F5, none, false), b"\x1b[15~");
        assert_eq!(bytes(Key::F12, none, false), b"\x1b[24~");
    }

    #[test]
    fn ctrl_and_alt_letters_become_control_bytes_and_escapes() {
        assert_eq!(bytes(Key::C, Modifiers::CTRL, false), vec![0x03]);
        assert_eq!(bytes(Key::D, Modifiers::CTRL, false), vec![0x04]);
        assert_eq!(bytes(Key::A, Modifiers::CTRL, false), vec![0x01]);
        assert_eq!(bytes(Key::Space, Modifiers::CTRL, false), vec![0x00]);
        assert_eq!(bytes(Key::B, Modifiers::ALT, false), vec![0x1b, b'b']);
        // An unmodified letter is delivered as text, not as a key.
        assert!(key_to_bytes(Key::C, Modifiers::NONE, &info(false)).is_none());
    }

    #[test]
    fn encodes_mouse_events() {
        // SGR: button, 1-based column and row, M for press and m for release.
        assert_eq!(
            encode_mouse(MouseProtocolEncoding::Sgr, 0, 4, 9, true),
            b"\x1b[<0;5;10M"
        );
        assert_eq!(
            encode_mouse(MouseProtocolEncoding::Sgr, 64, 0, 0, true),
            b"\x1b[<64;1;1M"
        );
        // X10: everything offset by 32, release reported as button 3.
        assert_eq!(
            encode_mouse(MouseProtocolEncoding::Default, 0, 0, 0, true),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(
            encode_mouse(MouseProtocolEncoding::Default, 0, 0, 0, false),
            vec![0x1b, b'[', b'M', 35, 33, 33]
        );
    }

    #[test]
    fn maps_the_xterm_256_colour_cube() {
        // The 16 base colours come from the app palette.
        assert_eq!(ansi_color(1), Color32::from_rgb(0xFF, 0x5D, 0x73));
        // 6x6x6 cube: index 16 is black, 231 is white.
        assert_eq!(ansi_color(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(ansi_color(231), Color32::from_rgb(255, 255, 255));
        // 16 + 36*r + 6*g + b with the standard level table.
        assert_eq!(ansi_color(16 + 36 * 5), Color32::from_rgb(255, 0, 0));
        assert_eq!(ansi_color(16 + 6 * 2 + 3), Color32::from_rgb(0, 135, 175));
        // Greyscale ramp: 8 + 10*i.
        assert_eq!(ansi_color(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(ansi_color(255), Color32::from_rgb(238, 238, 238));
    }

    #[test]
    fn selection_ordering_is_normalised() {
        let mut state = TerminalState {
            anchor: Some((4, 10)),
            head: Some((2, 3)),
            ..Default::default()
        };
        assert_eq!(state.ordered_selection(), Some(((2, 3), (4, 10))));
        state.head = Some((4, 10));
        assert_eq!(state.ordered_selection(), None, "an empty range is no selection");
        state.clear_selection();
        assert_eq!(state.ordered_selection(), None);
    }

    #[test]
    fn selection_membership_is_end_exclusive() {
        let sel = Some(((1, 2), (1, 5)));
        assert!(!in_selection(sel, 1, 1));
        assert!(in_selection(sel, 1, 2));
        assert!(in_selection(sel, 1, 4));
        assert!(!in_selection(sel, 1, 5));
        assert!(!in_selection(sel, 0, 9));
        assert!(in_selection(Some(((1, 2), (3, 0))), 2, 40), "rows in between");
    }
}
