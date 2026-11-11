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
//!
//! `CredentialsPort` stays in `application/ports/`. Keyring primitives
//! (keyring_storage) remain in `infrastructure/security/` — shared security
//! infrastructure used by multiple features.

pub mod adapter;
pub mod di;
pub mod dto;
pub mod keys;
#[cfg(test)]
pub(crate) mod memory;
pub mod remote_auth;
pub mod use_cases;
