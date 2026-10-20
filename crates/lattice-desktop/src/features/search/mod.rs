//! # Search feature
//!
//! Hybrid retrieval combining vector (USearch HNSW), keyword (SQLite
//! FTS5 BM25), filename, and recency search — with weighted reciprocal-rank
//! fusion and optional reranking.
//!
//! `lattice_core::application::contracts::search::ReciprocalRankFusion` is
//! the one rank-fusion implementation; every path that fuses ranked lists
//! goes through it, the folder index and memory recall included.
//!
//! ## Public surface
//!
//! - `crate::features::search::mapper::SearchMapper`
//! - `crate::features::search::use_cases` — hybrid/semantic/file/recency
//! - `crate::features::search::commands` — Tauri command handlers
//! - `crate::features::search::query_cache::QueryCache` — query result cache,
//!   one per app, held by the container
//! - `crate::features::search::plugin::init()` — Tauri plugin (directory-shaped)
//!
//! - `crate::features::search::engine` — retrieval engine (USearch +
//!   BM25 + hybrid/fusion/reranker/etc.)
//!   consumed by qa, mentions, indexing, conversation chat retrieval
//! - `crate::features::search::enrichment_service`
//!
//! The search DTOs, ranked branches and `HybridSearchTrait` live in core:
//! `lattice_core::application::contracts::search` and
//! `lattice_core::application::ports::hybrid_search`. That port's
//! `HybridSearchFactory`, implemented by `trait_def::HybridSearchUseCaseFactory`,
//! gives a passage index on disk (a folder's) this same orchestrator over its
//! own stores.
//!
//! Domain types used by the application layer live in the domain layer:
//! `lattice_core::domain::entities::SearchResult`,
//! and `lattice_core::domain::value_objects::{SearchMode, SearchQuery}`.
//!
//! The engine traits (`SearchServiceTrait`, `BM25SearchTrait`, `SparseSearchTrait`)
//! live in `lattice_core::application::ports::search_engines`.
//! The library search orchestrator is `use_cases::HybridSearchUseCase`.

pub mod commands;
pub mod di;
pub mod mapper;
pub mod plugin;
pub mod query_cache;
pub mod reranker_setup;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub mod engine;
pub mod enrichment_service;
pub mod hyde;
