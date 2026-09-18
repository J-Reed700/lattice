//! # Search feature
//!
//! Hybrid retrieval combining vector (USearch HNSW), keyword (SQLite
//! FTS5 BM25), filename, and recency search — with reciprocal-rank
//! fusion, optional reranking, and query expansion.
//!
//! ## Public surface
//!
//! - `crate::features::search::dto` — search DTOs
//! - `crate::features::search::mapper::SearchMapper`
//! - `crate::features::search::use_cases` — hybrid/semantic/file/recency
//! - `crate::features::search::commands` — Tauri command handlers
//! - `crate::features::search::plugin::init()` — Tauri plugin (directory-shaped)
//!
//! - `crate::features::search::engine` — retrieval engine (USearch +
//!   BM25 + hybrid/fusion/reranker/query_expansion/etc.)
//!   consumed by qa, mentions, indexing, conversation chat retrieval
//! - `crate::features::search::repository_tx` — transactional search
//!   repository adapter
//! - `crate::features::search::enrichment_service`
//!
//! Domain types used by the application layer live in the domain layer:
//! `crate::domain::entities::SearchResult`,
//! `crate::domain::value_objects::{SearchMode, SearchQuery}`,
//! `crate::domain::repositories::SearchRepository`, and
//! `crate::domain::services::SearchRankingService`.
//!
//! Public traits: `crate::features::search::{SearchServiceTrait, BM25SearchTrait, HybridSearchTrait}`.

pub mod commands;
pub mod di;
pub mod dto;
pub mod mapper;
pub mod plugin;
pub mod reranker_setup;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use trait_def::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait, SparseSearchTrait};
pub mod engine;
pub mod enrichment_service;
pub mod repository_tx;
