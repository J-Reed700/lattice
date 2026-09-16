//! Backup, restore, export and import command handlers.
//!
//! This module is a façade: every handler lives in a submodule grouped by
//! responsibility, and the whole command surface is re-exported here so
//! `crate::features::backup::commands::<name>` keeps resolving.
//!
//! - [`create`] / [`restore`] / [`list`] — the backup lifecycle
//! - [`archive`] — the encrypted off-device archive
//! - [`export`] / [`csv_export`] / [`html_export`] — outbound formats
//! - [`import`] — inbound third-party formats
//! - [`auto_backup`] — the scheduled-backup switch

mod archive;
mod auto_backup;
mod create;
mod csv_export;
mod export;
mod html_export;
mod import;
mod list;
mod restore;

pub use archive::{
    cancelled_restore, create_archive_impl, restore_api_error, restore_archive_impl,
    restore_attempt_is_finished,
};
pub use auto_backup::{start_auto_backup, stop_auto_backup};
pub use create::{create_backup, create_backup_impl};
pub use csv_export::{export_csv, export_csv_impl};
pub use export::{export_json, export_json_impl, export_markdown, export_markdown_impl};
pub use html_export::{export_html, export_html_impl};
pub use import::{import_notion_export, import_obsidian_vault, import_roam_json};
pub use list::{list_backups, list_backups_impl, BackupInfo};
pub use restore::{restore_backup, restore_backup_impl};
