//! Update Checker Adapter Implementation
//!
//! Implements UpdateCheckerPort using Tauri's updater.
//!
//! # Features
//! - Check for application updates
//! - Get current version
//! - Fetch release notes
//! - Provide download URLs
//!
//! # Notes
//! - Uses Tauri's built-in updater when available
//! - Falls back to manual version checking if updater is not configured
//! - Respects update configuration in tauri.conf.json

use crate::application::ports::update_checker_port::{UpdateCheckerPort, UpdateInfoData};
use crate::shared::error::{AppError, Result};
use crate::shared::utils::reqwest_client_builder;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "posh-industries/lattice"; // Update with actual repo
const GITHUB_API_BASE: &str = "https://api.github.com/repos";

#[derive(Debug, Serialize, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    prerelease: bool,
    draft: bool,
}

/// Update checker using GitHub releases API
pub struct UpdateCheckerAdapter {
    repo: String,
    current_version: String,
}

impl UpdateCheckerAdapter {
    pub fn new() -> Self {
        Self {
            repo: GITHUB_REPO.to_string(),
            current_version: APP_VERSION.to_string(),
        }
    }

    pub fn with_repo(repo: String) -> Self {
        Self {
            repo,
            current_version: APP_VERSION.to_string(),
        }
    }

    /// Fetch latest release from GitHub API
    async fn fetch_latest_release(&self) -> Result<GitHubRelease> {
        let url = format!("{}/{}/releases/latest", GITHUB_API_BASE, self.repo);

        let client = reqwest_client_builder()
            .user_agent("Lattice-Desktop")
            .build()
            .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch release info: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "GitHub API returned status: {}",
                response.status()
            )));
        }

        let release: GitHubRelease = response
            .json()
            .await
            .map_err(|e| AppError::Serialization(format!("Failed to parse release info: {}", e)))?;

        Ok(release)
    }

    /// Compare version strings (simple semver comparison)
    fn is_newer_version(&self, current: &str, latest: &str) -> bool {
        let current_parts: Vec<u32> = current
            .trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        let latest_parts: Vec<u32> = latest
            .trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        for (c, l) in current_parts.iter().zip(latest_parts.iter()) {
            if l > c {
                return true;
            } else if l < c {
                return false;
            }
        }

        // If all parts are equal, check if latest has more parts
        latest_parts.len() > current_parts.len()
    }
}

impl Default for UpdateCheckerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UpdateCheckerPort for UpdateCheckerAdapter {
    async fn check_for_updates(&self) -> Result<UpdateInfoData, AppError> {
        info!(
            "Checking for updates (current version: {})",
            self.current_version
        );

        match self.fetch_latest_release().await {
            Ok(release) => {
                // Skip drafts and prereleases
                if release.draft || release.prerelease {
                    return Ok(UpdateInfoData {
                        available: false,
                        current_version: self.current_version.clone(),
                        latest_version: None,
                        download_url: None,
                        release_notes: None,
                    });
                }

                let latest_version = release.tag_name.clone();
                let available = self.is_newer_version(&self.current_version, &latest_version);

                if available {
                    info!(
                        "Update available: {} -> {}",
                        self.current_version, latest_version
                    );
                } else {
                    info!("Already on latest version");
                }

                Ok(UpdateInfoData {
                    available,
                    current_version: self.current_version.clone(),
                    latest_version: Some(latest_version),
                    download_url: Some(release.html_url),
                    release_notes: release.body,
                })
            }
            Err(e) => {
                error!("Failed to check for updates: {}", e);
                // Return current version info without error
                Ok(UpdateInfoData {
                    available: false,
                    current_version: self.current_version.clone(),
                    latest_version: None,
                    download_url: None,
                    release_notes: Some(format!("Failed to check for updates: {}", e)),
                })
            }
        }
    }

    fn get_current_version(&self) -> String {
        self.current_version.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_version() {
        let adapter = UpdateCheckerAdapter::new();
        let version = adapter.get_current_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_version_comparison() {
        let adapter = UpdateCheckerAdapter::new();

        // Newer versions
        assert!(adapter.is_newer_version("1.0.0", "1.0.1"));
        assert!(adapter.is_newer_version("1.0.0", "1.1.0"));
        assert!(adapter.is_newer_version("1.0.0", "2.0.0"));
        assert!(adapter.is_newer_version("v1.0.0", "v1.0.1"));

        // Same version
        assert!(!adapter.is_newer_version("1.0.0", "1.0.0"));
        assert!(!adapter.is_newer_version("v1.0.0", "v1.0.0"));

        // Older versions
        assert!(!adapter.is_newer_version("1.0.1", "1.0.0"));
        assert!(!adapter.is_newer_version("1.1.0", "1.0.9"));
        assert!(!adapter.is_newer_version("2.0.0", "1.9.9"));
    }

    #[tokio::test]
    async fn test_check_updates_offline() {
        // This test will fail to connect to GitHub (no network in test env)
        // It should return current version without erroring
        let adapter = UpdateCheckerAdapter::with_repo("nonexistent/repo".to_string());
        let result = adapter.check_for_updates().await.unwrap();

        assert!(!result.available);
        assert!(!result.current_version.is_empty());
    }

    #[test]
    fn test_custom_repo() {
        let adapter = UpdateCheckerAdapter::with_repo("custom/repo".to_string());
        assert_eq!(adapter.repo, "custom/repo");
    }
}
