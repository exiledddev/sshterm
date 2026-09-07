//! ACLI — Amplified Command Line Interface.
//!
//! A terminal that splits: every pane is a real pseudo-terminal running your
//! shell, arranged on one sheet of glass. No SSH, no session manager, no
//! splash — it opens and it is ready.

pub mod app;
pub mod icons;
pub mod layout;
pub mod paths;
pub mod platform;
pub mod prompt;
pub mod term;
pub mod theme;
pub mod ui;
