//! The shell. It owns the tray icon, the panel, the full window and the
//! settings, and talks to tools only through the [`tool::Tool`] trait.

pub mod editor;
pub mod gallery;
pub mod layout;
pub mod panel;
pub mod settings;
pub mod shell;
pub mod tool;
pub mod window;
