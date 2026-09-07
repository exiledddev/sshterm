//! Where ACLI keeps its files.
//!
//! ```text
//! ~/.config/acli/config.json     prompt colours
//! ~/.local/share/acli/history    shell history for the suggestion engine
//! ~/.local/share/acli/acli.bashrc, acli.zshrc, acli.fish
//!                                generated prompts
//! ```

use std::path::PathBuf;

/// Set once by the tests to redirect everything into a scratch directory.
static DATA_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Redirects [`data_dir`] and [`config_dir`] for the lifetime of the process.
pub fn override_dirs(path: PathBuf) {
    let _ = DATA_DIR_OVERRIDE.set(path);
}

/// `~/.local/share/acli` (or `$XDG_DATA_HOME/acli`).
pub fn data_dir() -> PathBuf {
    if let Some(path) = DATA_DIR_OVERRIDE.get() {
        return path.clone();
    }
    dirs::data_dir()
        .unwrap_or_else(|| home().join(".local/share"))
        .join("acli")
}

/// `~/.config/acli` (or `$XDG_CONFIG_HOME/acli`).
pub fn config_dir() -> PathBuf {
    if let Some(path) = DATA_DIR_OVERRIDE.get() {
        return path.clone();
    }
    dirs::config_dir()
        .unwrap_or_else(|| home().join(".config"))
        .join("acli")
}

pub fn history_file() -> PathBuf {
    data_dir().join("history")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `YYYY-MM-DD HH:MM:SS` in UTC, computed from the system clock without
/// pulling in a date-time dependency.
pub fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;

    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);

    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}
