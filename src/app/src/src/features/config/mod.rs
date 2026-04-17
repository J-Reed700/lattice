//! # Config feature
//!
//! Tauri plugin for app configuration (get/save config, manage watch
//! folders). Thin feature — plugin contains all the logic.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::config::plugin::init()` — Tauri plugin
//!   (directory-shaped, with `commands` submodule)
//!
//! No use cases / DTOs / ports — direct plugin implementation.

pub mod plugin;
