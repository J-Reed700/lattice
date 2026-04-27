//! # Daily notes feature
//!
//! Daily journal workspace management — creating per-day entries in
//! a designated workspace folder. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::daily_notes::commands` — Tauri command handlers
//! - `crate::features::daily_notes::plugin::init()` — Tauri plugin
//!
//! Thin feature — all logic lives in the command file. No DTO,
//! use case, port, or repository.

pub mod commands;
pub mod plugin;
