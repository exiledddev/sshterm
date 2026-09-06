//! On-disk storage of saved SSH connections.
//!
//! Layout (the "app folder"):
//!
//! ```text
//! ~/.local/share/sscl/
//! ├── connections/
//! │   ├── <Session name>/
//! │   │   ├── connection.json   metadata needed to rebuild the ssh command
//! │   │   └── <key file>        private key, copied in and chmod 400
//! │   └── ...
//! ├── history                   shell history used by the suggestion engine
//! └── sscl.bashrc / sscl.zshrc  generated pretty prompts
//! ```

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Name of the metadata file inside each connection folder.
pub const META_FILE: &str = "connection.json";

/// A saved SSH connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// Display name shown in the Session Browser; also the folder name.
    pub name: String,
    /// Public IP address (or hostname) of the SSH server.
    pub host: String,
    /// Remote user name.
    pub username: String,
    /// TCP port. Defaults to 22 when missing from the file.
    #[serde(default = "default_port")]
    pub port: u16,
    /// File name (not path) of the private key inside the connection folder.
    #[serde(default)]
    pub key_file: String,
    /// Free-form note, shown as a subtitle in the browser.
    #[serde(default)]
    pub note: String,
    /// RFC-3339-ish creation stamp, informational only.
    #[serde(default)]
    pub created: String,

    /// Absolute path of the folder this connection was loaded from.
    /// Not serialised — it is derived from where the file was found.
    #[serde(skip)]
    pub dir: PathBuf,
}

fn default_port() -> u16 {
    22
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            username: String::new(),
            port: 22,
            key_file: String::new(),
            note: String::new(),
            created: String::new(),
            dir: PathBuf::new(),
        }
    }
}

impl Connection {
    /// Absolute path of the private key, if one was stored.
    pub fn key_path(&self) -> Option<PathBuf> {
        if self.key_file.is_empty() {
            None
        } else {
            Some(self.dir.join(&self.key_file))
        }
    }

    /// `user@host`, as passed to ssh.
    pub fn target(&self) -> String {
        format!("{}@{}", self.username, self.host)
    }

    /// The argument vector used to open this connection.
    pub fn ssh_argv(&self) -> Vec<String> {
        let mut v = vec!["ssh".to_string()];
        if let Some(key) = self.key_path() {
            v.push("-i".into());
            v.push(key.to_string_lossy().into_owned());
            // Only offer the key we stored, never the whole agent keyring.
            v.push("-o".into());
            v.push("IdentitiesOnly=yes".into());
        }
        if self.port != 22 {
            v.push("-p".into());
            v.push(self.port.to_string());
        }
        v.push(self.target());
        v
    }

    /// Human-readable preview of the command that will be run.
    pub fn command_preview(&self) -> String {
        self.ssh_argv().join(" ")
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Set once by the tests to redirect the app folder into a scratch dir.
static APP_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Redirects [`app_dir`] for the lifetime of the process. Test-only.
pub fn override_app_dir(path: PathBuf) {
    let _ = APP_DIR_OVERRIDE.set(path);
}

/// `~/.local/share/sscl` (or `$XDG_DATA_HOME/sscl`).
pub fn app_dir() -> PathBuf {
    if let Some(path) = APP_DIR_OVERRIDE.get() {
        return path.clone();
    }
    dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share")
        })
        .join("sscl")
}

/// `<app dir>/connections` — the folder opened by "Open Connections Folder".
pub fn connections_dir() -> PathBuf {
    app_dir().join("connections")
}

/// Creates the app folders if they do not exist yet.
pub fn ensure_dirs() -> io::Result<()> {
    fs::create_dir_all(connections_dir())
}

// ---------------------------------------------------------------------------
// Loading / saving
// ---------------------------------------------------------------------------

/// Reads every connection folder, sorted by name (case-insensitive).
pub fn load_all() -> Vec<Connection> {
    let root = connections_dir();
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&root) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let meta = dir.join(META_FILE);
        let Ok(text) = fs::read_to_string(&meta) else {
            continue;
        };
        match serde_json::from_str::<Connection>(&text) {
            Ok(mut conn) => {
                conn.dir = dir;
                if conn.name.trim().is_empty()
                    && let Some(n) = conn.dir.file_name() {
                        conn.name = n.to_string_lossy().into_owned();
                    }
                out.push(conn);
            }
            Err(_) => continue,
        }
    }
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

/// Strips path separators and control characters so a session name can be
/// used as a folder name.
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == '\0' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "connection".to_string()
    } else {
        cleaned
    }
}

/// Everything the "New Connection" dialog collects.
pub struct NewConnection {
    pub name: String,
    pub host: String,
    pub username: String,
    pub port: u16,
    pub note: String,
    /// Source path of the private key that was dropped in. When `None`, no
    /// key is copied (the user may rely on an agent or a password).
    pub key_source: Option<PathBuf>,
}

/// Creates the connection folder, copies the private key in with mode 400
/// and writes the metadata file. Returns the created connection.
pub fn create(req: &NewConnection) -> Result<Connection, String> {
    if req.name.trim().is_empty() {
        return Err("Session name is required.".into());
    }
    if req.host.trim().is_empty() {
        return Err("Public IP address is required.".into());
    }
    if req.username.trim().is_empty() {
        return Err("Username is required.".into());
    }

    ensure_dirs().map_err(|e| format!("Cannot create app folder: {e}"))?;
    let dir = connections_dir().join(sanitize_name(&req.name));
    if dir.exists() {
        return Err(format!(
            "A connection folder named \"{}\" already exists.",
            dir.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;

    let key_file = match &req.key_source {
        Some(src) => match install_key(src, &dir) {
            Ok(name) => name,
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(e);
            }
        },
        None => String::new(),
    };

    let conn = Connection {
        name: req.name.trim().to_string(),
        host: req.host.trim().to_string(),
        username: req.username.trim().to_string(),
        port: req.port,
        key_file,
        note: req.note.trim().to_string(),
        created: timestamp(),
        dir,
    };
    write_meta(&conn)?;
    Ok(conn)
}

/// Applies edits to an existing connection.
///
/// * `replacement_key` — when set, the new key replaces the stored one.
/// * Renaming the session renames the folder as well.
pub fn update(
    original: &Connection,
    req: &NewConnection,
    replacement_key: Option<&Path>,
) -> Result<Connection, String> {
    if req.name.trim().is_empty() {
        return Err("Session name is required.".into());
    }
    if req.host.trim().is_empty() {
        return Err("Public IP address is required.".into());
    }
    if req.username.trim().is_empty() {
        return Err("Username is required.".into());
    }

    let mut conn = original.clone();
    conn.dir = original.dir.clone();

    // Rename the folder if the session name changed.
    let wanted_dir = connections_dir().join(sanitize_name(&req.name));
    if wanted_dir != conn.dir {
        if wanted_dir.exists() {
            return Err(format!(
                "A connection folder named \"{}\" already exists.",
                wanted_dir.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        fs::rename(&conn.dir, &wanted_dir)
            .map_err(|e| format!("Cannot rename connection folder: {e}"))?;
        conn.dir = wanted_dir;
    }

    if let Some(src) = replacement_key {
        // Remove the previous key only once the new one is safely in place.
        let previous = conn.key_path();
        let new_name = install_key(src, &conn.dir)?;
        if let Some(prev) = previous
            && prev.file_name().map(|n| n.to_string_lossy().into_owned())
                != Some(new_name.clone())
            {
                let _ = fs::remove_file(prev);
            }
        conn.key_file = new_name;
    }

    conn.name = req.name.trim().to_string();
    conn.host = req.host.trim().to_string();
    conn.username = req.username.trim().to_string();
    conn.port = req.port;
    conn.note = req.note.trim().to_string();
    write_meta(&conn)?;
    Ok(conn)
}

/// Deletes the whole connection folder, private key included.
pub fn delete(conn: &Connection) -> Result<(), String> {
    if !conn.dir.starts_with(connections_dir()) {
        return Err("Refusing to delete a folder outside the app folder.".into());
    }
    fs::remove_dir_all(&conn.dir).map_err(|e| format!("Cannot delete connection: {e}"))
}

fn write_meta(conn: &Connection) -> Result<(), String> {
    let json = serde_json::to_string_pretty(conn)
        .map_err(|e| format!("Cannot serialise connection: {e}"))?;
    fs::write(conn.dir.join(META_FILE), json)
        .map_err(|e| format!("Cannot write {META_FILE}: {e}"))
}

/// Copies a private key into `dir` and sets mode 400 so that ssh accepts it.
fn install_key(src: &Path, dir: &Path) -> Result<String, String> {
    if !src.is_file() {
        return Err(format!("{} is not a file.", src.display()));
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "id_key".to_string());
    let dest = dir.join(&name);
    fs::copy(src, &dest).map_err(|e| format!("Cannot copy private key: {e}"))?;
    chmod_400(&dest)?;
    Ok(name)
}

/// `chmod 400` — owner read only, which is what ssh insists on.
pub fn chmod_400(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o400))
            .map_err(|e| format!("Cannot chmod 400 {}: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Quick sanity check so the dialog can warn about obviously wrong files
/// (a `.pub` key, or a binary blob).
pub fn looks_like_private_key(path: &Path) -> Option<&'static str> {
    if path.extension().map(|e| e == "pub").unwrap_or(false) {
        return Some("That looks like a public key (.pub) — ssh needs the private key.");
    }
    let Ok(bytes) = fs::read(path) else {
        return Some("The file could not be read.");
    };
    if bytes.len() > 64 * 1024 {
        return Some("That file is unusually large for a private key.");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).to_string();
    if head.contains("PRIVATE KEY") {
        None
    } else if head.starts_with("ssh-") || head.starts_with("ecdsa-") {
        Some("That looks like a public key — ssh needs the private key.")
    } else {
        Some("No \"PRIVATE KEY\" header found; this may not be an OpenSSH private key.")
    }
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

/// Opens `path` in the user's default file browser.
///
/// `xdg-open` is the right answer on a desktop Linux system; the rest are
/// fallbacks for the case where xdg-utils is not installed.
pub fn open_in_file_browser(path: &Path) -> Result<(), String> {
    let _ = fs::create_dir_all(path);
    const OPENERS: &[&str] = &[
        "xdg-open", "gio", "nautilus", "dolphin", "nemo", "thunar", "pcmanfm",
    ];
    for opener in OPENERS {
        let mut cmd = std::process::Command::new(opener);
        if *opener == "gio" {
            cmd.arg("open");
        }
        let spawned = cmd
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if spawned.is_ok() {
            return Ok(());
        }
    }
    Err(format!(
        "No file browser found (tried {}). The folder is at {}.",
        OPENERS.join(", "),
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-process scratch app folder. Prepared exactly once, because the
    /// tests share one process and run in parallel.
    fn scratch() -> PathBuf {
        static SCRATCH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        SCRATCH
            .get_or_init(|| {
                let dir =
                    std::env::temp_dir().join(format!("sscl-tests-{}", std::process::id()));
                let _ = fs::remove_dir_all(&dir);
                override_app_dir(dir.clone());
                ensure_dirs().unwrap();
                dir
            })
            .clone()
    }

    #[test]
    fn sanitizes_folder_names() {
        assert_eq!(sanitize_name("web/01"), "web_01");
        assert_eq!(sanitize_name("  spaced  "), "spaced");
        assert_eq!(sanitize_name(".."), "connection");
        assert_eq!(sanitize_name(""), "connection");
        assert_eq!(sanitize_name("a\nb"), "a_b");
        // Ordinary names survive untouched.
        assert_eq!(sanitize_name("Production web server"), "Production web server");
    }

    #[test]
    fn timestamp_looks_like_a_date() {
        let ts = timestamp();
        assert!(ts.ends_with(" UTC"), "{ts}");
        assert_eq!(ts.len(), "2026-01-01 00:00:00 UTC".len(), "{ts}");
        let year: i32 = ts[..4].parse().unwrap();
        assert!(year >= 2024, "{ts}");
    }

    #[test]
    fn builds_the_ssh_command() {
        let mut conn = Connection {
            name: "box".into(),
            host: "203.0.113.9".into(),
            username: "deploy".into(),
            port: 22,
            key_file: "id_ed25519".into(),
            dir: PathBuf::from("/tmp/box"),
            ..Default::default()
        };
        assert_eq!(
            conn.ssh_argv(),
            vec![
                "ssh",
                "-i",
                "/tmp/box/id_ed25519",
                "-o",
                "IdentitiesOnly=yes",
                "deploy@203.0.113.9"
            ]
        );
        conn.port = 2222;
        assert!(conn.ssh_argv().windows(2).any(|w| w == ["-p", "2222"]));
        conn.key_file.clear();
        assert_eq!(conn.ssh_argv().first().unwrap(), "ssh");
        assert!(!conn.ssh_argv().iter().any(|a| a == "-i"));
    }

    #[test]
    fn create_edit_and_delete_round_trip() {
        let root = scratch();
        let key = root.join("round_trip_key");
        fs::write(&key, "-----BEGIN OPENSSH PRIVATE KEY-----\nzzz\n").unwrap();

        let req = NewConnection {
            name: "Edge node".into(),
            host: "198.51.100.7".into(),
            username: "root".into(),
            port: 22,
            note: "first".into(),
            key_source: Some(key.clone()),
        };
        let conn = create(&req).unwrap();

        // Folder named after the session, holding the metadata and the key.
        assert_eq!(conn.dir, connections_dir().join("Edge node"));
        assert!(conn.dir.join(META_FILE).is_file());
        let stored_key = conn.key_path().unwrap();
        assert!(stored_key.is_file());

        // chmod 400.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&stored_key).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o400, "key should be chmod 400");
        }

        // Creating the same name twice is refused rather than clobbering.
        assert!(create(&req).is_err());

        // Loading finds it again, with `dir` filled in.
        let loaded = load_all()
            .into_iter()
            .find(|c| c.name == "Edge node")
            .expect("connection should be listed");
        assert_eq!(loaded.host, "198.51.100.7");
        assert_eq!(loaded.username, "root");
        assert_eq!(loaded.dir, conn.dir);

        // Editing every detail, including a rename, moves the folder.
        let edited = NewConnection {
            name: "Edge node 2".into(),
            host: "198.51.100.8".into(),
            username: "admin".into(),
            port: 2200,
            note: "second".into(),
            key_source: None,
        };
        let updated = update(&conn, &edited, None).unwrap();
        assert_eq!(updated.dir, connections_dir().join("Edge node 2"));
        assert!(!conn.dir.exists());
        assert_eq!(updated.username, "admin");
        assert_eq!(updated.port, 2200);
        // The key came along with the folder.
        assert!(updated.key_path().unwrap().is_file());

        // Replacing the key swaps the file and re-applies the mode.
        let key2 = root.join("other_key");
        fs::write(&key2, "-----BEGIN OPENSSH PRIVATE KEY-----\nyyy\n").unwrap();
        let updated = update(&updated, &edited, Some(&key2)).unwrap();
        assert_eq!(updated.key_file, "other_key");
        assert!(!updated.dir.join("round_trip_key").exists());

        let dir = updated.dir.clone();
        delete(&updated).unwrap();
        assert!(!dir.exists());
        assert!(!load_all().iter().any(|c| c.name.starts_with("Edge node")));
    }

    #[test]
    fn refuses_to_delete_outside_the_app_folder() {
        scratch();
        let conn = Connection {
            name: "evil".into(),
            dir: PathBuf::from("/etc"),
            ..Default::default()
        };
        assert!(delete(&conn).is_err());
        assert!(PathBuf::from("/etc").exists());
    }

    #[test]
    fn spots_files_that_are_not_private_keys() {
        let root = scratch();
        let pubkey = root.join("id.pub");
        fs::write(&pubkey, "ssh-ed25519 AAAA...").unwrap();
        assert!(looks_like_private_key(&pubkey).is_some());

        let real = root.join("id_ed25519");
        fs::write(&real, "-----BEGIN OPENSSH PRIVATE KEY-----\n").unwrap();
        assert!(looks_like_private_key(&real).is_none());
    }
}
