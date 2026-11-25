//! Resume arithmetic for partial files and post-transfer file validation.

use crate::infrastructure::services::file_cleanup::FileCleanupService;
use lattice_core::domain::download::{human_bytes, Checksum, DownloadError};
use std::path::Path;
use tracing::{debug, info, warn};

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

/// Move a file written by the old in-place writer to its `.part` name so the
/// transfer resumes it.
///
/// Only a strictly partial file qualifies (`0 < len < total`, and only when no
/// `.part` exists yet): a full-size file at the destination is a finished
/// download and stays where it is. Returns whether a file was moved.
pub(super) async fn adopt_in_place_partial(
    destination: &Path,
    part: &Path,
    total_bytes: Option<u64>,
) -> bool {
    let Some(total) = total_bytes else {
        return false;
    };
    if tokio::fs::try_exists(part).await.unwrap_or(true) {
        return false;
    }
    let Ok(metadata) = tokio::fs::metadata(destination).await else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() >= total {
        return false;
    }
    match tokio::fs::rename(destination, part).await {
        Ok(()) => {
            info!(
                destination = %destination.display(),
                bytes = metadata.len(),
                "Moved a partial file from the old in-place writer to its .part file"
            );
            true
        }
        Err(error) => {
            warn!(%error, destination = %destination.display(), "Could not move a partial file to its .part file");
            false
        }
    }
}

/// Whether the destination already holds the finished file: no `.part` is in
/// the way and the destination's length equals the known total. With no
/// checksum that is taken as done (multi-file models re-plan files they
/// already have). With one, the file is hashed in place; a file that does not
/// match is deleted so the download starts clean into a `.part`.
pub(super) async fn final_file_already_complete(
    destination: &Path,
    part: &Path,
    total_bytes: Option<u64>,
    checksum: Option<&Checksum>,
    file_cleanup: &FileCleanupService,
) -> bool {
    let Some(total) = total_bytes else {
        return false;
    };
    if tokio::fs::try_exists(part).await.unwrap_or(true) {
        return false;
    }
    let Ok(metadata) = tokio::fs::metadata(destination).await else {
        return false;
    };
    if !metadata.is_file() || metadata.len() != total || total == 0 {
        return false;
    }
    let Some(checksum) = checksum else {
        return true;
    };
    match sha256_file(destination).await {
        Ok(actual) if checksum.verify(&actual).is_ok() => true,
        Ok(_) => {
            warn!(destination = %destination.display(), "Existing file does not match its checksum; downloading it again");
            file_cleanup.delete_file_best_effort(destination).await;
            false
        }
        Err(error) => {
            warn!(%error, destination = %destination.display(), "Could not hash the existing file");
            false
        }
    }
}

/// SHA-256 of a whole file, as lowercase hex.
pub(super) async fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(buffer.get(..read).unwrap_or_default());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Rename a verified `.part` file onto its destination, replacing whatever is
/// there on every platform. Windows refuses to rename over a file another
/// handle holds open, so a failed rename is retried once after removing the
/// destination.
pub(super) async fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match tokio::fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(first) => {
            if !tokio::fs::try_exists(to).await.unwrap_or(false) {
                return Err(first);
            }
            tokio::fs::remove_file(to).await?;
            tokio::fs::rename(from, to).await
        }
    }
}

/// Validate that a downloaded file exists and has the expected size.
///
/// The error text is what the user sees on the failed row, so it is one plain
/// sentence; the exact paths and byte counts go to the log.
pub(super) async fn validate_downloaded_file(
    path: &Path,
    expected_bytes: u64,
) -> Result<(), DownloadError> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) => {
            warn!(path = %path.display(), %error, "Downloaded file cannot be read");
            return Err(DownloadError::ValidationFailed(
                "The downloaded file couldn't be found on disk. Try again.".to_string(),
            ));
        }
    };

    let actual_size = metadata.len();

    if actual_size == 0 {
        warn!(path = %path.display(), "Downloaded file is empty");
        return Err(DownloadError::ValidationFailed(
            "The downloaded file was empty, so it was discarded. Try again.".to_string(),
        ));
    }

    if actual_size != expected_bytes {
        warn!(
            path = %path.display(),
            expected_bytes,
            actual_size,
            "Downloaded file size mismatch"
        );
        let (actual, expected) = match (human_bytes(actual_size), human_bytes(expected_bytes)) {
            // A few bytes short rounds to the same figure; say it exactly.
            (actual, expected) if actual == expected => (
                format!("{actual_size} bytes"),
                format!("{expected_bytes} bytes"),
            ),
            pair => pair,
        };
        return Err(DownloadError::ValidationFailed(format!(
            "The downloaded file was {actual} instead of the expected {expected}, so it was \
             discarded. Try again."
        )));
    }

    debug!(
        path = %path.display(),
        size = actual_size,
        "File validation passed"
    );

    Ok(())
}
