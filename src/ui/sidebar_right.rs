//! Right sidebar — Session Info.
//!
//! Shows everything the passive probe could learn about the SSH server the
//! current session is talking to.

use crate::icons;
use crate::sshinfo::{ProbeState, SshInfo};
use crate::term::pty::{PtySession, SessionKind};
use crate::theme;
use crate::ui::widgets;
use eframe::egui::{self, Color32, FontFamily, FontId};

/// Actions the sidebar can request.
pub enum InfoAction {
    Rescan,
    Copy(String),
}

pub fn show(ui: &mut egui::Ui, session: Option<&PtySession>) -> Option<InfoAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Session Info")
                .font(FontId::new(13.0, FontFamily::Proportional))
                .color(theme::TEXT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let remote = session.map(|s| s.kind.is_remote()).unwrap_or(false);
            if remote
                && widgets::icon_button(
                    ui,
                    icons::refresh,
                    24.0,
                    theme::TEXT_DIM,
                    false,
                    "Probe the server again",
                )
                .clicked()
            {
                action = Some(InfoAction::Rescan);
            }
        });
    });
    ui.add_space(8.0);

    let Some(session) = session else {
        placeholder(ui, "No session", "Start a saved connection, or use the local shell.");
        return action;
    };

    match &session.kind {
        SessionKind::Local => {
            local_info(ui, session);
            None
        }
        SessionKind::Remote(conn) => {
            let state = session.probe.state();
            let mut info = session.probe.snapshot();
            // Before the probe reports back, show what the saved connection says.
            if info.port == 0 {
                info.port = conn.port;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    remote_info(ui, session, state, &info, &mut action);
                });
            action
        }
    }
}

fn placeholder(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(26.0), egui::Sense::hover());
        icons::info(ui.painter(), rect, theme::TEXT_FAINT, 1.5);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(title)
                .font(FontId::new(12.5, FontFamily::Proportional))
                .color(theme::TEXT_DIM),
        );
        ui.add_space(3.0);
        ui.label(
            egui::RichText::new(body)
                .font(FontId::new(11.0, FontFamily::Proportional))
                .color(theme::TEXT_FAINT),
        );
    });
}

fn local_info(ui: &mut egui::Ui, session: &PtySession) {
    theme::section_label(ui, "Local session");
    widgets::info_row(ui, "Shell", &session.subtitle, true);
    widgets::info_row(
        ui,
        "Terminal",
        &format!("{}×{} cells", session.cols, session.rows),
        false,
    );
    widgets::info_row(ui, "TERM", "xterm-256color", true);
    widgets::info_row(ui, "Started", &session.started, false);
    widgets::info_row(
        ui,
        "State",
        if session.is_alive() { "running" } else { "ended" },
        false,
    );
    ui.add_space(10.0);
    theme::separator(ui);
    placeholder(
        ui,
        "Not an SSH session",
        "Server fingerprinting appears here once you start a saved connection.",
    );
}

fn remote_info(
    ui: &mut egui::Ui,
    session: &PtySession,
    state: ProbeState,
    info: &SshInfo,
    action: &mut Option<InfoAction>,
) {
    // ---- Endpoint -------------------------------------------------------
    theme::section_label(ui, "Endpoint");
    widgets::info_row(
        ui,
        "IP address",
        info.ip.as_deref().unwrap_or(&info.host),
        true,
    );
    if info.ip.as_deref() != Some(info.host.as_str()) && !info.host.is_empty() {
        widgets::info_row(ui, "Host", &info.host, true);
    }
    widgets::info_row(ui, "Port", &info.port.to_string(), true);
    widgets::info_row(ui, "User", &session.subtitle, true);
    widgets::info_row(ui, "Opened", &session.started, false);
    ui.add_space(6.0);

    if state == ProbeState::Running {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
            let t = ui.input(|i| i.time) as f32;
            crate::ui::splash::spinner(ui.painter(), rect.center(), 6.5, t, 1.0);
            ui.label(
                egui::RichText::new("Fingerprinting server…")
                    .font(FontId::new(11.5, FontFamily::Proportional))
                    .color(theme::TEXT_DIM),
            );
        });
        ui.ctx().request_repaint();
        return;
    }
    if state == ProbeState::Idle {
        return;
    }

    if let Some(err) = &info.error {
        theme::separator(ui);
        ui.horizontal_top(|ui| {
            ui.label(
                egui::RichText::new("⚠")
                    .font(FontId::new(12.0, FontFamily::Proportional))
                    .color(theme::WARN),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(err)
                        .font(FontId::new(11.5, FontFamily::Proportional))
                        .color(theme::WARN),
                )
                .wrap(),
            );
        });
        ui.add_space(8.0);
        if widgets::ghost_button(ui, "Probe again", theme::TEXT_DIM).clicked() {
            *action = Some(InfoAction::Rescan);
        }
        ui.add_space(8.0);
    }

    // ---- Server software ------------------------------------------------
    if !info.banner.is_empty() {
        theme::separator(ui);
        theme::section_label(ui, "Server software");
        widgets::info_row(ui, "Identification", &info.banner, true);
        widgets::info_row(ui, "Software", &info.software, true);
        if !info.comment.is_empty() {
            widgets::info_row(ui, "Comment", &info.comment, true);
        }
        widgets::info_row(ui, "Protocol", &info.protocol, true);
        if !info.preamble.is_empty() {
            widgets::info_row(ui, "Pre-auth banner", &info.preamble.join(" / "), false);
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if widgets::ghost_button(ui, "Copy identification", theme::TEXT_DIM).clicked() {
                *action = Some(InfoAction::Copy(info.banner.clone()));
            }
        });
        ui.add_space(6.0);
    }

    // ---- OS clues -------------------------------------------------------
    if !info.os_clues.is_empty() {
        theme::separator(ui);
        theme::section_label(ui, "Operating-system clues");
        for clue in &info.os_clues {
            bullet(ui, clue, theme::TEXT_DIM);
        }
        ui.add_space(6.0);
    }

    // ---- Host keys ------------------------------------------------------
    theme::separator(ui);
    theme::section_label(ui, "Host keys");
    if info.host_keys.is_empty() {
        ui.label(
            egui::RichText::new("No host keys retrieved.")
                .font(FontId::new(11.0, FontFamily::Proportional))
                .color(theme::TEXT_FAINT),
        );
    }
    for key in &info.host_keys {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(13.0), egui::Sense::hover());
            icons::shield(ui.painter(), rect, theme::ACCENT_ALT, 1.4);
            let label = egui::Label::new(
                egui::RichText::new(&key.algorithm)
                    .font(FontId::new(11.5, FontFamily::Monospace))
                    .color(theme::TEXT),
            )
            .wrap()
            .sense(egui::Sense::click());
            if ui
                .add(label)
                .on_hover_text("Click to copy the public host key")
                .clicked()
            {
                *action = Some(InfoAction::Copy(format!(
                    "{} {}",
                    key.algorithm, key.blob_b64
                )));
            }
        });
        ui.add_space(2.0);
        let fp = egui::RichText::new(&key.fingerprint)
            .font(FontId::new(10.5, FontFamily::Monospace))
            .color(theme::ACCENT_ALT);
        if ui
            .add(egui::Label::new(fp).wrap().sense(egui::Sense::click()))
            .on_hover_text("Click to copy the fingerprint")
            .clicked()
        {
            *action = Some(InfoAction::Copy(key.fingerprint.clone()));
        }
        if let Some(bits) = key.bits {
            ui.label(
                egui::RichText::new(format!("{bits} bit"))
                    .font(FontId::new(10.0, FontFamily::Proportional))
                    .color(theme::TEXT_FAINT),
            );
        }
        ui.add_space(9.0);
    }

    // ---- Algorithms -----------------------------------------------------
    theme::separator(ui);
    theme::section_label(ui, "Negotiable algorithms");
    algo_list(ui, "Key exchange", &info.kex);
    algo_list(ui, "Host key", &info.host_key_algorithms);
    algo_list(ui, "Ciphers (server→client)", &info.ciphers_s2c);
    if info.ciphers_c2s != info.ciphers_s2c {
        algo_list(ui, "Ciphers (client→server)", &info.ciphers_c2s);
    }
    algo_list(ui, "MAC / integrity", &info.macs_s2c);
    if info.macs_c2s != info.macs_s2c {
        algo_list(ui, "MAC (client→server)", &info.macs_c2s);
    }
    algo_list(ui, "Compression", &info.compression_s2c);
    if !info.languages.is_empty() {
        algo_list(ui, "Languages", &info.languages);
    }

    // ---- Quirks ---------------------------------------------------------
    if !info.quirks.is_empty() {
        theme::separator(ui);
        theme::section_label(ui, "Implementation quirks");
        for q in &info.quirks {
            bullet(ui, q, theme::TEXT_DIM);
        }
    }

    if !info.notes.is_empty() {
        theme::separator(ui);
        theme::section_label(ui, "Notes");
        for n in &info.notes {
            bullet(ui, n, theme::TEXT_FAINT);
        }
    }

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(
            "Algorithm lists and the version string come from the server's own \
             key-exchange announcement; OS clues and quirks are inferences from them.",
        )
        .font(FontId::new(10.0, FontFamily::Proportional))
        .color(theme::TEXT_FAINT),
    );
    ui.add_space(12.0);
}

fn algo_list(ui: &mut egui::Ui, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("{title}  ({})", items.len()))
            .font(FontId::new(11.5, FontFamily::Proportional))
            .color(theme::TEXT_DIM),
    )
    .id_salt(title)
    .default_open(false)
    .show(ui, |ui| {
        for item in items {
            ui.label(
                egui::RichText::new(item)
                    .font(FontId::new(10.5, FontFamily::Monospace))
                    .color(theme::TEXT),
            );
        }
    });
    ui.add_space(2.0);
}

fn bullet(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            egui::RichText::new("▸")
                .font(FontId::new(11.0, FontFamily::Monospace))
                .color(theme::ACCENT),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(color),
            )
            .wrap(),
        );
    });
    ui.add_space(3.0);
}
