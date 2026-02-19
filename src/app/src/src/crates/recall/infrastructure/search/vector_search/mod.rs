//! Vector Search Implementations
//!
//! This module provides vector similarity search using USearch, a production-grade
//! SIMD-accelerated HNSW library with built-in persistence.
//!
//! # Architecture
//! - USearch owns the HNSW index and vector data (persistent, mmap-backed)
//! - SQLite owns document/chunk metadata
//! - A key map bridges string IDs to USearch's u64 keys

pub mod usearch_index;

// Re-export public types
pub use usearch_index::USearchVectorIndex;
