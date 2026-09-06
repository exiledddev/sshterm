//! Live probe test.
//!
//! Runs only when `SSCL_TEST_SSH_HOST` / `SSCL_TEST_SSH_PORT` point at a
//! reachable SSH server, so the suite stays green on machines without one.

use std::process::Command;

#[test]
fn probes_a_live_server() {
    let Ok(host) = std::env::var("SSCL_TEST_SSH_HOST") else {
        eprintln!("skipping: SSCL_TEST_SSH_HOST not set");
        return;
    };
    let port: u16 = std::env::var("SSCL_TEST_SSH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);

    // The probe lives in the binary crate, so drive it through the helper
    // binary that the crate exposes for testing.
    let out = Command::new(env!("CARGO_BIN_EXE_sscl-probe"))
        .args([&host, &port.to_string()])
        .output()
        .expect("probe helper should run");
    let text = String::from_utf8_lossy(&out.stdout);
    eprintln!("{text}");

    assert!(out.status.success(), "probe helper failed: {:?}", out.status);
    assert!(text.contains("banner: SSH-2.0"), "no SSH banner in output");
    assert!(text.contains("kex["), "no key-exchange algorithms parsed");
    assert!(text.contains("SHA256:"), "no host-key fingerprint computed");
}
