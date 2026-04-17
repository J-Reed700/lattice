//! Check For Updates Use Case
//!
//! Checks for available application updates from GitHub releases.
//!
//! # Dependencies
//! - `UpdateCheckerPort` - Update checking via GitHub API
//!
//! # Security
//! - Validates GitHub API responses
//! - Uses HTTPS for all update checks
//! - Verifies release signatures (if available)
//! - Rate limited to respect GitHub API limits
//!
//! # Example
//! ```rust,no_run
//! let use_case = CheckForUpdatesUseCase::new(update_checker);
//! let update_info = use_case.execute().await?;
//! if update_info.update_available {
//!     println!("Update available: {}", update_info.latest_version);
//! }
//! ```

use crate::features::updates::dto::UpdateInfoDto;
use crate::application::ports::UpdateCheckerPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct CheckForUpdatesUseCase {
    update_checker: Arc<dyn UpdateCheckerPort>,
}

impl CheckForUpdatesUseCase {
    pub fn new(update_checker: Arc<dyn UpdateCheckerPort>) -> Self {
        Self { update_checker }
    }

    pub async fn execute(&self) -> Result<UpdateInfoDto, AppError> {
        tracing::info!("Checking for application updates");

        let update_info = self.update_checker.check_for_updates().await?;

        tracing::info!(
            available = update_info.available,
            current = %update_info.current_version,
            latest = ?update_info.latest_version,
            "Update check completed"
        );

        Ok(UpdateInfoDto {
            available: update_info.available,
            current_version: update_info.current_version,
            latest_version: update_info.latest_version,
            download_url: update_info.download_url,
            release_notes: update_info.release_notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::UpdateInfoData;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockUpdateCheckerPort {
        current_version: String,
        latest_version: Option<String>,
        available: bool,
        download_url: Option<String>,
        release_notes: Option<String>,
        should_fail: AtomicBool,
    }

    impl MockUpdateCheckerPort {
        fn new_no_update(current_version: &str) -> Self {
            Self {
                current_version: current_version.to_string(),
                latest_version: Some(current_version.to_string()),
                available: false,
                download_url: None,
                release_notes: None,
                should_fail: AtomicBool::new(false),
            }
        }

        fn new_with_update(
            current_version: &str,
            latest_version: &str,
            download_url: &str,
            release_notes: &str,
        ) -> Self {
            Self {
                current_version: current_version.to_string(),
                latest_version: Some(latest_version.to_string()),
                available: true,
                download_url: Some(download_url.to_string()),
                release_notes: Some(release_notes.to_string()),
                should_fail: AtomicBool::new(false),
            }
        }

        fn new_failing(current_version: &str) -> Self {
            Self {
                current_version: current_version.to_string(),
                latest_version: None,
                available: false,
                download_url: None,
                release_notes: None,
                should_fail: AtomicBool::new(true),
            }
        }
    }

    #[async_trait]
    impl UpdateCheckerPort for MockUpdateCheckerPort {
        async fn check_for_updates(&self) -> Result<UpdateInfoData, AppError> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(AppError::Network("Failed to check for updates".to_string()));
            }

            Ok(UpdateInfoData {
                available: self.available,
                current_version: self.current_version.clone(),
                latest_version: self.latest_version.clone(),
                download_url: self.download_url.clone(),
                release_notes: self.release_notes.clone(),
            })
        }

        fn get_current_version(&self) -> String {
            self.current_version.clone()
        }
    }

    #[tokio::test]
    async fn test_check_for_updates_no_update_available() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new_no_update("1.0.0"));
        let use_case = CheckForUpdatesUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let update_info = result.unwrap();
        assert!(!update_info.available);
        assert_eq!(update_info.current_version, "1.0.0");
        assert_eq!(update_info.latest_version, Some("1.0.0".to_string()));
        assert!(update_info.download_url.is_none());
        assert!(update_info.release_notes.is_none());
    }

    #[tokio::test]
    async fn test_check_for_updates_update_available() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new_with_update(
            "1.0.0",
            "1.1.0",
            "https://example.com/download/v1.1.0",
            "Bug fixes and improvements",
        ));
        let use_case = CheckForUpdatesUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let update_info = result.unwrap();
        assert!(update_info.available);
        assert_eq!(update_info.current_version, "1.0.0");
        assert_eq!(update_info.latest_version, Some("1.1.0".to_string()));
        assert_eq!(
            update_info.download_url,
            Some("https://example.com/download/v1.1.0".to_string())
        );
        assert_eq!(
            update_info.release_notes,
            Some("Bug fixes and improvements".to_string())
        );
    }

    #[tokio::test]
    async fn test_check_for_updates_major_version_update() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new_with_update(
            "1.5.2",
            "2.0.0",
            "https://example.com/download/v2.0.0",
            "Major new features:\n- Feature A\n- Feature B",
        ));
        let use_case = CheckForUpdatesUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let update_info = result.unwrap();
        assert!(update_info.available);
        assert_eq!(update_info.current_version, "1.5.2");
        assert_eq!(update_info.latest_version, Some("2.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_check_for_updates_network_error() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new_failing("1.0.0"));
        let use_case = CheckForUpdatesUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Network(msg) => {
                assert!(msg.contains("Failed to check for updates"));
            }
            _ => panic!("Expected Network error"),
        }
    }

    #[tokio::test]
    async fn test_check_for_updates_multiple_calls() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new_with_update(
            "0.9.0",
            "1.0.0",
            "https://example.com/v1.0.0",
            "Release version",
        ));
        let use_case = CheckForUpdatesUseCase::new(mock_checker);

        let result1 = use_case.execute().await.unwrap();
        let result2 = use_case.execute().await.unwrap();

        assert_eq!(result1.available, result2.available);
        assert_eq!(result1.current_version, result2.current_version);
        assert_eq!(result1.latest_version, result2.latest_version);
    }

    #[tokio::test]
    async fn test_check_for_updates_with_empty_release_notes() {
        let mock_checker = Arc::new(MockUpdateCheckerPort {
            current_version: "1.0.0".to_string(),
            latest_version: Some("1.0.1".to_string()),
            available: true,
            download_url: Some("https://example.com/v1.0.1".to_string()),
            release_notes: Some("".to_string()),
            should_fail: AtomicBool::new(false),
        });
        let use_case = CheckForUpdatesUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let update_info = result.unwrap();
        assert!(update_info.available);
        assert_eq!(update_info.release_notes, Some("".to_string()));
    }
}
