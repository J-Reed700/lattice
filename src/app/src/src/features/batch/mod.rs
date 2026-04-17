//! # Batch feature
//!
//! Batch document import (file and URL) with job tracking, progress
//! monitoring, cancellation, retry of failed items, and history.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::batch::dto` — request/response DTOs
//! - `crate::features::batch::use_cases` — start/cancel/retry/history
//! - `crate::features::batch::services::{file_import, url_import}` —
//!   `BatchFileImportService`, `BatchUrlImportService`
//! - `crate::features::batch::commands` — Tauri command handlers
//!   (file_import, url_import, history submodules)
//! - `crate::features::batch::plugin::init()` — Tauri plugin
//!
//! `BatchJobRepositoryPort` stays in `application/ports/`. The two
//! service traits (`BatchFileImportServiceTrait`,
//! `BatchUrlImportServiceTrait`) remain loaded via the shared
//! `infrastructure::services::traits` aggregator; consumers import
//! through that aggregator.

pub mod commands;
pub mod dto;
pub mod plugin;
pub mod services;
pub mod use_cases;
