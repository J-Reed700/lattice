//! Hybrid Search Implementations
//!
//! This module contains hybrid search combining vector and text search.
//!
//! # Migration Status
//! - [ ] fusion.rs - Will move from `search/fusion.rs`
//! - [ ] reranker.rs - Will move from `search/reranker.rs`
//!
//! # Strategies
//! - Reciprocal Rank Fusion (RRF) for combining multiple rankings
//! - Cross-encoder reranking for improved relevance

pub mod fusion;
pub mod reranker;
pub mod service;

pub use service::{HybridSearchResult, HybridSearchService, SearchConfig, SearchMode};
