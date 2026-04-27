//! Hash computation and validation utilities.
//!
//! This module provides SHA256 hashing for content-addressed file storage
//! and validation of hash strings.

use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Computes SHA256 hash of a file.
///
/// Reads the file in chunks to handle large files efficiently.
///
/// # Arguments
///
/// * `path` - Path to the file to hash
///
/// # Returns
///
/// 64-character lowercase hex string representing the SHA256 hash
pub(crate) async fn compute_hash(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::FileStorage(format!("Failed to open file for hashing: {}", e)))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        let n = file.read(&mut buffer).await.map_err(|e| {
            AppError::FileStorage(format!("Failed to read file for hashing: {}", e))
        })?;

        if n == 0 {
            break;
        }
        let data = buffer.get(..n).ok_or_else(|| {
            AppError::FileStorage(format!("Buffer slice out of bounds: 0..{}", n))
        })?;
        hasher.update(data);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Validates that a string is a valid SHA256 hash.
///
/// A valid hash must be exactly 64 hexadecimal characters.
///
/// # Arguments
///
/// * `hash` - Hash string to validate
///
/// # Errors
///
/// Returns error if hash is not 64 characters or contains non-hex characters.
pub(crate) fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64 {
        return Err(AppError::FileStorage(format!(
            "Invalid hash length: {} (expected 64)",
            hash.len()
        )));
    }

    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::FileStorage(
            "Invalid hash: contains non-hex characters".to_string(),
        ));
    }

    Ok(())
}

/// Generates a storage path from a content hash.
///
/// Creates a two-level directory structure using the first two characters
/// of the hash as a prefix for better file system performance.
///
/// # Arguments
///
/// * `hash` - SHA256 hash (64 hex characters)
///
/// # Returns
///
/// Storage path in format: `files/{first_2_chars}/{remaining_62_chars}`
///
/// # Example
///
/// ```ignore
/// let path = get_storage_path("abc123...")?;
/// // Returns: "files/ab/c123..."
/// ```
pub(crate) fn get_storage_path(hash: &str) -> Result<String> {
    validate_hash(hash)?;
    Ok(format!("files/{}/{}", &hash[0..2], &hash[2..]))
}
