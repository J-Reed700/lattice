//! # Credentials feature
//!
//! API key / credential storage via the OS keyring. Set / get / delete
//! API keys for LLM providers, plus custom endpoint configuration.
//!
//! ## File layout
//!
//! | File             | Canonical module path                                         |
//! |------------------|---------------------------------------------------------------|
//! | `dto.rs`         | `crate::application::dtos::credential_dto`                    |
//! | `use_cases/`     | `crate::application::use_cases::credentials`                  |
//! | `adapter.rs`     | `crate::infrastructure::security::credentials_adapter`        |
//! | `commands.rs`    | `crate::interfaces::commands::credentials` (aka `credentials_commands`) |
//! | `plugin/`        | `crate::plugins::credentials` (directory plugin)              |
//!
//! `CredentialsPort` stays in `application/ports/`. Keyring primitives
//! (keyring_storage, migration) remain in `infrastructure/security/` —
//! those are shared security infrastructure used by multiple features.
