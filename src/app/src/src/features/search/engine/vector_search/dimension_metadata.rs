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
//! Format is intentionally boring: a single JSON object `{"dimension": N}`.
//! Readable, easy to debug, trivial to evolve if we need more metadata later.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::shared::error::AppError;
use crate::shared::result::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDimensionMetadata {
    pub dimension: usize,
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
    let meta_path = metadata_path_for(index_path);
    let bytes = std::fs::read(&meta_path).ok()?;
    let meta: IndexDimensionMetadata = serde_json::from_slice(&bytes).ok()?;
    Some(meta.dimension)
}

/// Write dimension metadata next to the index. Overwrites if present.
pub fn write_dimension(index_path: &Path, dimension: usize) -> Result<()> {
    let meta_path = metadata_path_for(index_path);
    let meta = IndexDimensionMetadata { dimension };
    let bytes = serde_json::to_vec_pretty(&meta).map_err(|e| {
        AppError::Serialization(format!("serialize dimension metadata: {}", e))
    })?;
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

    for p in [index_path, &meta_path, &keymap_path] {
        if p.exists() {
            if let Err(e) = std::fs::remove_file(p) {
                tracing::warn!(path = %p.display(), error = %e, "failed to remove stale index file");
            }
        }
    }
    Ok(())
}

/// Compare persisted dimension to the requested one. Returns whether a wipe
/// is needed, and (on wipe) performs it immediately so the caller can open a
/// clean index.
pub fn ensure_dimension_match(index_path: &Path, requested: usize) -> Result<DimensionCheck> {
    match read_dimension(index_path) {
        Some(existing) if existing == requested => Ok(DimensionCheck::Match),
        Some(existing) => {
            tracing::warn!(
                existing,
                requested,
                "USearch index dimension mismatch — wiping stale index"
            );
            wipe_index_files(index_path)?;
            write_dimension(index_path, requested)?;
            Ok(DimensionCheck::Wiped {
                previous_dimension: existing,
            })
        }
        None => {
            // Index may exist from an earlier version that didn't track
            // dimension. Write the sidecar so future startups see it, but
            // don't wipe blindly — the first load attempt will fail loudly
            // if dimensions actually differ, surfacing the real issue.
            write_dimension(index_path, requested)?;
            Ok(DimensionCheck::FreshMetadata)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DimensionCheck {
    /// Persisted dimension matches the requested one.
    Match,
    /// No persisted dimension was found — metadata file was created but the
    /// index was left untouched.
    FreshMetadata,
    /// Persisted dimension differed — index files were deleted.
    Wiped { previous_dimension: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
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
        // Create a stub index file + metadata
        std::fs::write(&path, b"stub").unwrap();
        write_dimension(&path, 384).unwrap();

        let result = ensure_dimension_match(&path, 1024).unwrap();
        assert_eq!(result, DimensionCheck::Wiped { previous_dimension: 384 });
        assert!(!path.exists(), "stub index file should have been deleted");
        assert_eq!(read_dimension(&path), Some(1024));
    }
}
