//! USearch vector index adapter.
//!
//! A production-grade, SIMD-accelerated HNSW index with built-in persistence.
//! It implements core's `VectorSearchPort` and `SearchServiceTrait`; the
//! search feature decides when to open, rebuild and certify it (see
//! `features::search::engine::vector_search`).
//!
//! # Architecture
//! - USearch owns the HNSW index and vector data (persistent, mmap-backed)
//! - SQLite owns document/chunk metadata
//! - A key map bridges string IDs to USearch's u64 keys
//!
//! # Compression
//! Vectors are stored full-dimension `f32` by default. [`VectorIndexCompression`]
//! opts an index into Matryoshka truncation and/or `i8` quantization, in which
//! case the HNSW index becomes a candidate generator and results are rescored
//! with exact cosine against the full-precision vectors kept in
//! [`rescore_store`].

pub mod compression;
pub mod dimension_metadata;
pub mod index_config;
pub mod rescore_store;
pub mod usearch_index;
pub mod vector_ops;

pub use compression::{VectorIndexCompression, VectorQuantization, DEFAULT_RESCORE_FACTOR};
pub use dimension_metadata::{
    ensure_dimension_match, ensure_index_layout_match, metadata_path_for, read_dimension,
    read_metadata, wipe_index_files, DimensionCheck, IndexDimensionMetadata,
};
pub use index_config::{manifest_path_for, IndexConfig};
pub use rescore_store::{vectors_path_for, RescoreVectorStore};
pub use usearch_index::{SavePolicy, USearchVectorIndex};
pub use vector_ops::{cosine_similarity_naive, cosine_similarity_simd, normalize_vector};

#[cfg(test)]
mod vector_ops_test;
