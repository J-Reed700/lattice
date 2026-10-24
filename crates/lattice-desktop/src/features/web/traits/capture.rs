use async_trait::async_trait;
use lattice_core::application::contracts::web::UrlPreview;
use lattice_core::shared::error::Result;

/// URL preview and metadata extraction for web content ingestion.
///
/// Implementations must enforce SSRF protections, timeout boundaries, and size limits.
#[async_trait]
pub trait WebCaptureServiceTrait: Send + Sync {
    /// Fetch URL preview with metadata extraction.
    async fn fetch_url_preview(&self, url: &str) -> Result<UrlPreview>;
}
