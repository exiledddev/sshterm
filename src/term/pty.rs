//! PTY-backed terminal sessions.
//!
//! Every session owns a real pseudo-terminal running either the user's login
//! shell or an `ssh` process, so the centre page behaves like any other
//! Linux terminal emulator. Output is fed into a `vt100::Parser` on a reader
//! thread; the UI thread only ever reads the parsed screen.

use crate::prompt::{PromptTheme, Shell};
use crate::term::suggest::LineTracker;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A live terminal session.
pub struct PtySession {
    pub id: u64,
    /// Name of the program the pane is running, e.g. `bash`.
    pub shell: String,
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// False once the child process has exited.
    pub alive: Arc<AtomicBool>,
    /// Exit description, set when the child goes away.
    pub exit_note: Arc<Mutex<Option<String>>>,
    /// Tracks the line being typed, for fish-style suggestions.
    pub line: LineTracker,
    /// Current scrollback offset in rows (0 = following the live output).
    pub scroll: usize,
    /// Grid size currently negotiated with the child.
    pub cols: u16,
    pub rows: u16,
    pub started: String,

    writer: Option<Box<dyn Write + Send>>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl PtySession {
    /// Opens a local shell session with the generated pretty prompt.
    pub fn local(ctx: &eframe::egui::Context, cols: u16, rows: u16) -> Result<Self, String> {
        let (program, args, extra_env) = login_shell_command();
        let mut cmd = CommandBuilder::new(&program);
        for a in &args {
            cmd.arg(a);
        }
        for (k, v) in &extra_env {
            cmd.env(k, v);
        }
        if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }
        let shell_name = PathBuf::from(&program)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.clone());
        Self::spawn(ctx, cmd, shell_name, cols, rows)
    }

    fn spawn(
        ctx: &eframe::egui::Context,
        mut cmd: CommandBuilder,
        shell_name: String,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("ACLI_SESSION", "1");

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Cannot open a pseudo-terminal: {e}"))?;

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Cannot start the session: {e}"))?;
        // The slave handle must be dropped or the reader never sees EOF.
        drop(pair.slave);

        let killer = child.clone_killer();
        let master = pair.master;
        let reader = master
            .try_clone_reader()
            .map_err(|e| format!("Cannot read from the pseudo-terminal: {e}"))?;
        let writer = master
            .take_writer()
            .map_err(|e| format!("Cannot write to the pseudo-terminal: {e}"))?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 10_000)));
        let alive = Arc::new(AtomicBool::new(true));
        let exit_note = Arc::new(Mutex::new(None));

        // Reader thread: PTY output -> vt100 parser.
        {
            let parser = parser.clone();
            let ctx = ctx.clone();
            let alive = alive.clone();
            std::thread::spawn(move || {
                let mut reader = reader;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut p) = parser.lock() {
                                p.process(&buf[..n]);
                            }
                            ctx.request_repaint();
                        }
                    }
                }
                alive.store(false, Ordering::SeqCst);
                ctx.request_repaint();
            });
        }

        // Reaper thread: avoids zombies and records how the child ended.
        {
            let alive = alive.clone();
            let exit_note = exit_note.clone();
            let ctx = ctx.clone();
            let mut child = child;
            std::thread::spawn(move || {
                let note = match child.wait() {
                    Ok(status) if status.success() => "Session ended.".to_string(),
                    Ok(status) => match status.signal() {
                        Some(sig) => format!("Session terminated by signal {sig}."),
                        None => format!("Session ended with exit code {}.", status.exit_code()),
                    },
                    Err(e) => format!("Session ended: {e}"),
                };
                if let Ok(mut slot) = exit_note.lock() {
                    *slot = Some(note);
                }
                alive.store(false, Ordering::SeqCst);
                ctx.request_repaint();
            });
        }

        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            shell: shell_name,
            parser,
            alive,
            exit_note,
            line: LineTracker::default(),
            scroll: 0,
            cols,
            rows,
            started: crate::paths::timestamp(),
            writer: Some(writer),
            master,
            killer,
        })
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn exit_note(&self) -> Option<String> {
        self.exit_note.lock().ok().and_then(|g| g.clone())
    }

    /// Sends raw bytes to the child process.
    pub fn write(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || !self.is_alive() {
            return;
        }
        if let Some(w) = self.writer.as_mut() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
        // Any keystroke means the user wants to see the live output again.
        if self.scroll != 0 {
            self.scroll_to_bottom();
        }
    }

    /// Re-negotiates the grid size with the child. No-op when unchanged.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(8);
        let rows = rows.max(2);
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_size(rows, cols);
        }
    }

    /// Adjusts the scrollback offset, clamped to what the parser holds.
    pub fn scroll_by(&mut self, delta: isize) {
        let max = 10_000usize;
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, max as isize) as usize;
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_scrollback(self.scroll);
            // The parser clamps to what actually exists; read it back.
            self.scroll = p.screen().scrollback();
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = 0;
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_scrollback(0);
        }
    }

    /// Terminates the child process.
    pub fn terminate(&mut self) {
        let _ = self.killer.kill();
        self.alive.store(false, Ordering::SeqCst);
        // Dropping the writer sends EOF, which unblocks a shell waiting on input.
        self.writer = None;
    }

}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}

// ---------------------------------------------------------------------------
// Login shell + generated prompt
// ---------------------------------------------------------------------------

/// Chooses the program, arguments and environment for a local shell, wiring
/// in the generated ACLI prompt where the shell supports one.
///
/// The rc file is rewritten on every launch, so a colour changed in Settings
/// takes effect in the next pane you open.
fn login_shell_command() -> (String, Vec<String>, Vec<(String, String)>) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let path = PathBuf::from(&shell);
    let (theme, _) = PromptTheme::load();

    let plain = (shell.clone(), vec!["-i".to_string()], Vec::new());
    let Some(kind) = Shell::from_path(&path) else {
        return plain;
    };
    let Ok(rc) = theme.write_rc(kind) else {
        return plain;
    };

    match kind {
        Shell::Bash => (
            shell,
            vec!["--rcfile".into(), rc.to_string_lossy().into_owned(), "-i".into()],
            Vec::new(),
        ),
        // zsh only reads a directory, so the generated file is placed in one
        // of its own and ZDOTDIR points at it. The user's ~/.zshrc is sourced
        // from inside it, so nothing of theirs is bypassed.
        Shell::Zsh => match install_zdotdir(&rc) {
            Ok(dir) => (
                shell,
                vec!["-i".into()],
                vec![("ZDOTDIR".into(), dir.to_string_lossy().into_owned())],
            ),
            Err(_) => plain,
        },
        Shell::Fish => (
            shell,
            vec!["-C".into(), format!("source {}", rc.display())],
            Vec::new(),
        ),
    }
}

/// Copies the generated zsh prompt into a ZDOTDIR as `.zshrc`.
fn install_zdotdir(rc: &std::path::Path) -> std::io::Result<PathBuf> {
    let dir = crate::paths::data_dir().join("zdotdir");
    std::fs::create_dir_all(&dir)?;
    std::fs::copy(rc, dir.join(".zshrc"))?;
    Ok(dir)
}

/// Re-sources the generated prompt in a shell that is sitting at a fresh
/// prompt, so a colour change can be seen without opening a new pane.
pub fn reload_prompt_command() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    let kind = Shell::from_path(&PathBuf::from(&shell))?;
    let rc = crate::paths::data_dir().join(kind.rc_name());
    match kind {
        Shell::Bash | Shell::Zsh => Some(format!("source {}\n", rc.display())),
        Shell::Fish => Some(format!("source {}\n", rc.display())),
    }
}
