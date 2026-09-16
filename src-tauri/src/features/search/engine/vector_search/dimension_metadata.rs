//! USearch index dimension metadata.
//!
//! The USearch index file itself doesn't expose its vector dimension without
//! loading the whole file. We persist a tiny sidecar file (`<index>.dim`)
//! that stores just the dimension so startup can:
//!
//!  - Detect when the active embedding model's dimension has changed (e.g.
//!    user switched from BGE-small-384 to mxbai-1024)
//!  - Wipe the stale index before USearch tries to load mismatched vectors
//!
//!  - Detect when the vector *compression* configuration has changed, which
//!    reshapes the stored vectors just as thoroughly as a dimension change
//!    does (a 512-d `i8` vector cannot be compared against a 1024-d `f32` one)
//!
//! Format is intentionally boring: a single JSON object
//! `{"dimension": N, "compression": {...}}`. Readable, easy to debug, trivial
//! to evolve if we need more metadata later. Sidecars written before
//! compression existed carry only `dimension` and load as
//! [`VectorIndexCompression::None`] — the configuration they were in fact
//! written under.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::compression::VectorIndexCompression;
use super::rescore_store::vectors_path_for;
use crate::shared::error::AppError;
use crate::shared::result::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDimensionMetadata {
    pub dimension: usize,
    /// How vectors are stored. Absent in sidecars written before compression
    /// was configurable, where the only possible answer was `None`.
    #[serde(default)]
    pub compression: VectorIndexCompression,
}

/// Compute the metadata path from the index path (`foo.usearch` → `foo.usearch.dim`).
pub fn metadata_path_for(index_path: &Path) -> PathBuf {
    let mut p = index_path.as_os_str().to_owned();
    p.push(".dim");
    PathBuf::from(p)
}

/// Read persisted dimension, or `None` if the sidecar doesn't exist / is
/// malformed. A malformed file is treated as missing — we wipe and recreate
/// rather than block on someone's half-written JSON.
pub fn read_dimension(index_path: &Path) -> Option<usize> {
    Some(read_metadata(index_path)?.dimension)
}

/// Read the full persisted sidecar, or `None` if it doesn't exist / is
/// malformed.
pub fn read_metadata(index_path: &Path) -> Option<IndexDimensionMetadata> {
    let meta_path = metadata_path_for(index_path);
    let bytes = std::fs::read(&meta_path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write dimension metadata next to the index, recording no compression.
pub fn write_dimension(index_path: &Path, dimension: usize) -> Result<()> {
    write_metadata(index_path, dimension, &VectorIndexCompression::None)
}

/// Write dimension + compression metadata next to the index. Overwrites if present.
pub fn write_metadata(
    index_path: &Path,
    dimension: usize,
    compression: &VectorIndexCompression,
) -> Result<()> {
    let meta_path = metadata_path_for(index_path);
    let meta = IndexDimensionMetadata {
        dimension,
        compression: compression.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&meta)
        .map_err(|e| AppError::Serialization(format!("serialize dimension metadata: {}", e)))?;
    std::fs::write(&meta_path, bytes).map_err(|e| {
        AppError::FileStorage(format!(
            "write dimension metadata to {}: {}",
            meta_path.display(),
            e
        ))
    })?;
    Ok(())
}

/// Delete the index file + sidecar + known companion files if they exist.
/// Called when the model dimension changes — USearch can't reshape an index
/// at runtime, so a full wipe + rebuild is the only safe migration.
pub fn wipe_index_files(index_path: &Path) -> Result<()> {
    let meta_path = metadata_path_for(index_path);
    let keymap_path = {
        let mut p = index_path.as_os_str().to_owned();
        p.push(".keymap.json");
        PathBuf::from(p)
    };
    // The key map USearchVectorIndex actually writes lives next to the index
    // under the stem, not the full file name; remove both spellings.
    let stem_keymap_path = index_path.with_file_name(format!(
        "{}.keymap.json",
        index_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "index".to_string())
    ));
    let vectors_path = vectors_path_for(index_path);

    for p in [
        index_path,
        &meta_path,
        &keymap_path,
        &stem_keymap_path,
        &vectors_path,
    ] {
        if p.exists() {
            if let Err(e) = std::fs::remove_file(p) {
                tracing::warn!(path = %p.display(), error = %e, "failed to remove stale index file");
            }
        }
    }
    Ok(())
}

/// Compare persisted dimension to the requested one, assuming no compression.
///
/// Thin wrapper over [`ensure_index_layout_match`] kept for callers that never
/// opt into compression.
pub fn ensure_dimension_match(index_path: &Path, requested: usize) -> Result<DimensionCheck> {
    ensure_index_layout_match(index_path, requested, &VectorIndexCompression::None)
}

/// Compare the persisted vector layout — dimension *and* compression — to the
/// requested one. Returns whether a wipe is needed, and (on wipe) performs it
/// immediately so the caller can open a clean index.
///
/// A compression change is exactly as destructive as a dimension change:
/// USearch cannot reinterpret 512 `i8` components as 1024 `f32` ones, and a
/// half-converted index would silently rank two incomparable vector spaces
/// against each other. Both therefore take the same path — wipe, rewrite the
/// sidecar, let the caller rebuild from SQLite.
pub fn ensure_index_layout_match(
    index_path: &Path,
    requested: usize,
    compression: &VectorIndexCompression,
) -> Result<DimensionCheck> {
    match read_metadata(index_path) {
        Some(existing) if existing.dimension != requested => {
            tracing::warn!(
                existing = existing.dimension,
                requested,
                "USearch index dimension mismatch — wiping stale index"
            );
            wipe_index_files(index_path)?;
            write_metadata(index_path, requested, compression)?;
            Ok(DimensionCheck::Wiped {
                previous_dimension: existing.dimension,
            })
        }
        Some(existing) if !existing.compression.layout_matches(compression) => {
            tracing::warn!(
                existing = ?existing.compression,
                requested = ?compression,
                "USearch index compression mismatch — wiping stale index"
            );
            wipe_index_files(index_path)?;
            write_metadata(index_path, requested, compression)?;
            Ok(DimensionCheck::WipedCompression {
                previous_compression: existing.compression,
            })
        }
        Some(existing) if existing.compression != *compression => {
            // Same stored layout, different query-time rescore factor. Nothing
            // on disk has to change; just record the new knob.
            write_metadata(index_path, requested, compression)?;
            Ok(DimensionCheck::Match)
        }
        Some(_) => Ok(DimensionCheck::Match),
        None => {
            // Index may exist from an earlier version that didn't track
            // dimension. Write the sidecar so future startups see it, but
            // don't wipe blindly — the first load attempt will fail loudly
            // if dimensions actually differ, surfacing the real issue.
            write_metadata(index_path, requested, compression)?;
            Ok(DimensionCheck::FreshMetadata)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DimensionCheck {
    /// Persisted dimension and compression match the requested ones.
    Match,
    /// No persisted metadata was found — metadata file was created but the
    /// index was left untouched.
    FreshMetadata,
    /// Persisted dimension differed — index files were deleted.
    Wiped { previous_dimension: usize },
    /// Persisted compression configuration differed — index files were deleted.
    WipedCompression {
        previous_compression: VectorIndexCompression,
    },
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use crate::features::search::engine::vector_search::compression::VectorQuantization;
    use tempfile::TempDir;

    fn tmp_index() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.usearch");
        (dir, path)
    }

    #[test]
    fn metadata_path_appends_dim_suffix() {
        let p = Path::new("/tmp/foo.usearch");
        assert_eq!(metadata_path_for(p), PathBuf::from("/tmp/foo.usearch.dim"));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let (_dir, path) = tmp_index();
        write_dimension(&path, 384).unwrap();
        assert_eq!(read_dimension(&path), Some(384));
    }

    #[test]
    fn read_missing_returns_none() {
        let (_dir, path) = tmp_index();
        assert_eq!(read_dimension(&path), None);
    }

    #[test]
    fn ensure_match_on_fresh_dir_is_fresh_metadata() {
        let (_dir, path) = tmp_index();
        let result = ensure_dimension_match(&path, 384).unwrap();
        assert_eq!(result, DimensionCheck::FreshMetadata);
        assert_eq!(read_dimension(&path), Some(384));
    }

    #[test]
    fn ensure_match_on_same_dim_is_match() {
        let (_dir, path) = tmp_index();
        write_dimension(&path, 384).unwrap();
        let result = ensure_dimension_match(&path, 384).unwrap();
        assert_eq!(result, DimensionCheck::Match);
    }

    #[test]
    fn ensure_match_on_different_dim_wipes_and_records() {
        let (_dir, path) = tmp_index();
        std::fs::write(&path, b"stub").unwrap();
        write_dimension(&path, 384).unwrap();

        let result = ensure_dimension_match(&path, 1024).unwrap();
        assert_eq!(
            result,
            DimensionCheck::Wiped {
                previous_dimension: 384
            }
        );
        assert!(!path.exists(), "stub index file should have been deleted");
        assert_eq!(read_dimension(&path), Some(1024));
    }

    #[test]
    fn metadata_round_trips_with_a_compression_config() {
        let (_dir, path) = tmp_index();
        let compression = VectorIndexCompression::truncated(512, VectorQuantization::I8);
        write_metadata(&path, 1024, &compression).unwrap();

        let meta = read_metadata(&path).unwrap();
        assert_eq!(meta.dimension, 1024);
        assert_eq!(meta.compression, compression);
        assert_eq!(read_dimension(&path), Some(1024));
    }

    /// Sidecars written before compression existed carry only `dimension`.
    /// They describe an uncompressed index and must load as one rather than
    /// being treated as malformed (which would wipe a healthy index).
    #[test]
    fn metadata_without_the_compression_field_reads_as_none() {
        let (_dir, path) = tmp_index();
        std::fs::write(metadata_path_for(&path), br#"{"dimension": 384}"#).unwrap();

        let meta = read_metadata(&path).unwrap();
        assert_eq!(meta.dimension, 384);
        assert_eq!(meta.compression, VectorIndexCompression::None);
        assert_eq!(
            ensure_dimension_match(&path, 384).unwrap(),
            DimensionCheck::Match,
            "a legacy sidecar must not trigger a rebuild"
        );
    }

    #[test]
    fn switching_compression_on_wipes_the_index() {
        let (_dir, path) = tmp_index();
        std::fs::write(&path, b"stub").unwrap();
        write_dimension(&path, 1024).unwrap();

        let compression = VectorIndexCompression::truncated(512, VectorQuantization::I8);
        let result = ensure_index_layout_match(&path, 1024, &compression).unwrap();
        assert_eq!(
            result,
            DimensionCheck::WipedCompression {
                previous_compression: VectorIndexCompression::None
            }
        );
        assert!(!path.exists(), "stub index file should have been deleted");
        assert_eq!(read_metadata(&path).unwrap().compression, compression);
    }

    #[test]
    fn switching_compression_off_wipes_the_index() {
        let (_dir, path) = tmp_index();
        std::fs::write(&path, b"stub").unwrap();
        let compression = VectorIndexCompression::truncated(512, VectorQuantization::I8);
        write_metadata(&path, 1024, &compression).unwrap();

        let result = ensure_dimension_match(&path, 1024).unwrap();
        assert_eq!(
            result,
            DimensionCheck::WipedCompression {
                previous_compression: compression
            }
        );
        assert!(!path.exists());
    }

    #[test]
    fn changing_only_the_rescore_factor_keeps_the_index() {
        let (_dir, path) = tmp_index();
        std::fs::write(&path, b"stub").unwrap();
        write_metadata(
            &path,
            1024,
            &VectorIndexCompression::truncated(512, VectorQuantization::I8),
        )
        .unwrap();

        let retuned = VectorIndexCompression::Truncated {
            dims: 512,
            quantization: VectorQuantization::I8,
            rescore_factor: 12,
        };
        assert_eq!(
            ensure_index_layout_match(&path, 1024, &retuned).unwrap(),
            DimensionCheck::Match
        );
        assert!(path.exists(), "a query-time knob must not cost a rebuild");
        assert_eq!(read_metadata(&path).unwrap().compression, retuned);
    }

    #[test]
    fn wipe_removes_the_rescore_vector_store() {
        let (_dir, path) = tmp_index();
        std::fs::write(&path, b"stub").unwrap();
        let vectors = super::vectors_path_for(&path);
        std::fs::write(&vectors, b"vectors").unwrap();
        let keymap = path.with_file_name("test.keymap.json");
        std::fs::write(&keymap, b"{}").unwrap();

        wipe_index_files(&path).unwrap();

        assert!(!path.exists());
        assert!(!vectors.exists(), "stale full vectors would rescore ghosts");
        assert!(!keymap.exists());
    }
}
