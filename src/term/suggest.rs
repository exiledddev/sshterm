//! Fish-style command autosuggestions for the terminal page.
//!
//! The shell inside the PTY does its own line editing, so ACLI mirrors the
//! keystrokes it forwards in order to know what the current line looks like.
//! Whenever an operation cannot be modelled faithfully (tab completion,
//! history search, arrow-key history) the mirror marks itself out of sync and
//! suggestions are suppressed until the next `Enter`. That way a suggestion is
//! only ever shown when it is known to be correct.

use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Where a candidate came from, used for the little tag in the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    History,
    Command,
}

impl Source {
    pub fn tag(self) -> &'static str {
        match self {
            Source::History => "history",
            Source::Command => "command",
        }
    }
}

/// One suggestion offered to the user.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    pub source: Source,
}

/// Mirror of the line currently being typed in a session.
#[derive(Debug, Default, Clone)]
pub struct LineTracker {
    chars: Vec<char>,
    cursor: usize,
    /// False when the mirror can no longer be trusted.
    in_sync: bool,
    /// Index of the highlighted entry in the suggestion popup.
    pub selected: usize,
}

impl LineTracker {
    pub fn line(&self) -> String {
        self.chars.iter().collect()
    }

    /// The text before the cursor, which is what suggestions complete.
    pub fn prefix(&self) -> String {
        self.chars[..self.cursor.min(self.chars.len())]
            .iter()
            .collect()
    }

    /// Suggestions are only offered on a trusted, non-empty line whose
    /// cursor sits at the end — exactly like fish.
    pub fn can_suggest(&self) -> bool {
        self.in_sync && self.cursor == self.chars.len() && !self.chars.is_empty()
    }

    pub fn insert_str(&mut self, text: &str) {
        if !self.in_sync {
            return;
        }
        for c in text.chars() {
            if c == '\r' || c == '\n' {
                self.reset();
                continue;
            }
            if c.is_control() {
                self.desync();
                return;
            }
            self.chars.insert(self.cursor, c);
            self.cursor += 1;
        }
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        if self.in_sync && self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
            self.selected = 0;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.in_sync && self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
            self.selected = 0;
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Ctrl-W: delete the word before the cursor.
    pub fn delete_word(&mut self) {
        if !self.in_sync {
            return;
        }
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
        self.selected = 0;
    }

    /// Ctrl-K: kill to end of line.
    pub fn kill_to_end(&mut self) {
        if self.in_sync {
            self.chars.truncate(self.cursor);
        }
    }

    /// Returns the finished line and starts a new one.
    pub fn submit(&mut self) -> Option<String> {
        let line = if self.in_sync {
            let l = self.line();
            let trimmed = l.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        } else {
            None
        };
        self.reset();
        line
    }

    /// Line thrown away (Ctrl-C, Ctrl-U, or a fresh prompt).
    pub fn reset(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.in_sync = true;
        self.selected = 0;
    }

    /// True when the shell is sitting at an empty prompt we are in step
    /// with — the only moment it is safe to type a command into it.
    pub fn at_fresh_prompt(&self) -> bool {
        self.in_sync && self.chars.is_empty()
    }

    /// The shell changed the line in a way we cannot follow.
    pub fn desync(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.in_sync = false;
        self.selected = 0;
    }
}

/// Suggestion database: shell history plus the executables on `$PATH`.
pub struct Suggestions {
    /// Oldest first; the newest entries win when several match.
    history: Vec<String>,
    /// Sorted, de-duplicated executable names found on `$PATH`.
    commands: Arc<Mutex<Vec<String>>>,
    scanning: Arc<AtomicBool>,
    history_path: std::path::PathBuf,
}

impl Default for Suggestions {
    fn default() -> Self {
        Self::new()
    }
}

impl Suggestions {
    pub fn new() -> Self {
        let history_path = crate::paths::history_file();
        let mut history = read_lines(&history_path, 5_000);
        if history.is_empty() {
            // Seed from the user's own shell history so suggestions are
            // useful on first run. Stays entirely on this machine.
            if let Some(home) = dirs::home_dir() {
                history = read_lines(&home.join(".bash_history"), 2_000);
                if history.is_empty() {
                    history = read_zsh_history(&home.join(".zsh_history"), 2_000);
                }
            }
        }
        dedup_keep_last(&mut history);

        let commands = Arc::new(Mutex::new(Vec::new()));
        let scanning = Arc::new(AtomicBool::new(true));
        {
            let commands = commands.clone();
            let scanning = scanning.clone();
            std::thread::spawn(move || {
                let found = scan_path();
                if let Ok(mut slot) = commands.lock() {
                    *slot = found;
                }
                scanning.store(false, Ordering::SeqCst);
            });
        }

        Self {
            history,
            commands,
            scanning,
            history_path,
        }
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::SeqCst)
    }

    pub fn command_count(&self) -> usize {
        self.commands.lock().map(|c| c.len()).unwrap_or(0)
    }

    pub fn history_count(&self) -> usize {
        self.history.len()
    }

    /// Records an executed command and appends it to the history file.
    pub fn record(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.len() > 4096 {
            return;
        }
        if self.history.last().map(String::as_str) == Some(line) {
            return;
        }
        self.history.retain(|h| h != line);
        self.history.push(line.to_string());
        if self.history.len() > 5_000 {
            let excess = self.history.len() - 5_000;
            self.history.drain(..excess);
        }
        if let Some(parent) = self.history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Up to `max` completions, most relevant first.
    pub fn candidates(&self, prefix: &str, max: usize) -> Vec<Candidate> {
        match self.commands.lock() {
            Ok(commands) => match_candidates(&self.history, &commands, prefix, max),
            Err(_) => match_candidates(&self.history, &[], prefix, max),
        }
    }
}

/// Ranks completions for `prefix`: the most recent matching history entries
/// first, then executable names while the command word is still being typed.
///
/// `commands` must be sorted, which lets the scan start at the first possible
/// match instead of walking the whole list.
fn match_candidates(
    history: &[String],
    commands: &[String],
    prefix: &str,
    max: usize,
) -> Vec<Candidate> {
    if prefix.is_empty() || max == 0 {
        return Vec::new();
    }
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();

    for line in history.iter().rev() {
        if line.len() > prefix.len() && line.starts_with(prefix) && seen.insert(line.as_str()) {
            out.push(Candidate {
                text: line.clone(),
                source: Source::History,
            });
            if out.len() >= max {
                return out;
            }
        }
    }

    if !prefix.contains(char::is_whitespace) {
        let start = commands.partition_point(|c| c.as_str() < prefix);
        for name in commands[start..].iter() {
            if !name.starts_with(prefix) {
                break;
            }
            if name.len() > prefix.len() && seen.insert(name.as_str()) {
                out.push(Candidate {
                    text: name.clone(),
                    source: Source::Command,
                });
                if out.len() >= max {
                    return out;
                }
            }
        }
    }
    out
}

fn dedup_keep_last(lines: &mut Vec<String>) {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(lines.len());
    for line in lines.iter().rev() {
        if seen.insert(line.clone()) {
            out.push(line.clone());
        }
    }
    out.reverse();
    *lines = out;
}

fn read_lines(path: &std::path::Path, limit: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.len() <= 4096)
        .map(str::to_string)
        .collect();
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    lines
}

/// zsh writes `: <timestamp>:<elapsed>;<command>` in extended history mode.
fn read_zsh_history(path: &std::path::Path, limit: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| match l.strip_prefix(':') {
            Some(rest) => rest.split_once(';').map(|(_, cmd)| cmd).unwrap_or(l).trim(),
            None => l.trim(),
        })
        .filter(|l| !l.is_empty() && l.len() <= 4096)
        .map(str::to_string)
        .collect();
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    lines
}

/// Collects the executables reachable through `$PATH`, plus common shell
/// builtins that never appear there.
fn scan_path() -> Vec<String> {
    const BUILTINS: &[&str] = &[
        "cd", "export", "alias", "unalias", "source", "history", "jobs", "fg", "bg", "kill",
        "exit", "pushd", "popd", "dirs", "set", "unset", "echo", "printf", "read", "test",
        "type", "which", "umask", "wait", "trap", "exec", "eval", "let", "local", "return",
    ];

    let mut set: HashSet<String> = BUILTINS.iter().map(|s| s.to_string()).collect();
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':').filter(|d| !d.is_empty()) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten().take(8_000) {
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_dir() {
                    continue;
                }
                if !is_executable(&entry.path()) {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    set.insert(name.to_string());
                }
            }
        }
    }
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    out
}

fn is_executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mirrors_ordinary_typing() {
        let mut t = LineTracker::default();
        t.reset();
        t.insert_str("git st");
        assert_eq!(t.line(), "git st");
        assert_eq!(t.prefix(), "git st");
        assert!(t.can_suggest());

        t.backspace();
        assert_eq!(t.line(), "git s");

        // With the cursor away from the end, fish suppresses suggestions.
        t.left();
        assert!(!t.can_suggest());
        t.end();
        assert!(t.can_suggest());

        assert_eq!(t.submit().as_deref(), Some("git s"));
        assert_eq!(t.line(), "");
        assert!(!t.can_suggest(), "an empty line has nothing to suggest");
    }

    #[test]
    fn word_and_line_kills_track_the_shell() {
        let mut t = LineTracker::default();
        t.reset();
        t.insert_str("cargo build --release");
        t.delete_word();
        assert_eq!(t.line(), "cargo build ");
        t.delete_word();
        assert_eq!(t.line(), "cargo ");

        t.reset();
        t.insert_str("echo hello");
        t.home();
        t.kill_to_end();
        assert_eq!(t.line(), "");
    }

    #[test]
    fn stops_guessing_once_the_shell_rewrites_the_line() {
        let mut t = LineTracker::default();
        t.reset();
        t.insert_str("ls -l");
        // Tab completion, history search and arrow-history are not modelled.
        t.desync();
        assert!(!t.can_suggest());
        // Typing while out of sync must not resurrect a stale line.
        t.insert_str("more");
        assert_eq!(t.line(), "");
        assert!(!t.can_suggest());
        // Enter resynchronises, and records nothing it is unsure about.
        assert_eq!(t.submit(), None);
        t.insert_str("ok");
        assert!(t.can_suggest());
    }

    #[test]
    fn history_beats_commands_and_newest_wins() {
        let history = strings(&["git status", "git stash", "git status --short"]);
        let commands = strings(&["git", "gitk", "grep"]);

        let out = match_candidates(&history, &commands, "git s", 5);
        assert_eq!(out[0].text, "git status --short", "newest match first");
        assert_eq!(out[0].source, Source::History);
        assert!(out.iter().all(|c| c.source == Source::History));

        // A bare word also offers executables, after the history.
        let out = match_candidates(&history, &commands, "git", 5);
        assert_eq!(out[0].source, Source::History);
        assert!(
            out.iter().any(|c| c.text == "gitk" && c.source == Source::Command),
            "{out:?}"
        );
        // "git" itself is not a completion of "git".
        assert!(!out.iter().any(|c| c.text == "git"));
    }

    #[test]
    fn arguments_are_completed_from_history_only() {
        let history = strings(&["ssh deploy@example.com"]);
        let commands = strings(&["scp", "ssh", "sshd"]);
        let out = match_candidates(&history, &commands, "ssh d", 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "ssh deploy@example.com");
    }

    #[test]
    fn respects_the_candidate_limit_and_deduplicates() {
        let history = strings(&["ls", "ls -a", "ls -l", "ls -la"]);
        let commands = strings(&["ls", "lsblk", "lsof"]);
        assert_eq!(match_candidates(&history, &commands, "ls", 2).len(), 2);
        let all = match_candidates(&history, &commands, "ls", 20);
        let mut texts: Vec<&str> = all.iter().map(|c| c.text.as_str()).collect();
        let before = texts.len();
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(before, texts.len(), "candidates must be unique");
        assert!(match_candidates(&history, &commands, "", 5).is_empty());
    }

    #[test]
    fn parses_extended_zsh_history() {
        let dir = std::env::temp_dir().join(format!("acli-zsh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".zsh_history");
        std::fs::write(
            &path,
            ": 1700000000:0;cargo test\n: 1700000001:12;git push\nplain command\n",
        )
        .unwrap();
        let lines = read_zsh_history(&path, 100);
        assert_eq!(lines, strings(&["cargo test", "git push", "plain command"]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
