//! File checksum calculation utilities.

use crate::features::indexing::engine::error::{IndexingError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Calculate SHA256 checksum of a file.
///
/// Reads the file in chunks to handle large files efficiently.
pub async fn calculate_checksum(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| IndexingError::FileRead {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| IndexingError::FileRead {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        if n == 0 {
            break;
        }

        let data = buffer.get(..n).ok_or_else(|| IndexingError::FileRead {
            path: path.display().to_string(),
            reason: format!("Buffer slice out of bounds: 0..{}", n),
        })?;
        hasher.update(data);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_checksum_calculation() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        temp_file.flush().unwrap();

        let checksum1 = calculate_checksum(temp_file.path()).await.unwrap();
        let checksum2 = calculate_checksum(temp_file.path()).await.unwrap();

        assert_eq!(checksum1, checksum2);
        assert!(!checksum1.is_empty());
    }
}
