//! Interfaces layer: Tauri IPC command handlers, event processors, and DI wiring.

pub mod commands;
pub mod di;
pub mod dto;
pub mod settings_side_effects;

pub use commands::*;
pub use di::Container;
