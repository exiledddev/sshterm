//! Censoring of connection details.
//!
//! SSCL puts host names, addresses, user names and key file names on screen in
//! several places at once. That is exactly what you want while working and
//! exactly what you do not want while screen-sharing or filing a screenshot in
//! a bug report, so a single toggle masks all of it.
//!
//! Only the chrome is masked — the terminal shows whatever the remote host
//! sends, and SSCL does not rewrite that.

use std::sync::atomic::{AtomicBool, Ordering};

static CENSORED: AtomicBool = AtomicBool::new(false);

pub fn is_censored() -> bool {
    CENSORED.load(Ordering::Relaxed)
}

pub fn set_censored(on: bool) {
    CENSORED.store(on, Ordering::Relaxed);
}

pub fn toggle() {
    CENSORED.fetch_xor(true, Ordering::Relaxed);
}

/// Masks `text` when censoring is on, keeping its shape so the layout does
/// not jump: letters and digits become bullets, punctuation stays.
pub fn mask(text: &str) -> String {
    if !is_censored() {
        return text.to_string();
    }
    text.chars()
        .map(|c| if c.is_alphanumeric() { '•' } else { c })
        .collect()
}

/// Masks a whole value to a fixed width, for things whose length is itself a
/// hint — a fingerprint, say.
pub fn mask_opaque(text: &str) -> String {
    if !is_censored() {
        return text.to_string();
    }
    "•".repeat(text.chars().count().clamp(8, 24))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_keeps_the_shape_but_not_the_content() {
        set_censored(false);
        assert_eq!(mask("ubuntu@130.61.144.27"), "ubuntu@130.61.144.27");

        set_censored(true);
        assert_eq!(mask("ubuntu@130.61.144.27"), "••••••@•••.••.•••.••");
        assert_eq!(mask("ssh-key-2026-09-07.key"), "•••-•••-••••-••-••.•••");
        // Nothing of the original survives.
        assert!(!mask("secret-host").contains('s'));

        assert_eq!(mask_opaque("SHA256:abc").chars().count(), 10);
        assert_eq!(mask_opaque("x").chars().count(), 8, "short values still hide");

        set_censored(false);
        assert_eq!(mask("plain"), "plain");
    }

    #[test]
    fn toggling_flips_the_flag() {
        set_censored(false);
        toggle();
        assert!(is_censored());
        toggle();
        assert!(!is_censored());
    }
}
