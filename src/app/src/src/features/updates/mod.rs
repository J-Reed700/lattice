//! # Updates feature
//!
//! Application update checking. Queries GitHub Releases for newer versions
//! and returns version info / update notifications.
//!
//! ## File layout
//!
//! All files in this directory are loaded via `#[path]` redirects from their
//! legacy module locations so existing imports keep working (Strangler Fig).
//! Each file has exactly ONE canonical module path during migration:
//!
//! | File                         | Canonical module path                               |
//! |------------------------------|-----------------------------------------------------|
//! | `dto.rs`                     | `crate::application::dtos::update_dto`              |
//! | `use_cases/`                 | `crate::application::use_cases::updates`            |
//! | `adapter.rs`                 | `crate::infrastructure::updates::update_checker_adapter` |
//! | `commands.rs`                | `crate::interfaces::commands::updates_commands`     |
//! | `plugin.rs`                  | `crate::plugins::updates_plugin`                    |
//!
//! The port trait (`UpdateCheckerPort`) stays in `application/ports/` — features
//! consume shared ports, they don't own them.
//!
//! This `features/updates/mod.rs` exists only so the directory is recognized
//! by `cargo` as a module; it intentionally does not declare submodules to
//! avoid loading each file under two module paths and duplicating types.
