//! # Batch feature
//!
//! Imports of many files or URLs at once. Each import is one job on the
//! shared job runtime, which owns its status, progress, cancellation, retry
//! and restart recovery; the import keeps one row per file or URL, so a
//! stopped import resumes from the items it had not finished.
//!
//! ## Public surface
//!
//! - `dto` — request/response DTOs
//! - `imports::BatchImports` — start, read, cancel, retry and delete imports
//! - `plugin::init()` — the Tauri plugin, which registers the import job kinds

pub mod dto;
mod file_job;
pub mod imports;
pub mod items;
pub mod plugin;
mod url_job;
mod worker;

#[cfg(test)]
mod tests;
