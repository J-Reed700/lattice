//! Hybrid retrieval combining vector (USearch HNSW), keyword (SQLite FTS5 BM25),
//! filename, and recency search with reciprocal-rank fusion.

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
