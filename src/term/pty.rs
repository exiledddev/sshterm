//! PTY-backed terminal sessions.
//!
//! Every session owns a real pseudo-terminal running either the user's login
//! shell or an `ssh` process, so the centre page behaves like any other
//! Linux terminal emulator. Output is fed into a `vt100::Parser` on a reader
//! thread; the UI thread only ever reads the parsed screen.

use crate::sshinfo::{self, ProbeHandle};
use crate::store::Connection;
use crate::term::suggest::LineTracker;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// What a session is attached to.
#[derive(Debug, Clone)]
pub enum SessionKind {
    /// The local login shell, so the app is a usable terminal on its own.
    Local,
    /// `ssh` to a saved connection.
    Remote(Box<Connection>),
}

impl SessionKind {
    pub fn is_remote(&self) -> bool {
        matches!(self, SessionKind::Remote(_))
    }
}

/// A live terminal session.
pub struct PtySession {
    pub id: u64,
    pub title: String,
    pub subtitle: String,
    pub kind: SessionKind,
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// False once the child process has exited.
    pub alive: Arc<AtomicBool>,
    /// Exit description, set when the child goes away.
    pub exit_note: Arc<Mutex<Option<String>>>,
    /// SSH fingerprinting results for the right sidebar.
    pub probe: Arc<ProbeHandle>,
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
        Self::spawn(
            ctx,
            cmd,
            SessionKind::Local,
            "Local shell".to_string(),
            shell_name,
            cols,
            rows,
        )
    }

    /// Opens an ssh session for a saved connection and starts fingerprinting
    /// the server in the background.
    pub fn remote(
        ctx: &eframe::egui::Context,
        conn: &Connection,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        let argv = conn.ssh_argv();
        let mut cmd = CommandBuilder::new(&argv[0]);
        for a in &argv[1..] {
            cmd.arg(a);
        }
        cmd.env("TERM", "xterm-256color");
        if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }
        let session = Self::spawn(
            ctx,
            cmd,
            SessionKind::Remote(Box::new(conn.clone())),
            conn.name.clone(),
            conn.target(),
            cols,
            rows,
        )?;
        sshinfo::spawn(session.probe.clone(), conn.host.clone(), conn.port);
        Ok(session)
    }

    fn spawn(
        ctx: &eframe::egui::Context,
        mut cmd: CommandBuilder,
        kind: SessionKind,
        title: String,
        subtitle: String,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("SSCL_SESSION", "1");

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
            title,
            subtitle,
            kind,
            parser,
            alive,
            exit_note,
            probe: ProbeHandle::new(),
            line: LineTracker::default(),
            scroll: 0,
            cols,
            rows,
            started: crate::store::timestamp(),
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

    /// The remote connection behind this session, if any.
    pub fn connection(&self) -> Option<&Connection> {
        match &self.kind {
            SessionKind::Remote(c) => Some(c),
            SessionKind::Local => None,
        }
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

/// Chooses the program, arguments and environment used for a local shell,
/// wiring in the generated SSCL prompt where the shell supports it.
fn login_shell_command() -> (String, Vec<String>, Vec<(String, String)>) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let name = PathBuf::from(&shell)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    match name.as_str() {
        "bash" => match write_prompt_file("sscl.bashrc", BASH_PROMPT) {
            Ok(path) => (
                shell,
                vec![
                    "--rcfile".into(),
                    path.to_string_lossy().into_owned(),
                    "-i".into(),
                ],
                vec![],
            ),
            Err(_) => (shell, vec!["-i".into()], vec![]),
        },
        "zsh" => match write_zdotdir() {
            Ok(dir) => (
                shell,
                vec!["-i".into()],
                vec![("ZDOTDIR".into(), dir.to_string_lossy().into_owned())],
            ),
            Err(_) => (shell, vec!["-i".into()], vec![]),
        },
        "fish" => match write_prompt_file("sscl.fish", FISH_PROMPT) {
            Ok(path) => (
                shell,
                vec!["-C".into(), format!("source {}", path.display())],
                vec![],
            ),
            Err(_) => (shell, vec![], vec![]),
        },
        _ => (shell, vec!["-i".into()], vec![]),
    }
}

fn write_prompt_file(name: &str, contents: &str) -> std::io::Result<PathBuf> {
    let dir = crate::store::app_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    std::fs::write(&path, contents)?;
    Ok(path)
}

fn write_zdotdir() -> std::io::Result<PathBuf> {
    let dir = crate::store::app_dir().join("zdotdir");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(".zshrc"), ZSH_PROMPT)?;
    Ok(dir)
}

/// Generated bash rc: sources the user's own configuration first, then
/// installs the two-line SSCL prompt.
const BASH_PROMPT: &str = r##"# Generated by SSCL (Secure Shell Command Line). Safe to delete.
[ -f /etc/bash.bashrc ] && . /etc/bash.bashrc
[ -f "$HOME/.bashrc" ] && . "$HOME/.bashrc"

__sscl_git() {
  local b
  b=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null) || return
  local dirty=""
  [ -n "$(git status --porcelain 2>/dev/null)" ] && dirty="*"
  printf ' \001\033[38;5;60m\002─\001\033[0m\002 \001\033[38;5;215m\002%s%s\001\033[0m\002' "$b" "$dirty"
}

__sscl_ps1() {
  local ec=$?
  local dot arrow code=""
  if [ $ec -eq 0 ]; then dot='\[\e[38;5;84m\]●'; arrow='\[\e[1;38;5;141m\]❯'
  else dot='\[\e[38;5;203m\]●'; arrow='\[\e[1;38;5;203m\]❯'; code=" \[\e[38;5;203m\][$ec]"; fi
  PS1="\[\e[38;5;60m\]╭─\[\e[0m\] ${dot} \[\e[38;5;141m\]\u\[\e[38;5;60m\]@\[\e[38;5;86m\]\h\[\e[0m\] \[\e[38;5;60m\]─\[\e[0m\] \[\e[38;5;110m\]\w\[\e[0m\]\$(__sscl_git)${code}\[\e[0m\]\n\[\e[38;5;60m\]╰─\[\e[0m\]${arrow}\[\e[0m\] "
  PS2="\[\e[38;5;60m\]  ·\[\e[0m\] "
}
PROMPT_COMMAND=__sscl_ps1
"##;

/// Generated .zshrc used through ZDOTDIR so the user's own files are untouched.
const ZSH_PROMPT: &str = r##"# Generated by SSCL (Secure Shell Command Line). Safe to delete.
[ -f /etc/zsh/zshrc ] && source /etc/zsh/zshrc
[ -f "$HOME/.zshrc" ] && source "$HOME/.zshrc"

setopt prompt_subst
autoload -Uz vcs_info 2>/dev/null
__sscl_git() {
  local b
  b=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null) || return
  local dirty=""
  [ -n "$(git status --porcelain 2>/dev/null)" ] && dirty="*"
  print -n " %F{60}─%f %F{215}${b}${dirty}%f"
}
PROMPT='%F{60}╭─%f %(?.%F{84}●%f.%F{203}●%f) %F{141}%n%F{60}@%F{86}%m%f %F{60}─%f %F{110}%~%f$(__sscl_git)%(?..  %F{203}[%?]%f)
%F{60}╰─%f%(?.%B%F{141}❯%f%b.%B%F{203}❯%f%b) '
PROMPT2='%F{60}  ·%f '
"##;

/// Generated fish snippet, sourced with `fish -C`.
const FISH_PROMPT: &str = r##"# Generated by SSCL (Secure Shell Command Line). Safe to delete.
function fish_prompt
    set -l ec $status
    set_color 60; echo -n '╭─ '
    if test $ec -eq 0
        set_color 84
    else
        set_color 203
    end
    echo -n '● '
    set_color 141; echo -n (whoami)
    set_color 60;  echo -n '@'
    set_color 86;  echo -n (prompt_hostname)
    set_color 60;  echo -n ' ─ '
    set_color 110; echo -n (prompt_pwd)
    set -l branch (git symbolic-ref --short HEAD 2>/dev/null; or git rev-parse --short HEAD 2>/dev/null)
    if test -n "$branch"
        set_color 60; echo -n ' ─ '
        set_color 215; echo -n $branch
    end
    if test $ec -ne 0
        set_color 203; echo -n "  [$ec]"
    end
    echo
    set_color 60; echo -n '╰─'
    if test $ec -eq 0
        set_color -o 141
    else
        set_color -o 203
    end
    echo -n '❯ '
    set_color normal
end
"##;
