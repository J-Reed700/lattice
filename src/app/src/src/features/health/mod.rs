//! # Health feature
//!
//! System health check + version info + database initialization.
//!
//! ## File layout
//!
//! | File            | Canonical module path                                       |
//! |-----------------|-------------------------------------------------------------|
//! | `dto.rs`        | `crate::application::dtos::health_dto`                      |
//! | `use_cases/`    | `crate::application::use_cases::health`                     |
//! | `commands.rs`   | `crate::interfaces::commands::health_commands` (aka `health`)|
//! | `plugin/`       | `crate::plugins::health` (directory plugin with commands + types) |
//!
//! First migration to include a *directory-shaped* plugin (as opposed
//! to the single-file `<name>_plugin.rs` pattern). The plugin keeps
//! its internal structure (`plugin/commands.rs`, `plugin/types.rs`)
//! and is wired through a single `#[path]` redirect on `plugins::health`.
//!
//! No ports, repositories, or traits — health is a thin diagnostics
//! feature.
