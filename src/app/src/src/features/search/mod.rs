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
//! ## Kept as shared namespaces (redirects retained)
//!
//! - `crate::domain::entities::search_result::SearchResult`
//! - `crate::domain::value_objects::{search_mode, search_query}`
//! - `crate::domain::repositories::search_repository`
//! - `crate::domain::services::search_ranking_service`
//! - `crate::infrastructure::search` — retrieval engine (USearch +
//!   BM25 + hybrid/fusion/reranker/query_expansion/etc.)
//!   consumed by qa, mentions, indexing, conversation chat retrieval
//! - `crate::infrastructure::persistence::repositories::search`
//!   (tx-wrapper)
//! - `crate::infrastructure::services::search_enrichment_service`
//! - `trait_def` + `mocks` via shared services aggregators

pub mod commands;
pub mod dto;
pub mod mapper;
pub mod plugin;
pub mod use_cases;
