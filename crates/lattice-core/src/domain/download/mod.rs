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
            // These reach the user as a toast, so each carries the plain
            // sentence; the raw detail (hashes, URLs, OS errors) is logged
            // where the error arises.
            DownloadError::ChecksumMismatch { .. } => AppError::InvalidData(e.user_message()),
            DownloadError::NetworkError(_) => AppError::Network(e.user_message()),
            DownloadError::IoError(_) => AppError::FileSystem(e.user_message()),
            DownloadError::Cancelled => AppError::Other("Download cancelled".to_string()),
            DownloadError::MaxRetriesExceeded => AppError::Other(e.user_message()),
            DownloadError::HttpError { .. } => AppError::Network(e.user_message()),
            DownloadError::InvalidResponse(_) => AppError::Network(e.user_message()),
            DownloadError::ValidationFailed(msg) => AppError::ValidationFailed(msg),
            DownloadError::EngineInitializationError(msg) => {
                AppError::InternalError(format!("Failed to initialize download engine: {}", msg))
            }
            DownloadError::DestinationBusy(_) => AppError::InvalidState(e.user_message()),
            DownloadError::InsufficientDiskSpace { .. } => AppError::FileSystem(e.user_message()),
        }
    }
}
