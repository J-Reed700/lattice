//! Web capture service trait
//!
//! Provides URL preview and metadata extraction for web content ingestion.

use async_trait::async_trait;
use lattice_core::application::contracts::web::UrlPreview;
use lattice_core::shared::error::Result;

/// Web capture: URL previews with their metadata.
#[async_trait]
pub trait WebCaptureServiceTrait: Send + Sync {
    /// Downloads a page and extracts its metadata (title, description, language,
    /// Open Graph and more).
    ///
    /// # Errors
    /// - `AppError::InvalidUrl` if URL validation fails (SSRF prevention)
    /// - `AppError::Network` if HTTP request fails or times out
    /// - `AppError::Other` if HTML parsing or metadata extraction fails
    async fn fetch_url_preview(&self, url: &str) -> Result<UrlPreview>;
}
