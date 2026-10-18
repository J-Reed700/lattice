//! # Stats feature
//!
//! Aggregate system statistics (document counts, chunk counts, tag
//! counts, storage size). Consumed primarily by the health feature's
//! `get_system_stats` Tauri command. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::stats::use_cases::GetSystemStatsUseCase`
//! - `crate::features::stats::use_cases::GetCorpusShapeUseCase`
//! - `crate::features::stats::database_stats::DatabaseStatsAdapter`
//! - `crate::features::stats::type_label::type_label`
//!
//! `DatabaseStatsPort` stays in `application/ports/`. No DTO, no
//! plugin, no commands — the use case is invoked directly from the
//! health plugin's commands.

pub mod corpus_shape_repository;
pub mod database_stats;
pub mod di;
pub mod type_label;
pub mod use_cases;
