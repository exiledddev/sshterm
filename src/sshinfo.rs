//! Passive SSH server fingerprinting for the Session Info sidebar.
//!
//! Two independent sources are used:
//!
//! 1. A raw SSH transport probe. We open a TCP socket, exchange identification
//!    strings and read the server's `SSH_MSG_KEXINIT` packet (RFC 4253 §7.1).
//!    That single packet carries every algorithm name-list the server is
//!    willing to negotiate, and the identification string carries the software
//!    version. No authentication is attempted and the connection is dropped
//!    immediately afterwards.
//! 2. `ssh-keyscan`, from openssh-client, for the host keys themselves. The
//!    SHA-256 fingerprints are computed here from the returned key blobs, so
//!    they match what `ssh-keygen -lf` and the ssh client prompt display.

use base64::Engine;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Identification string we present. Purely cosmetic for the remote end.
const CLIENT_ID: &str = "SSH-2.0-SSCL_0.1.0";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const IO_TIMEOUT: Duration = Duration::from_secs(6);

/// A host key returned by `ssh-keyscan`.
#[derive(Debug, Clone)]
pub struct HostKey {
    /// e.g. `ssh-ed25519`
    pub algorithm: String,
    /// e.g. `SHA256:AbCd...`
    pub fingerprint: String,
    /// Key size in bits when it can be derived from the blob.
    pub bits: Option<u32>,
    /// The base64 blob, so the whole key can be copied out.
    pub blob_b64: String,
}

/// Everything the probe managed to learn about one server.
#[derive(Debug, Clone, Default)]
pub struct SshInfo {
    pub host: String,
    pub port: u16,
    /// Address the name actually resolved to.
    pub ip: Option<String>,
    /// Raw identification string, e.g. `SSH-2.0-OpenSSH_9.6p1 Debian-4`.
    pub banner: String,
    /// `SSH-2.0` — the protocol version from the identification string.
    pub protocol: String,
    /// `OpenSSH_9.6p1`
    pub software: String,
    /// The trailing comment, e.g. `Debian-4`.
    pub comment: String,
    /// Any text the server printed before its identification string.
    pub preamble: Vec<String>,

    pub kex: Vec<String>,
    pub host_key_algorithms: Vec<String>,
    pub ciphers_c2s: Vec<String>,
    pub ciphers_s2c: Vec<String>,
    pub macs_c2s: Vec<String>,
    pub macs_s2c: Vec<String>,
    pub compression_c2s: Vec<String>,
    pub compression_s2c: Vec<String>,
    pub languages: Vec<String>,

    pub host_keys: Vec<HostKey>,
    /// Best-effort operating system clues drawn from the version string.
    pub os_clues: Vec<String>,
    /// Implementation quirks that help fingerprint the software.
    pub quirks: Vec<String>,
    /// Non-fatal notes (e.g. `ssh-keyscan` unavailable).
    pub notes: Vec<String>,
    /// Fatal error, if the probe could not run at all.
    pub error: Option<String>,
}

/// Probe lifecycle, shared with the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeState {
    /// No remote host (e.g. the local shell session).
    Idle,
    Running,
    Done,
}

/// Handle held by a session; the worker thread fills it in.
#[derive(Debug)]
pub struct ProbeHandle {
    pub state: Mutex<ProbeState>,
    pub info: Mutex<SshInfo>,
}

impl ProbeHandle {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ProbeState::Idle),
            info: Mutex::new(SshInfo::default()),
        })
    }

    pub fn state(&self) -> ProbeState {
        self.state.lock().map(|g| g.clone()).unwrap_or(ProbeState::Idle)
    }

    pub fn snapshot(&self) -> SshInfo {
        self.info.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Kicks off a probe on a background thread. Safe to call repeatedly; a
/// probe already in flight is left alone.
pub fn spawn(handle: Arc<ProbeHandle>, host: String, port: u16) {
    {
        let mut state = match handle.state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if *state == ProbeState::Running {
            return;
        }
        *state = ProbeState::Running;
    }
    std::thread::spawn(move || {
        let info = probe(&host, port);
        if let Ok(mut slot) = handle.info.lock() {
            *slot = info;
        }
        if let Ok(mut state) = handle.state.lock() {
            *state = ProbeState::Done;
        }
    });
}

/// Runs the full probe synchronously.
pub fn probe(host: &str, port: u16) -> SshInfo {
    let mut info = SshInfo {
        host: host.to_string(),
        port,
        ..Default::default()
    };

    let addr = match resolve(host, port) {
        Ok(a) => {
            info.ip = Some(a.ip().to_string());
            a
        }
        Err(e) => {
            info.error = Some(format!("Cannot resolve {host}: {e}"));
            return info;
        }
    };

    match transport_probe(addr, &mut info) {
        Ok(()) => {}
        Err(e) => info.error = Some(e),
    }

    derive_hints(&mut info);

    match keyscan(host, port) {
        Ok(keys) if !keys.is_empty() => info.host_keys = keys,
        Ok(_) => info
            .notes
            .push("ssh-keyscan returned no host keys for this address.".into()),
        Err(e) => info.notes.push(e),
    }

    info
}

fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "no addresses returned".to_string())
}

// ---------------------------------------------------------------------------
// Transport probe
// ---------------------------------------------------------------------------

fn transport_probe(addr: SocketAddr, info: &mut SshInfo) -> Result<(), String> {
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("Cannot connect to {addr}: {e}"))?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_nodelay(true).ok();

    stream
        .write_all(format!("{CLIENT_ID}\r\n").as_bytes())
        .map_err(|e| format!("Cannot send identification string: {e}"))?;
    stream.flush().ok();

    let mut r = Reader::new(stream);

    // RFC 4253 §4.2: the server may send any number of lines before its
    // identification string; only the line beginning with "SSH-" counts.
    let mut banner = None;
    for _ in 0..64 {
        let line = r.read_line()?;
        if line.starts_with("SSH-") {
            banner = Some(line);
            break;
        }
        if !line.trim().is_empty() {
            info.preamble.push(line);
        }
    }
    let banner = banner.ok_or_else(|| {
        "The server never sent an SSH identification string (is this an SSH port?)".to_string()
    })?;

    info.banner = banner.clone();
    let mut parts = banner.splitn(3, '-');
    let proto_prefix = parts.next().unwrap_or("SSH");
    let proto_version = parts.next().unwrap_or("");
    info.protocol = format!("{proto_prefix}-{proto_version}");
    let rest = parts.next().unwrap_or("");
    match rest.split_once(' ') {
        Some((sw, comment)) => {
            info.software = sw.to_string();
            info.comment = comment.trim().to_string();
        }
        None => info.software = rest.to_string(),
    }

    // Read binary packets until the KEXINIT turns up.
    for _ in 0..8 {
        let payload = r.read_packet()?;
        if payload.is_empty() {
            continue;
        }
        match payload[0] {
            20 => {
                parse_kexinit(&payload, info)?;
                return Ok(());
            }
            // SSH_MSG_DISCONNECT
            1 => {
                let mut c = Cursor::new(&payload[1..]);
                let code = c.u32().unwrap_or(0);
                let msg = c.string_utf8().unwrap_or_default();
                return Err(format!("Server disconnected (reason {code}): {msg}"));
            }
            _ => continue,
        }
    }
    Err("The server did not send a key-exchange init packet.".into())
}

fn parse_kexinit(payload: &[u8], info: &mut SshInfo) -> Result<(), String> {
    // byte SSH_MSG_KEXINIT, byte[16] cookie, then ten name-lists.
    if payload.len() < 17 {
        return Err("Truncated KEXINIT packet.".into());
    }
    let mut c = Cursor::new(&payload[17..]);
    let mut lists = Vec::with_capacity(10);
    for _ in 0..10 {
        lists.push(c.name_list().ok_or("Malformed KEXINIT name-list.")?);
    }
    info.kex = lists[0].clone();
    info.host_key_algorithms = lists[1].clone();
    info.ciphers_c2s = lists[2].clone();
    info.ciphers_s2c = lists[3].clone();
    info.macs_c2s = lists[4].clone();
    info.macs_s2c = lists[5].clone();
    info.compression_c2s = lists[6].clone();
    info.compression_s2c = lists[7].clone();
    let mut langs = lists[8].clone();
    for l in &lists[9] {
        if !langs.contains(l) {
            langs.push(l.clone());
        }
    }
    info.languages = langs;
    Ok(())
}

/// Buffered reader over the socket with just enough framing for the probe.
struct Reader {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Reader {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::with_capacity(4096),
        }
    }

    fn fill(&mut self, want: usize) -> Result<(), String> {
        let mut chunk = [0u8; 4096];
        while self.buf.len() < want {
            let n = self
                .stream
                .read(&mut chunk)
                .map_err(|e| format!("Read failed: {e}"))?;
            if n == 0 {
                return Err("The server closed the connection.".into());
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
        Ok(())
    }

    fn read_line(&mut self) -> Result<String, String> {
        loop {
            if let Some(idx) = self.buf.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.buf.drain(..=idx).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.len() > 4096 {
                    return Err("Identification line too long.".into());
                }
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            if self.buf.len() > 64 * 1024 {
                return Err("The server sent no identification string.".into());
            }
            let before = self.buf.len();
            self.fill(before + 1)?;
        }
    }

    /// Reads one unencrypted binary packet and returns its payload.
    fn read_packet(&mut self) -> Result<Vec<u8>, String> {
        self.fill(4)?;
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if !(2..=256 * 1024).contains(&len) {
            return Err(format!("Implausible SSH packet length ({len})."));
        }
        self.fill(4 + len)?;
        let padding = self.buf[4] as usize;
        if padding + 1 > len {
            return Err("Malformed SSH packet padding.".into());
        }
        let payload = self.buf[5..4 + len - padding].to_vec();
        self.buf.drain(..4 + len);
        Ok(payload)
    }
}

/// Minimal reader for the SSH wire encoding (RFC 4251 §5).
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u32(&mut self) -> Option<u32> {
        let end = self.pos.checked_add(4)?;
        if end > self.data.len() {
            return None;
        }
        let v = u32::from_be_bytes(self.data[self.pos..end].try_into().ok()?);
        self.pos = end;
        Some(v)
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.u32()? as usize;
        let end = self.pos.checked_add(len)?;
        if end > self.data.len() {
            return None;
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Some(out)
    }

    fn string_utf8(&mut self) -> Option<String> {
        Some(String::from_utf8_lossy(self.bytes()?).into_owned())
    }

    fn name_list(&mut self) -> Option<Vec<String>> {
        let raw = self.string_utf8()?;
        Some(
            raw.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
        )
    }
}

// ---------------------------------------------------------------------------
// Host keys
// ---------------------------------------------------------------------------

fn keyscan(host: &str, port: u16) -> Result<Vec<HostKey>, String> {
    let output = std::process::Command::new("ssh-keyscan")
        .args(["-T", "5", "-p", &port.to_string(), host])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| {
            format!("ssh-keyscan is unavailable ({e}); install openssh-client for host keys.")
        })?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let _host = fields.next();
        let (Some(alg), Some(blob)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(blob) else {
            continue;
        };
        let digest = Sha256::digest(&raw);
        let fingerprint = format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
        );
        keys.push(HostKey {
            algorithm: alg.to_string(),
            fingerprint,
            bits: key_bits(&raw),
            blob_b64: blob.to_string(),
        });
    }
    keys.sort_by(|a, b| a.algorithm.cmp(&b.algorithm));
    keys.dedup_by(|a, b| a.fingerprint == b.fingerprint);
    Ok(keys)
}

/// Derives the key size from a public key blob where it is well defined.
fn key_bits(blob: &[u8]) -> Option<u32> {
    let mut c = Cursor::new(blob);
    let kind = c.string_utf8()?;
    match kind.as_str() {
        "ssh-ed25519" => Some(256),
        "ssh-rsa" | "rsa-sha2-256" | "rsa-sha2-512" => {
            let _e = c.bytes()?;
            let n = c.bytes()?;
            // Skip the leading zero byte an mpint uses to stay positive.
            let n = n.strip_prefix(&[0u8]).unwrap_or(n);
            let lead = n.first().copied()?;
            Some((n.len() as u32 - 1) * 8 + (8 - lead.leading_zeros()))
        }
        "ssh-dss" => Some(1024),
        k if k.starts_with("ecdsa-sha2-") => match k.rsplit('-').next() {
            Some("nistp256") => Some(256),
            Some("nistp384") => Some(384),
            Some("nistp521") => Some(521),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Heuristics
// ---------------------------------------------------------------------------

/// Turns the identification string and algorithm lists into readable
/// OS clues and implementation quirks. Everything here is a heuristic and
/// is labelled as such in the sidebar.
fn derive_hints(info: &mut SshInfo) {
    let sw = info.software.clone();
    let comment = info.comment.clone();
    let joined = format!("{sw} {comment}");
    let lower = joined.to_lowercase();

    if sw.starts_with("OpenSSH") {
        if sw.contains('p') {
            info.os_clues.push(
                "OpenSSH \"portable\" build (the p suffix) — a Unix-like host, not OpenBSD."
                    .into(),
            );
        } else if !sw.is_empty() {
            info.os_clues
                .push("OpenSSH without a portable suffix — typically OpenBSD.".into());
        }
    }
    for (needle, clue) in [
        ("debian", "Version comment names Debian — Debian or a derivative."),
        ("ubuntu", "Version comment names Ubuntu."),
        ("freebsd", "Version comment names FreeBSD."),
        ("raspbian", "Version comment names Raspbian."),
        ("fips", "FIPS-validated build."),
        ("windows", "Identifies as an OpenSSH for Windows build."),
        ("mikrotik", "MikroTik RouterOS."),
        ("cisco", "Cisco network device."),
    ] {
        if lower.contains(needle) {
            info.os_clues.push(clue.to_string());
        }
    }
    if sw.starts_with("dropbear") {
        info.os_clues
            .push("Dropbear — common on embedded Linux, OpenWrt and busybox systems.".into());
    }
    if lower.contains("libssh") {
        info.os_clues
            .push("libssh-based server rather than OpenSSH.".into());
    }
    if info.comment.is_empty() && sw.starts_with("OpenSSH") {
        info.os_clues.push(
            "No distribution comment in the version string — often an upstream or hardened build."
                .into(),
        );
    }
    if info.os_clues.is_empty() && !info.banner.is_empty() {
        info.os_clues
            .push("No recognisable OS marker in the version string.".into());
    }

    let has = |list: &[String], needle: &str| list.iter().any(|s| s == needle);
    let any = |list: &[String], needle: &str| list.iter().any(|s| s.contains(needle));

    if any(&info.kex, "sntrup761x25519") {
        info.quirks.push(
            "Offers sntrup761x25519 post-quantum key exchange — OpenSSH 8.5 or newer.".into(),
        );
    }
    if has(&info.ciphers_s2c, "chacha20-poly1305@openssh.com") {
        info.quirks
            .push("Offers chacha20-poly1305@openssh.com — OpenSSH 6.5 or newer.".into());
    }
    if !info.host_key_algorithms.is_empty() && !has(&info.host_key_algorithms, "ssh-rsa") {
        info.quirks.push(
            "SHA-1 ssh-rsa host keys are not offered — matches OpenSSH 8.8+ defaults.".into(),
        );
    }
    if has(&info.kex, "ext-info-s") || has(&info.kex, "ext-info-c") {
        info.quirks
            .push("Advertises RFC 8308 extension negotiation (ext-info).".into());
    }
    if any(&info.kex, "kex-strict-s-v00@openssh.com") {
        info.quirks
            .push("Advertises strict KEX — OpenSSH 9.6 or newer (Terrapin mitigation).".into());
    }
    if any(&info.ciphers_s2c, "cbc") {
        info.quirks
            .push("Still offers CBC-mode ciphers — an older or deliberately permissive config."
                .into());
    }
    if has(&info.compression_s2c, "zlib@openssh.com") {
        info.quirks
            .push("Supports delayed compression (zlib@openssh.com).".into());
    }
    if info.compression_s2c.len() == 1 && has(&info.compression_s2c, "none") {
        info.quirks.push("Compression is disabled server-side.".into());
    }
    if !info.preamble.is_empty() {
        info.quirks
            .push("The server prints a pre-authentication banner.".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the payload of an `SSH_MSG_KEXINIT` packet from ten name-lists.
    fn kexinit_payload(lists: &[&str; 10]) -> Vec<u8> {
        let mut out = vec![20u8];
        out.extend_from_slice(&[0u8; 16]); // cookie
        for list in lists {
            out.extend_from_slice(&(list.len() as u32).to_be_bytes());
            out.extend_from_slice(list.as_bytes());
        }
        out.push(0); // first_kex_packet_follows
        out.extend_from_slice(&0u32.to_be_bytes()); // reserved
        out
    }

    #[test]
    fn parses_every_kexinit_name_list() {
        let payload = kexinit_payload(&[
            "curve25519-sha256,ext-info-s",
            "ssh-ed25519,rsa-sha2-512",
            "aes128-ctr",
            "chacha20-poly1305@openssh.com,aes256-gcm@openssh.com",
            "hmac-sha1",
            "hmac-sha2-256-etm@openssh.com",
            "none",
            "none,zlib@openssh.com",
            "",
            "en-US",
        ]);
        let mut info = SshInfo::default();
        parse_kexinit(&payload, &mut info).unwrap();

        assert_eq!(info.kex, ["curve25519-sha256", "ext-info-s"]);
        assert_eq!(info.host_key_algorithms, ["ssh-ed25519", "rsa-sha2-512"]);
        assert_eq!(info.ciphers_c2s, ["aes128-ctr"]);
        assert_eq!(
            info.ciphers_s2c,
            ["chacha20-poly1305@openssh.com", "aes256-gcm@openssh.com"]
        );
        assert_eq!(info.macs_c2s, ["hmac-sha1"]);
        assert_eq!(info.macs_s2c, ["hmac-sha2-256-etm@openssh.com"]);
        assert_eq!(info.compression_c2s, ["none"]);
        assert_eq!(info.compression_s2c, ["none", "zlib@openssh.com"]);
        assert_eq!(info.languages, ["en-US"]);
    }

    #[test]
    fn rejects_a_truncated_kexinit() {
        assert!(parse_kexinit(&[20u8; 4], &mut SshInfo::default()).is_err());
        let mut short = vec![20u8];
        short.extend_from_slice(&[0u8; 16]);
        short.extend_from_slice(&[0, 0, 0, 40]); // claims 40 bytes that follow
        assert!(parse_kexinit(&short, &mut SshInfo::default()).is_err());
    }

    #[test]
    fn derives_key_sizes_from_blobs() {
        // ssh-ed25519 blob: string "ssh-ed25519", string <32 bytes>.
        let mut blob = Vec::new();
        blob.extend_from_slice(&11u32.to_be_bytes());
        blob.extend_from_slice(b"ssh-ed25519");
        blob.extend_from_slice(&32u32.to_be_bytes());
        blob.extend_from_slice(&[7u8; 32]);
        assert_eq!(key_bits(&blob), Some(256));

        // ssh-rsa blob: string "ssh-rsa", mpint e, mpint n.
        let mut blob = Vec::new();
        blob.extend_from_slice(&7u32.to_be_bytes());
        blob.extend_from_slice(b"ssh-rsa");
        blob.extend_from_slice(&3u32.to_be_bytes());
        blob.extend_from_slice(&[0x01, 0x00, 0x01]);
        // A 2048-bit modulus is 256 bytes, preceded by the mpint sign byte.
        let mut modulus = vec![0x00, 0x80];
        modulus.extend_from_slice(&[0xAB; 255]);
        blob.extend_from_slice(&(modulus.len() as u32).to_be_bytes());
        blob.extend_from_slice(&modulus);
        assert_eq!(key_bits(&blob), Some(2048));

        for (curve, bits) in [("nistp256", 256), ("nistp384", 384), ("nistp521", 521)] {
            let name = format!("ecdsa-sha2-{curve}");
            let mut blob = Vec::new();
            blob.extend_from_slice(&(name.len() as u32).to_be_bytes());
            blob.extend_from_slice(name.as_bytes());
            assert_eq!(key_bits(&blob), Some(bits));
        }

        assert_eq!(key_bits(b"not a blob"), None);
    }

    #[test]
    fn draws_os_clues_and_quirks_from_the_version_string() {
        let mut info = SshInfo {
            banner: "SSH-2.0-OpenSSH_9.6p1 Ubuntu-3ubuntu13".into(),
            software: "OpenSSH_9.6p1".into(),
            comment: "Ubuntu-3ubuntu13".into(),
            kex: vec![
                "sntrup761x25519-sha512@openssh.com".into(),
                "kex-strict-s-v00@openssh.com".into(),
            ],
            host_key_algorithms: vec!["ssh-ed25519".into()],
            ciphers_s2c: vec!["chacha20-poly1305@openssh.com".into()],
            compression_s2c: vec!["none".into(), "zlib@openssh.com".into()],
            ..Default::default()
        };
        derive_hints(&mut info);

        assert!(info.os_clues.iter().any(|c| c.contains("portable")));
        assert!(info.os_clues.iter().any(|c| c.contains("Ubuntu")));
        assert!(info.quirks.iter().any(|q| q.contains("sntrup761x25519")));
        assert!(info.quirks.iter().any(|q| q.contains("strict KEX")));
        assert!(info.quirks.iter().any(|q| q.contains("ssh-rsa")));
        assert!(info.quirks.iter().any(|q| q.contains("delayed compression")));

        // A Dropbear server is recognised as embedded rather than OpenBSD.
        let mut info = SshInfo {
            software: "dropbear_2022.83".into(),
            banner: "SSH-2.0-dropbear_2022.83".into(),
            ..Default::default()
        };
        derive_hints(&mut info);
        assert!(info.os_clues.iter().any(|c| c.contains("Dropbear")));
    }

    #[test]
    fn reads_the_ssh_wire_encoding() {
        let mut data = Vec::new();
        data.extend_from_slice(&5u32.to_be_bytes());
        data.extend_from_slice(b"a,b,c");
        data.extend_from_slice(&0u32.to_be_bytes());
        let mut c = Cursor::new(&data);
        assert_eq!(c.name_list().unwrap(), ["a", "b", "c"]);
        assert_eq!(c.name_list().unwrap(), Vec::<String>::new());
        assert!(c.name_list().is_none(), "reading past the end must fail");

        // A length that runs off the end is rejected rather than panicking.
        let mut c = Cursor::new(&[0, 0, 0, 200, 1, 2, 3]);
        assert!(c.bytes().is_none());
    }
}
