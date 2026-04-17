//! # Updates feature
//!
//! Application update checking. Queries GitHub Releases for newer
//! versions and returns version info / update notifications.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::updates::dto` — `UpdateInfoDto`, `VersionInfoDto`
//! - `crate::features::updates::use_cases` — `CheckForUpdatesUseCase`,
//!   `GetCurrentVersionUseCase`
//! - `crate::features::updates::adapter::UpdateCheckerAdapter` —
//!   GitHub Releases impl of `UpdateCheckerPort`
//! - `crate::features::updates::commands` — Tauri command impls
//! - `crate::features::updates::plugin::init()` — Tauri plugin
//!
//! `UpdateCheckerPort` stays in `application/ports/`.

pub mod adapter;
pub mod commands;
pub mod dto;
pub mod plugin;
pub mod use_cases;
