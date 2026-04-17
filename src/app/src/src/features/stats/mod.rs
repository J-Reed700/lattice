//! # Stats feature
//!
//! Aggregate system statistics (document counts, chunk counts, tag
//! counts, storage size). Consumed primarily by the health feature's
//! `get_system_stats` Tauri command.
//!
//! ## File layout
//!
//! | File                  | Canonical module path                                           |
//! |-----------------------|-----------------------------------------------------------------|
//! | `use_cases/`          | `crate::application::use_cases::stats`                          |
//! | `database_stats.rs`   | `crate::infrastructure::persistence::database::stats` (DatabaseStatsAdapter) |
//!
//! `DatabaseStatsPort` stays in `application/ports/`.
//!
//! No DTO, no plugin, no commands — the use case is invoked directly
//! from the health plugin's commands.
