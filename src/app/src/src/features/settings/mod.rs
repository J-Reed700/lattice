//! # Settings feature
//!
//! Application settings: get / update / reset / import / export / validate.
//! The plugin also handles direct HTTP request proxying for frontend
//! (OpenAI-compatible model listing). Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::settings::dto` — settings DTOs (directory-shaped)
//! - `crate::features::settings::mapper` — SettingsMapper
//! - `crate::features::settings::use_cases` — settings use cases
//! - `crate::features::settings::repository::SettingsRepository`
//! - `crate::features::settings::plugin::init()` — Tauri plugin (also
//!   hosts command impls directly)
//!
//! `SettingsRepositoryPort` stays in `application/ports/`.
//!
//! `interfaces/commands/domains/hf_settings.rs` is NOT part of this
//! feature — it's huggingface's concern.

pub mod di;
pub mod dto;
pub mod mapper;
pub mod plugin;
pub mod repository;
pub mod use_cases;
