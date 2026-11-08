//! The configuration a persisted USearch index was built with, and where the
//! manifest that certifies it lives.
//!
//! The manifest itself (its source stamp and the SQLite check behind it)
//! belongs to the search feature; the index only needs to know the manifest's
//! path, so it can invalidate it before a save, and the layout it was built
//! with.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::compression::{VectorIndexCompression, VectorQuantization};

/// Compute the manifest path from the index path
/// (`foo.usearch` → `foo.usearch.manifest.json`).
pub fn manifest_path_for(index_path: &Path) -> PathBuf {
    let mut p = index_path.as_os_str().to_owned();
    p.push(".manifest.json");
    PathBuf::from(p)
}

/// The graph and vector layout the index was built with. Every field here
/// changes what the persisted file means, so any difference forces a rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConfig {
    /// Embedding dimension callers hand in.
    pub dimension: usize,
    /// Dimension actually stored in USearch (smaller under truncation).
    pub stored_dimension: usize,
    pub metric: String,
    pub quantization: String,
    /// HNSW `M`.
    pub connectivity: usize,
    pub expansion_add: usize,
    pub expansion_search: usize,
}

impl IndexConfig {
    pub fn new(
        dimension: usize,
        compression: &VectorIndexCompression,
        connectivity: usize,
        expansion_add: usize,
        expansion_search: usize,
    ) -> Self {
        Self {
            dimension,
            stored_dimension: compression.stored_dimension(dimension),
            metric: "cos".to_string(),
            quantization: match compression.quantization() {
                VectorQuantization::F32 => "f32",
                VectorQuantization::I8 => "i8",
            }
            .to_string(),
            connectivity,
            expansion_add,
            expansion_search,
        }
    }
}
