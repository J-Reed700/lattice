use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Invalid destination path: {0}")]
    InvalidDestination(String),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: DownloadState,
        to: DownloadState,
    },

    #[error("Download session not found: {0}")]
    SessionNotFound(String),

    #[error("Checksum verification failed: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Download cancelled")]
    Cancelled,

    #[error("Maximum retry attempts exceeded")]
    MaxRetriesExceeded,

    #[error("HTTP error {status}: {message}")]
    HttpError { status: u16, message: String },

    #[error("Invalid HTTP response: {0}")]
    InvalidResponse(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("HTTP engine failed to initialize: {0}")]
    EngineInitializationError(String),

    #[error("Insufficient disk space: required {required} bytes, available {available} bytes")]
    InsufficientDiskSpace { required: u64, available: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn can_transition_to(&self, target: &DownloadState) -> bool {
        use DownloadState::*;
        matches!(
            (self, target),
            (Pending, Downloading)
                | (Pending, Cancelled)
                | (Downloading, Paused)
                | (Downloading, Completed)
                | (Downloading, Failed)
                | (Downloading, Cancelled)
                | (Paused, Downloading)
                | (Paused, Cancelled)
                | (Failed, Downloading)
                | (Failed, Pending)
                | (Cancelled, Downloading)
                | (Cancelled, Pending)
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DownloadState::Completed | DownloadState::Failed | DownloadState::Cancelled
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, DownloadState::Downloading)
    }

    pub fn can_pause(&self) -> bool {
        matches!(self, DownloadState::Downloading)
    }

    pub fn can_resume(&self) -> bool {
        matches!(self, DownloadState::Paused | DownloadState::Failed)
    }

    pub fn can_cancel(&self) -> bool {
        !self.is_terminal()
    }
}

/// Download progress snapshot
///
/// Immutable snapshot of download progress at a point in time.
/// Contains calculated metrics like percentage, speed, and ETA.
///
/// # Business Rules
/// - Speed is 0 when no data transferred yet
/// - Percentage is None when total_bytes unknown
/// - ETA is None when speed is 0 or total_bytes unknown
/// - Percentage clamped to 0-100 range
/// - ETA is 0 when download is complete or nearly complete
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    bytes_downloaded: u64,
    total_bytes: Option<u64>,
    bytes_per_second: f64,
    last_updated: DateTime<Utc>,
}

impl DownloadProgress {
    pub fn new(total_bytes: Option<u64>) -> Self {
        Self {
            bytes_downloaded: 0,
            total_bytes,
            bytes_per_second: 0.0,
            last_updated: Utc::now(),
        }
    }

    pub fn update(&mut self, bytes_downloaded: u64, bytes_per_second: f64) {
        self.bytes_downloaded = bytes_downloaded;
        self.bytes_per_second = bytes_per_second.max(0.0);
        self.last_updated = Utc::now();
    }

    /// Calculate download percentage
    ///
    /// Returns percentage (0-100) if total_bytes is known.
    /// Returns None if total_bytes is unknown or 0.
    ///
    /// # Edge Cases
    /// - Returns 100.0 if bytes_downloaded >= total_bytes
    /// - Returns 0.0 if total_bytes is 0
    /// - Clamps to 0-100 range
    pub fn percentage(&self) -> Option<f64> {
        self.total_bytes.map(|total| {
            if total == 0 {
                return 0.0;
            }
            let percentage = (self.bytes_downloaded as f64 / total as f64) * 100.0;
            percentage.clamp(0.0, 100.0)
        })
    }

    pub fn bytes_downloaded(&self) -> u64 {
        self.bytes_downloaded
    }

    pub fn total_bytes(&self) -> Option<u64> {
        self.total_bytes
    }

    pub fn bytes_per_second(&self) -> f64 {
        self.bytes_per_second
    }

    /// Estimate time remaining in seconds
    ///
    /// Returns ETA in seconds if total_bytes is known and speed > 0.
    /// Returns None if speed is 0, total_bytes unknown, or download complete.
    ///
    /// # Edge Cases
    /// - Returns None if bytes_per_second <= 0
    /// - Returns Some(0) if already complete (downloaded >= total)
    /// - Returns None if total_bytes unknown
    /// - Returns rounded seconds (no fractional seconds)
    pub fn estimated_time_remaining(&self) -> Option<u64> {
        if self.bytes_per_second <= 0.0 {
            return None;
        }

        self.total_bytes.map(|total| {
            if self.bytes_downloaded >= total {
                return 0;
            }
            let remaining = total.saturating_sub(self.bytes_downloaded);
            (remaining as f64 / self.bytes_per_second).round() as u64
        })
    }

    pub fn is_complete(&self) -> bool {
        self.total_bytes
            .map(|total| self.bytes_downloaded >= total)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumAlgorithm {
    Sha256,
    Md5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checksum {
    algorithm: ChecksumAlgorithm,
    value: String,
}

impl Checksum {
    pub fn new(algorithm: ChecksumAlgorithm, value: String) -> Result<Self, DownloadError> {
        if value.trim().is_empty() {
            return Err(DownloadError::InvalidUrl(
                "Checksum value cannot be empty".to_string(),
            ));
        }

        let expected_len = match algorithm {
            ChecksumAlgorithm::Sha256 => 64,
            ChecksumAlgorithm::Md5 => 32,
        };

        if value.len() != expected_len {
            return Err(DownloadError::InvalidUrl(format!(
                "Invalid checksum length: expected {}, got {}",
                expected_len,
                value.len()
            )));
        }

        if !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DownloadError::InvalidUrl(
                "Checksum must be hexadecimal".to_string(),
            ));
        }

        Ok(Self {
            algorithm,
            value: value.to_lowercase(),
        })
    }

    pub fn algorithm(&self) -> &ChecksumAlgorithm {
        &self.algorithm
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn verify(&self, actual: &str) -> Result<(), DownloadError> {
        let actual_normalized = actual.to_lowercase();
        if self.value == actual_normalized {
            Ok(())
        } else {
            Err(DownloadError::ChecksumMismatch {
                expected: self.value.clone(),
                actual: actual_normalized,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSession {
    id: String,
    url: String,
    destination: PathBuf,
    state: DownloadState,
    progress: DownloadProgress,
    checksum: Option<Checksum>,
    error_message: Option<String>,
    retry_count: u32,
    max_retries: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    model_name: Option<String>,
    model_id: Option<String>,
}

impl DownloadSession {
    const MAX_RETRIES: u32 = 3;

    pub fn new(
        id: String,
        url: String,
        destination: PathBuf,
        total_bytes: Option<u64>,
        checksum: Option<Checksum>,
    ) -> Result<Self, DownloadError> {
        if url.trim().is_empty() {
            return Err(DownloadError::InvalidUrl("URL cannot be empty".to_string()));
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(DownloadError::InvalidUrl(format!(
                "URL must start with http:// or https://, got: {}",
                url
            )));
        }

        if destination.to_str().map(|s| s.is_empty()).unwrap_or(true) {
            return Err(DownloadError::InvalidDestination(
                "Destination path cannot be empty".to_string(),
            ));
        }

        let now = Utc::now();

        Ok(Self {
            id,
            url,
            destination,
            state: DownloadState::Pending,
            progress: DownloadProgress::new(total_bytes),
            checksum,
            error_message: None,
            retry_count: 0,
            max_retries: Self::MAX_RETRIES,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            model_name: None,
            model_id: None,
        })
    }

    pub fn with_model_metadata(mut self, model_name: String, model_id: String) -> Self {
        self.model_name = Some(model_name);
        self.model_id = Some(model_id);
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn destination(&self) -> &PathBuf {
        &self.destination
    }

    pub fn state(&self) -> &DownloadState {
        &self.state
    }

    pub fn progress(&self) -> &DownloadProgress {
        &self.progress
    }

    pub fn checksum(&self) -> Option<&Checksum> {
        self.checksum.as_ref()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }

    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }

    pub fn is_stale_at(&self, cutoff: DateTime<Utc>) -> bool {
        self.state == DownloadState::Downloading && self.updated_at < cutoff
    }

    pub fn started_at(&self) -> Option<&DateTime<Utc>> {
        self.started_at.as_ref()
    }

    pub fn completed_at(&self) -> Option<&DateTime<Utc>> {
        self.completed_at.as_ref()
    }

    pub fn model_name(&self) -> Option<&str> {
        self.model_name.as_deref()
    }

    pub fn model_id(&self) -> Option<&str> {
        self.model_id.as_deref()
    }

    pub fn start(&mut self) -> Result<(), DownloadError> {
        self.transition_to(DownloadState::Downloading)?;
        if self.started_at.is_none() {
            self.started_at = Some(Utc::now());
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), DownloadError> {
        if !self.state.can_pause() {
            return Err(DownloadError::InvalidStateTransition {
                from: self.state,
                to: DownloadState::Paused,
            });
        }
        self.transition_to(DownloadState::Paused)
    }

    pub fn resume(&mut self) -> Result<(), DownloadError> {
        if !self.state.can_resume() {
            return Err(DownloadError::InvalidStateTransition {
                from: self.state,
                to: DownloadState::Downloading,
            });
        }
        self.transition_to(DownloadState::Downloading)
    }

    pub fn cancel(&mut self) -> Result<(), DownloadError> {
        if !self.state.can_cancel() {
            return Err(DownloadError::InvalidStateTransition {
                from: self.state,
                to: DownloadState::Cancelled,
            });
        }
        self.transition_to(DownloadState::Cancelled)
    }

    pub fn complete(&mut self) -> Result<(), DownloadError> {
        self.transition_to(DownloadState::Completed)?;
        self.completed_at = Some(Utc::now());
        Ok(())
    }

    pub fn fail(&mut self, error_message: String) -> Result<(), DownloadError> {
        self.error_message = Some(error_message);
        self.retry_count += 1;
        self.transition_to(DownloadState::Failed)
    }

    pub fn update_progress(&mut self, bytes_downloaded: u64, bytes_per_second: f64) {
        self.progress.update(bytes_downloaded, bytes_per_second);
        self.updated_at = Utc::now();
    }

    pub fn can_retry(&self) -> bool {
        self.state == DownloadState::Failed && self.retry_count < self.max_retries
    }

    /// Retry a failed download
    ///
    /// **Business Logic**:
    /// - Can only retry from Failed state
    /// - Retry count must be < max_retries (3)
    /// - Clears error_message
    /// - Resets progress to 0 (file may have been deleted)
    /// - Resets to Pending state for re-queueing
    ///
    /// **Invariants**:
    /// - retry_count < MAX_RETRIES (enforced by can_retry())
    /// - state == DownloadState::Failed
    ///
    /// **Critical**: Resets progress because failed downloads may have had their
    /// partial files deleted during cleanup. Starting fresh prevents crashes from
    /// trying to resume from a non-existent file.
    pub fn retry(&mut self) -> Result<(), DownloadError> {
        if !self.can_retry() {
            return Err(DownloadError::MaxRetriesExceeded);
        }

        self.error_message = None;
        // Reset progress to 0 - file may have been deleted during cleanup
        self.progress = DownloadProgress::new(self.progress.total_bytes());
        self.transition_to(DownloadState::Pending)
    }

    /// Manual retry initiated by user - resets retry counter and attempts download again
    /// This allows users to retry even after automatic retries are exhausted
    ///
    /// Only valid from Failed or Cancelled states to maintain state machine integrity
    pub fn manual_retry(&mut self) -> Result<(), DownloadError> {
        // Validate state - only allow manual retry from Failed or Cancelled states
        if self.state != DownloadState::Failed && self.state != DownloadState::Cancelled {
            return Err(DownloadError::InvalidStateTransition {
                from: self.state,
                to: DownloadState::Pending,
            });
        }

        // Reset retry counter to give fresh automatic retry attempts
        self.retry_count = 0;
        self.error_message = None;
        // Reset progress to 0 - file may have been deleted during cleanup
        self.progress = DownloadProgress::new(self.progress.total_bytes());
        self.transition_to(DownloadState::Pending)
    }

    pub fn should_verify_checksum(&self) -> bool {
        self.state == DownloadState::Completed && self.checksum.is_some()
    }

    fn transition_to(&mut self, new_state: DownloadState) -> Result<(), DownloadError> {
        if !self.state.can_transition_to(&new_state) {
            return Err(DownloadError::InvalidStateTransition {
                from: self.state,
                to: new_state,
            });
        }

        self.state = new_state;
        self.updated_at = Utc::now();

        if new_state.is_terminal() {
            self.error_message = None;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    #[test]
    fn test_download_state_transitions() {
        assert!(DownloadState::Pending.can_transition_to(&DownloadState::Downloading));
        assert!(DownloadState::Downloading.can_transition_to(&DownloadState::Paused));
        assert!(DownloadState::Paused.can_transition_to(&DownloadState::Downloading));
        assert!(!DownloadState::Completed.can_transition_to(&DownloadState::Downloading));
    }

    #[test]
    fn test_download_state_predicates() {
        assert!(DownloadState::Completed.is_terminal());
        assert!(!DownloadState::Downloading.is_terminal());
        assert!(DownloadState::Downloading.is_active());
        assert!(DownloadState::Downloading.can_pause());
        assert!(DownloadState::Paused.can_resume());
    }

    #[test]
    fn test_download_progress_percentage() {
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, 100.0);
        assert_eq!(progress.percentage(), Some(50.0));
    }

    #[test]
    fn test_download_progress_percentage_edge_cases() {
        // Unknown total
        let progress = DownloadProgress::new(None);
        assert_eq!(progress.percentage(), None);

        // Zero total
        let progress = DownloadProgress::new(Some(0));
        assert_eq!(progress.percentage(), Some(0.0));

        // Complete download
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(1000, 100.0);
        assert_eq!(progress.percentage(), Some(100.0));

        // Over 100% (should clamp)
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(1500, 100.0);
        assert_eq!(progress.percentage(), Some(100.0));

        // Very small progress
        let mut progress = DownloadProgress::new(Some(1_000_000));
        progress.update(1, 100.0);
        let percentage = progress.percentage().unwrap();
        assert!(percentage > 0.0 && percentage < 1.0);
    }

    #[test]
    fn test_download_progress_eta() {
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, 100.0);
        assert_eq!(progress.estimated_time_remaining(), Some(5));
    }

    #[test]
    fn test_download_progress_eta_edge_cases() {
        // Zero speed
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, 0.0);
        assert_eq!(progress.estimated_time_remaining(), None);

        // Negative speed (clamped to 0)
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, -10.0);
        assert_eq!(progress.bytes_per_second(), 0.0);
        assert_eq!(progress.estimated_time_remaining(), None);

        // Unknown total
        let mut progress = DownloadProgress::new(None);
        progress.update(500, 100.0);
        assert_eq!(progress.estimated_time_remaining(), None);

        // Complete download
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(1000, 100.0);
        assert_eq!(progress.estimated_time_remaining(), Some(0));

        // Over complete
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(1200, 100.0);
        assert_eq!(progress.estimated_time_remaining(), Some(0));

        // Very slow speed
        let mut progress = DownloadProgress::new(Some(1_000_000_000));
        progress.update(0, 0.001);
        let eta = progress.estimated_time_remaining().unwrap();
        assert!(eta > 0);

        // Rounding test
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, 123.0);
        let eta = progress.estimated_time_remaining().unwrap();
        assert_eq!(eta, 4);
    }

    #[test]
    fn test_download_progress_is_complete() {
        let mut progress = DownloadProgress::new(Some(1000));
        assert!(!progress.is_complete());

        progress.update(500, 100.0);
        assert!(!progress.is_complete());

        progress.update(1000, 100.0);
        assert!(progress.is_complete());

        progress.update(1200, 100.0);
        assert!(progress.is_complete());

        let progress_unknown = DownloadProgress::new(None);
        assert!(!progress_unknown.is_complete());
    }

    #[test]
    fn test_checksum_validation() {
        let valid_sha256 = "a".repeat(64);
        let checksum = Checksum::new(ChecksumAlgorithm::Sha256, valid_sha256.clone());
        assert!(checksum.is_ok());

        let invalid_length = "abc123";
        let checksum = Checksum::new(ChecksumAlgorithm::Sha256, invalid_length.to_string());
        assert!(checksum.is_err());

        let invalid_chars = "z".repeat(64);
        let checksum = Checksum::new(ChecksumAlgorithm::Sha256, invalid_chars);
        assert!(checksum.is_err());
    }

    #[test]
    fn test_checksum_verification() {
        let value = "a".repeat(64);
        let checksum = Checksum::new(ChecksumAlgorithm::Sha256, value.clone()).unwrap();

        assert!(checksum.verify(&value).is_ok());
        assert!(checksum.verify(&value.to_uppercase()).is_ok());

        let wrong_value = "b".repeat(64);
        assert!(checksum.verify(&wrong_value).is_err());
    }

    #[test]
    fn test_download_session_creation() {
        let session = DownloadSession::new(
            "test-id".to_string(),
            "https://example.com/file.bin".to_string(),
            temp_path("file.bin"),
            Some(1000),
            None,
        );

        assert!(session.is_ok());
        let session = session.unwrap();
        assert_eq!(session.state(), &DownloadState::Pending);
        assert_eq!(session.retry_count(), 0);
    }

    #[test]
    fn test_download_session_invalid_url() {
        let session = DownloadSession::new(
            "test-id".to_string(),
            "invalid-url".to_string(),
            temp_path("file.bin"),
            Some(1000),
            None,
        );

        assert!(session.is_err());
    }

    #[test]
    fn test_download_session_lifecycle() {
        let mut session = DownloadSession::new(
            "test-id".to_string(),
            "https://example.com/file.bin".to_string(),
            temp_path("file.bin"),
            Some(1000),
            None,
        )
        .unwrap();

        assert!(session.start().is_ok());
        assert_eq!(session.state(), &DownloadState::Downloading);
        assert!(session.started_at().is_some());

        session.update_progress(500, 100.0);
        assert_eq!(session.progress().bytes_downloaded(), 500);

        assert!(session.pause().is_ok());
        assert_eq!(session.state(), &DownloadState::Paused);

        assert!(session.resume().is_ok());
        assert_eq!(session.state(), &DownloadState::Downloading);

        assert!(session.complete().is_ok());
        assert_eq!(session.state(), &DownloadState::Completed);
        assert!(session.completed_at().is_some());
    }

    #[test]
    fn test_download_session_retry_logic() {
        let mut session = DownloadSession::new(
            "test-id".to_string(),
            "https://example.com/file.bin".to_string(),
            temp_path("file.bin"),
            Some(1000),
            None,
        )
        .unwrap();

        session.start().unwrap();
        session.fail("Network error".to_string()).unwrap();

        assert!(session.can_retry());
        assert_eq!(session.retry_count(), 1);

        session.resume().unwrap();
        session.fail("Network error".to_string()).unwrap();
        assert_eq!(session.retry_count(), 2);

        session.resume().unwrap();
        session.fail("Network error".to_string()).unwrap();
        assert_eq!(session.retry_count(), 3);

        assert!(!session.can_retry());
    }

    #[test]
    fn test_retry_resets_progress() {
        let mut session = DownloadSession::new(
            "test-id".to_string(),
            "https://example.com/file.bin".to_string(),
            temp_path("file.bin"),
            Some(1000),
            None,
        )
        .unwrap();

        // Start download and make progress
        session.start().unwrap();
        session.update_progress(500, 100.0);
        assert_eq!(session.progress().bytes_downloaded(), 500);

        // Fail the download (e.g., 0 bytes downloaded, file cleaned up)
        session
            .fail("Validation failed: 0 bytes".to_string())
            .unwrap();
        assert_eq!(session.state(), &DownloadState::Failed);

        // Retry should reset progress to 0 (since file was deleted)
        session.retry().unwrap();
        assert_eq!(session.progress().bytes_downloaded(), 0);
        assert_eq!(session.state(), &DownloadState::Pending);
        assert!(session.error_message().is_none());

        // Progress should still know total_bytes
        assert_eq!(session.progress().total_bytes(), Some(1000));
    }

    #[test]
    fn test_download_session_cancel() {
        let mut session = DownloadSession::new(
            "test-id".to_string(),
            "https://example.com/file.bin".to_string(),
            temp_path("file.bin"),
            Some(1000),
            None,
        )
        .unwrap();

        session.start().unwrap();
        assert!(session.cancel().is_ok());
        assert_eq!(session.state(), &DownloadState::Cancelled);

        assert!(session.resume().is_err());
    }

    // ============================================================================
    // Domain Calculation Tests (Tests 31-33)
    // ============================================================================

    #[test]
    fn test_progress_percentage_calculation() {
        // Arrange: Create progress with known values
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(750, 0.0);

        // Act: Calculate progress percentage
        let percentage = progress.percentage();

        // Assert: Should be 75%
        assert_eq!(percentage, Some(75.0), "750 of 1000 bytes should be 75%");
    }

    #[test]
    fn test_progress_percentage_clamped_to_100() {
        // Arrange: Create progress with bytes > total (edge case)
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(1200, 0.0); // More than total

        // Act: Calculate progress percentage
        let percentage = progress.percentage();

        // Assert: Should be clamped to 100%
        assert_eq!(percentage, Some(100.0), "Progress should never exceed 100%");
    }

    #[test]
    fn test_eta_calculation() {
        // Arrange: Create progress with known progress and speed
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(250, 250.0); // 250 bytes per second

        // Act: Calculate ETA
        let eta_seconds = progress.estimated_time_remaining();

        // Assert: Should be approximately 3 seconds
        // Remaining: 1000 - 250 = 750 bytes
        // Speed: 250 bytes/sec
        // ETA: 750 / 250 = 3 seconds
        assert!(eta_seconds.is_some(), "ETA should be calculable");
        let eta = eta_seconds.unwrap();
        assert!(
            eta >= 2 && eta <= 4,
            "ETA should be approximately 3 seconds, got {}",
            eta
        );
    }

    // ============================================================================
    // Domain Edge Case Tests (Tests 34-37)
    // ============================================================================

    #[test]
    fn test_eta_calculation_zero_speed() {
        // Arrange: Create progress with zero speed
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(250, 0.0); // Zero speed - stalled download

        // Act: Calculate ETA
        let eta = progress.estimated_time_remaining();

        // Assert: Should return None (cannot calculate ETA with zero speed)
        assert!(
            eta.is_none(),
            "ETA should be None when speed is zero (stalled download)"
        );
    }

    #[test]
    fn test_eta_calculation_completed() {
        // Arrange: Create completed download (all bytes downloaded)
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(1000, 250.0);

        // Act: Calculate ETA
        let eta = progress.estimated_time_remaining();

        // Assert: Should return Some(0) (download already complete)
        assert!(
            eta == Some(0),
            "ETA should be 0 for completed download, got {:?}",
            eta
        );
    }

    #[test]
    fn test_speed_calculation() {
        // Arrange: Create progress with different speeds
        let test_cases = vec![
            (1000.0, "1 KB/s"),
            (1_000_000.0, "1 MB/s"),
            (500.5, "500.5 B/s"),
        ];

        for (speed_bps, expected_desc) in test_cases {
            let mut progress = DownloadProgress::new(Some(1000));
            progress.update(500, speed_bps);

            // Act: Get speed
            let actual_speed = progress.bytes_per_second();

            // Assert: Speed matches expected
            assert_eq!(
                actual_speed, speed_bps,
                "Speed should be {} ({})",
                speed_bps, expected_desc
            );
        }
    }

    #[test]
    fn test_speed_clamped_to_zero() {
        // Arrange: Create progress with negative speed (invalid)
        let mut progress = DownloadProgress::new(Some(1000));
        progress.update(500, -100.0); // Negative speed (should be clamped)

        // Act: Get speed
        let actual_speed = progress.bytes_per_second();

        // Assert: Speed should be clamped to 0.0 (DownloadProgress.update() clamps to max(0.0))
        assert_eq!(actual_speed, 0.0, "Negative speed should be clamped to 0.0");
    }
}

/// Represents the outcome of a model download operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum DownloadOperationState {
    /// Download initiated successfully
    DownloadStarted {
        total_size_bytes: u64,
        files_to_download: usize,
    },

    /// Model already exists and passed verification
    AlreadyDownloaded {
        verified_files: usize,
        total_size_bytes: u64,
    },

    /// Network connectivity issues
    NetworkError { error_message: String },

    /// Generic error for unexpected failures
    OperationFailed { error_message: String },
}

impl DownloadOperationState {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            DownloadOperationState::DownloadStarted { .. }
                | DownloadOperationState::AlreadyDownloaded { .. }
        )
    }

    pub fn display_message(&self) -> String {
        match self {
            DownloadOperationState::DownloadStarted {
                files_to_download, ..
            } => {
                format!("Download started ({} files)", files_to_download)
            }
            DownloadOperationState::AlreadyDownloaded { verified_files, .. } => {
                format!(
                    "Model already downloaded ({} files verified)",
                    verified_files
                )
            }
            DownloadOperationState::NetworkError { error_message } => {
                format!("Network error: {}", error_message)
            }
            DownloadOperationState::OperationFailed { error_message } => {
                format!("Operation failed: {}", error_message)
            }
        }
    }
}

// ============================================================================
// MODULE 1: DOMAIN STATE SNAPSHOT MODELS
// ============================================================================

/// Download status enum matching TypeScript discriminated union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Error,
    Cancelled,
}

impl DownloadStatus {
    /// Convert from DownloadState domain model
    pub fn from_state(state: &DownloadState) -> Self {
        match state {
            DownloadState::Pending => DownloadStatus::Pending,
            DownloadState::Downloading => DownloadStatus::Downloading,
            DownloadState::Paused => DownloadStatus::Paused,
            DownloadState::Completed => DownloadStatus::Completed,
            DownloadState::Failed => DownloadStatus::Error,
            DownloadState::Cancelled => DownloadStatus::Cancelled,
        }
    }
}

/// Per-file status for multi-file downloads
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Pending,
    Downloading,
    Completed,
    Error,
}

/// Per-file progress information for batch downloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProgress {
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub status: FileStatus,
}

/// Single file download snapshot (legacy-compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub struct SingleFileSnapshot {
    pub kind: String, // Always "single"
    pub id: String,
    pub url: String,
    pub destination: String,
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: f64,
    pub percentage: Option<f64>,
    pub eta_seconds: Option<u64>,
    pub status: DownloadStatus,
}

impl SingleFileSnapshot {
    /// Create snapshot from DownloadSession domain model
    pub fn from_session(session: &DownloadSession) -> Self {
        let filename = session
            .destination()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Self {
            kind: "single".to_string(),
            id: session.id().to_string(),
            url: session.url().to_string(),
            destination: session.destination().to_string_lossy().to_string(),
            filename,
            bytes_downloaded: session.progress().bytes_downloaded(),
            total_bytes: session.progress().total_bytes(),
            bytes_per_second: session.progress().bytes_per_second(),
            percentage: session.progress().percentage(),
            eta_seconds: session.progress().estimated_time_remaining(),
            status: DownloadStatus::from_state(session.state()),
        }
    }
}

/// Multi-file download snapshot (for LLM models)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub struct BatchDownloadSnapshot {
    pub kind: String, // Always "batch"
    pub id: String,
    pub group_name: String,
    pub files: Vec<FileProgress>,
    pub total_files: usize,
    pub completed_files: usize,
    pub aggregate_bytes_downloaded: u64,
    pub aggregate_total_bytes: u64,
    pub aggregate_bytes_per_second: f64,
    pub aggregate_percentage: f64,
    pub aggregate_eta_seconds: Option<u64>,
    pub status: DownloadStatus,
}

/// Discriminated union of download snapshot types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DownloadStateSnapshot {
    #[serde(rename = "single")]
    Single(SingleFileSnapshot),
    #[serde(rename = "batch")]
    Batch(BatchDownloadSnapshot),
}
