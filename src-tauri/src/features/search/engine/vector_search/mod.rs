//! Vector index lifecycle for search.
//!
//! The USearch adapter itself lives in
//! [`crate::infrastructure::vector_index`]. This module decides what to do
//! with it: the manifest that certifies a persisted index against SQLite,
//! startup restore or rebuild, and the runtime handle swapped on a model
//! change.

pub mod manifest;
pub mod persistence;
pub mod runtime_index;

pub use manifest::{
    read_manifest, read_source_stamp, write_manifest, IndexManifest, ManifestMismatch, SourceStamp,
    MANIFEST_FORMAT_VERSION,
};
pub use persistence::{IndexPersistence, StartupPath};
