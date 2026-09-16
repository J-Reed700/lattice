//! Vector Search Implementations
//!
//! This module provides vector similarity search using USearch, a production-grade
//! SIMD-accelerated HNSW library with built-in persistence.
//!
//! # Architecture
//! - USearch owns the HNSW index and vector data (persistent, mmap-backed)
//! - SQLite owns document/chunk metadata
//! - A key map bridges string IDs to USearch's u64 keys

pub mod dimension_metadata;
pub mod usearch_index;

// Re-export public types
pub use dimension_metadata::{
    ensure_dimension_match, metadata_path_for, read_dimension, wipe_index_files, DimensionCheck,
};
pub use usearch_index::USearchVectorIndex;
