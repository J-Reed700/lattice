//! # Backup feature
//!
//! Database + vault backup and restore, with optional auto-backup scheduler.
//!
//! ## File layout
//!
//! All files are loaded via `#[path]` redirects from their legacy module
//! locations so existing imports keep working (Strangler Fig). Each file
//! has exactly ONE canonical module path during migration:
//!
//! | File               | Canonical module path                                       |
//! |--------------------|-------------------------------------------------------------|
//! | `dto.rs`           | `crate::application::dtos::backup_dto`                      |
//! | `use_cases/`       | `crate::application::use_cases::backup`                     |
//! | `adapter.rs`       | `crate::infrastructure::persistence::backup_adapter`        |
//! | `scheduler.rs`     | `crate::infrastructure::services::backup_scheduler`         |
//! | `commands.rs`      | `crate::interfaces::commands::backup` (aka `backup_commands`)|
//! | `plugin.rs`        | `crate::plugins::backup_plugin`                             |
//!
//! The ports (`BackupPort`, `BackupSchedulerPort`) stay in
//! `application/ports/` — features consume ports, they don't own them.
//! (These ports are only used by backup code today, so they could
//! graduate into the feature in a later pass if desired.)
//!
//! This module intentionally declares no submodules to avoid loading
//! files under two module paths.
