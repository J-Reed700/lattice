//! Vector Search Implementations
//!
//! This module provides vector similarity search using USearch, a production-grade
//! SIMD-accelerated HNSW library with built-in persistence.
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
pub mod manifest;
pub mod persistence;
pub mod rescore_store;
pub mod runtime_index;
pub mod usearch_index;

pub use compression::{VectorIndexCompression, VectorQuantization, DEFAULT_RESCORE_FACTOR};
pub use dimension_metadata::{
    ensure_dimension_match, ensure_index_layout_match, metadata_path_for, read_dimension,
    read_metadata, wipe_index_files, DimensionCheck, IndexDimensionMetadata,
};
pub use manifest::{
    manifest_path_for, read_manifest, read_source_stamp, write_manifest, IndexConfig,
    IndexManifest, ManifestMismatch, SourceStamp, MANIFEST_FORMAT_VERSION,
};
pub use persistence::{IndexPersistence, StartupPath};
pub use rescore_store::{vectors_path_for, RescoreVectorStore};
pub use usearch_index::{SavePolicy, USearchVectorIndex};
