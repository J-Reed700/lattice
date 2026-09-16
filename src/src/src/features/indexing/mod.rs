//! # Indexing feature
//!
//! Document indexing pipeline: content extraction (PDF, DOCX, HTML,
//! …), semantic chunking, metadata extraction, and storage of chunks
//! and embeddings. The actor-based service in `engine/actor.rs` is the
//! primary worker driving the pipeline.
//!
//! ## Public surface
//!
//! - `crate::features::indexing::dto`
//! - `crate::features::indexing::mapper::IndexingMapper`
//! - `crate::features::indexing::outcome` — IndexingOutcome value object
//! - `crate::features::indexing::use_cases` — index/reindex/delete use cases
//! - `crate::features::indexing::commands` — Tauri command handlers
//!
//! ## Engine
//!
//! - `crate::features::indexing::engine` — the indexing pipeline, owned
//!   by this feature and consumed by file, batch, web, conversation, etc.
//!
//! Public traits: `crate::features::indexing::{IndexingServiceTrait, IndexStorageTrait}`.
//!
//! No port or plugin — indexing flows through other plugins.

pub mod commands;
pub mod di;
pub mod dto;
pub mod mapper;
pub mod outcome;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use trait_def::{IndexStorageTrait, IndexingServiceTrait};
pub mod engine;
