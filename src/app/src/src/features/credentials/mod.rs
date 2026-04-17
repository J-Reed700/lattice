//! # Credentials feature
//!
//! API key / credential storage via the OS keyring. Set / get / delete
//! API keys for LLM providers, plus custom endpoint configuration.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::credentials::dto` — credential DTOs
//! - `crate::features::credentials::use_cases` — CRUD use cases
//! - `crate::features::credentials::adapter::CredentialsAdapter` —
//!   OS keyring impl of `CredentialsPort`
//! - `crate::features::credentials::commands` — Tauri command impls
//! - `crate::features::credentials::plugin::init()` — Tauri plugin
//!   (directory-shaped)
//!
//! `CredentialsPort` stays in `application/ports/`. Keyring primitives
//! (keyring_storage, migration) remain in `infrastructure/security/` —
//! those are shared security infrastructure used by multiple features.

pub mod adapter;
pub mod commands;
pub mod dto;
pub mod plugin;
pub mod use_cases;
