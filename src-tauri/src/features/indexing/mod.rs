//! # Indexing feature
//!
//! Document indexing pipeline: content extraction (PDF, DOCX, HTML,
//! …), semantic chunking, metadata extraction, and storage of chunks
//! and embeddings. `use_cases/index_file.rs` drives the pipeline; the
//! modules under `engine/` are the pieces it composes.
//!
//! ## Public surface
//!
//! - `crate::features::indexing::dto`
//! - `crate::features::indexing::mapper::IndexingMapper`
//! - `crate::features::indexing::outcome` — IndexingOutcome value object
//! - `crate::features::indexing::use_cases` — index/reindex/delete use cases
//! - `crate::features::indexing::library_gc` — LibraryGc, the one owner of
//!   blob deletion in `~/.lattice/files`
//! - `crate::features::indexing::commands` — Tauri command handlers
//!
//! ## Engine
//!
//! - `crate::features::indexing::engine` — the indexing pipeline, owned
//!   by this feature and consumed by file, batch, web, conversation, etc.
//!
//! Public trait: `crate::features::indexing::IndexStorageTrait`.
//!
//! No port or plugin — indexing flows through other plugins.

pub mod commands;
pub mod di;
pub mod dto;
pub mod library_gc;
pub mod mapper;
pub mod outcome;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use library_gc::{LibraryGc, SweepReport};
pub use trait_def::IndexStorageTrait;
pub mod engine;
