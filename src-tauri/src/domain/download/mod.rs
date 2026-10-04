//! Download sessions, state transitions, and progress snapshots.

mod session;
pub mod snapshot;

pub use session::*;

use crate::shared::error::AppError;

/// Auto-convert from DownloadError to AppError
impl From<crate::domain::download::DownloadError> for AppError {
    fn from(e: crate::domain::download::DownloadError) -> Self {
        use crate::domain::download::DownloadError;
        match e {
            DownloadError::InvalidUrl(msg) => AppError::InvalidUrl(msg),
            DownloadError::InvalidDestination(msg) => AppError::InvalidInput(msg),
            DownloadError::InvalidStateTransition { from, to } => AppError::InvalidState(format!(
                "Invalid download state transition from {:?} to {:?}",
                from, to
            )),
            DownloadError::SessionNotFound(id) => {
                AppError::NotFound(format!("Download session not found: {}", id))
            }
            DownloadError::ChecksumMismatch { expected, actual } => AppError::InvalidData(format!(
                "Checksum mismatch: expected {}, got {}",
                expected, actual
            )),
            DownloadError::NetworkError(msg) => AppError::Network(msg),
            DownloadError::IoError(msg) => AppError::FileSystem(msg),
            DownloadError::Cancelled => AppError::Other("Download cancelled".to_string()),
            DownloadError::MaxRetriesExceeded => {
                AppError::Other("Maximum retry attempts exceeded".to_string())
            }
            DownloadError::HttpError { status, message } => {
                AppError::Network(format!("HTTP error {}: {}", status, message))
            }
            DownloadError::InvalidResponse(msg) => {
                AppError::Network(format!("Invalid HTTP response: {}", msg))
            }
            DownloadError::ValidationFailed(msg) => AppError::ValidationFailed(msg),
            DownloadError::EngineInitializationError(msg) => {
                AppError::InternalError(format!("Failed to initialize download engine: {}", msg))
            }
            DownloadError::InsufficientDiskSpace {
                required,
                available,
            } => AppError::FileSystem(format!(
                "Insufficient disk space: required {} bytes, available {} bytes",
                required, available
            )),
        }
    }
}
