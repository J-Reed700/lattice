//! File system operations.
//!
//! This module provides low-level file system operations for the storage system,
//! including atomic file copying.

use crate::shared::error::{AppError, Result};
use std::path::Path;

/// Copies a file atomically by writing to a temporary file first, then renaming.
///
/// This ensures that the destination file is never in a partial state, which is
/// important for crash recovery and concurrent access.
///
/// # Arguments
///
/// * `source` - Source file path
/// * `dest` - Destination file path
///
/// # Process
///
/// 1. Copy source to `dest.tmp`
/// 2. Atomically rename `dest.tmp` to `dest`
///
/// # Errors
///
/// Returns error if copy or rename operations fail.
pub(crate) async fn copy_file_atomic(source: &Path, dest: &Path) -> Result<()> {
    // Write to temp file first
    let temp_path = dest.with_extension("tmp");

    tokio::fs::copy(source, &temp_path).await.map_err(|e| {
        AppError::FileStorage(format!("Failed to copy file to temp location: {}", e))
    })?;

    // Atomic rename
    tokio::fs::rename(&temp_path, dest)
        .await
        .map_err(|e| AppError::FileStorage(format!("Failed to rename temp file: {}", e)))?;

    Ok(())
}
