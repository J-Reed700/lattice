//! Hybrid Search Implementations
//!
//! This module contains hybrid search combining vector and text search.
//!
//! # Strategies
//! - Reciprocal Rank Fusion (RRF) for combining multiple rankings
//! - Cross-encoder reranking for improved relevance

pub mod reranker;
pub mod service;

pub use service::{HybridSearchResult, HybridSearchService, SearchConfig, SearchMode};
