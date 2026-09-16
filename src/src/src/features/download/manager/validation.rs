//! Resume arithmetic for partial files and post-transfer file validation.

use crate::domain::download::DownloadError;
use std::path::PathBuf;
use tracing::debug;

/// Resolve the byte offset for a persisted partial download.
///
/// The file is append-only, while DB progress is intentionally throttled, so
/// the file will commonly be ahead of the last recorded byte count after a
/// crash. The file length is therefore authoritative unless it is smaller
/// than the recorded progress or exceeds the advertised total.
pub(super) fn recover_resume_offset(
    recorded_bytes: u64,
    file_size: u64,
    total_bytes: Option<u64>,
) -> Option<u64> {
    if file_size == 0 || file_size < recorded_bytes {
        return None;
    }
    if total_bytes.is_some_and(|total| file_size > total) {
        return None;
    }
    Some(file_size)
}

/// Validate that a downloaded file exists and has the expected size
///
/// # Arguments
/// * `path` - Path to the downloaded file
/// * `expected_bytes` - Expected file size in bytes
///
/// # Returns
/// * `Ok(())` if file exists and has the expected size
/// * `Err(DownloadError)` if validation fails
pub(super) async fn validate_downloaded_file(
    path: &PathBuf,
    expected_bytes: u64,
) -> Result<(), DownloadError> {
    if !path.exists() {
        return Err(DownloadError::ValidationFailed(format!(
            "Downloaded file does not exist: {}",
            path.display()
        )));
    }

    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to read file metadata: {}", e)))?;

    let actual_size = metadata.len();

    if actual_size == 0 {
        return Err(DownloadError::ValidationFailed(format!(
            "Downloaded file is empty (0 bytes): {}",
            path.display()
        )));
    }

    if actual_size != expected_bytes {
        return Err(DownloadError::ValidationFailed(format!(
            "File size mismatch: expected {} bytes, got {} bytes",
            expected_bytes, actual_size
        )));
    }

    debug!(
        path = %path.display(),
        size = actual_size,
        "File validation passed"
    );

    Ok(())
}
