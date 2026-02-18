//! Tokio-based checksum adapter.
//!
//! Implements file checksum calculations using the Tokio async runtime.

use crate::domain::ports::file_access::ChecksumService;
use crate::shared::error::AppError;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Tokio-based adapter for calculating file checksums.
///
/// This adapter uses Tokio's async filesystem operations to calculate
/// SHA256 checksums of files. It implements the domain's ChecksumService
/// port, allowing the domain to remain independent of the async runtime.
pub struct TokioChecksumAdapter;

#[async_trait::async_trait]
impl ChecksumService for TokioChecksumAdapter {
    async fn calculate_sha256(&self, file_path: &Path) -> Result<String, AppError> {
        let mut file = tokio::fs::File::open(file_path).await.map_err(|e| {
            AppError::FileSystem(format!(
                "Failed to open file for checksum: {}: {}",
                file_path.display(),
                e
            ))
        })?;

        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer).await.map_err(|e| {
                AppError::FileSystem(format!(
                    "Failed to read file for checksum: {}: {}",
                    file_path.display(),
                    e
                ))
            })?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(buffer.get(..bytes_read).unwrap_or(&[]));
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
