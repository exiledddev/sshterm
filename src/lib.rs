//! SSCL — Secure Shell Command Line.
//!
//! A terminal application for opening and remembering SSH connections.
//! The crate is split into a library so that the GUI binary and the
//! `sscl-probe` command-line helper share exactly the same code.

pub mod app;
pub mod icons;
pub mod sshinfo;
pub mod store;
pub mod term;
pub mod theme;
pub mod ui;
