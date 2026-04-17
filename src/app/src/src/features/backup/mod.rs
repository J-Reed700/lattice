//! # Backup feature
//!
//! Database + vault backup and restore, with optional auto-backup
//! scheduler. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::backup::dto` — backup DTOs (BackupInfo, etc.)
//! - `crate::features::backup::use_cases` — backup/restore use cases
//! - `crate::features::backup::adapter::BackupAdapter` — SQLite impl
//!   of `BackupPort`
//! - `crate::features::backup::scheduler::BackupScheduler` — impl of
//!   `BackupSchedulerPort`
//! - `crate::features::backup::commands` — Tauri command handlers
//! - `crate::features::backup::plugin::init()` — Tauri plugin
//!
//! `BackupPort` and `BackupSchedulerPort` stay in `application/ports/`.

pub mod adapter;
pub mod commands;
pub mod dto;
pub mod plugin;
pub mod scheduler;
pub mod use_cases;
