//! Web search and page reads, as the model's tools and course research use
//! them.

use async_trait::async_trait;

use crate::application::contracts::web::{
    FetchUrlContentOutput, ProvenanceSignals, ReferencePage, WebSearchInput, WebSearchOutput,
};
use crate::shared::error::Result;

/// Why a fetcher without a metadata reader returns no provenance signals.
pub const NO_PROVENANCE_READER: &str = "this fetcher does not extract provenance";

/// Searches the web and reads pages behind the SSRF checks.
///
/// # Implementations
/// - `WebService`: production implementation with a shared page cache
/// - `MockWebService`: canned results for testing
#[async_trait]
pub trait WebServiceTrait: Send + Sync {
    /// Search the web using configured public providers (DuckDuckGo/Bing/Wikipedia)
    ///
    /// # Arguments
    /// * `input` - Search query/options including pagination and provider hints
    ///
    /// # Returns
    /// Web search output with results, pagination metadata, and provider details
    ///
    /// # Errors
    /// Returns error if search fails or timeout occurs
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput>;

    /// Fetch and extract content from a URL
    ///
    /// # Arguments
    /// * `url` - URL to fetch
    ///
    /// # Returns
    /// Extracted web content with text and metadata
    ///
    /// # Errors
    /// Returns error if:
    /// - URL is invalid or blocked (SSRF prevention)
    /// - Request times out
    /// - Content extraction fails
    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput>;

    /// Read a larger page for an immutable reference collection. Implementations
    /// retain URL/DNS/redirect checks and must report any text truncation.
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetch_url_content(url).await
    }

    /// Read a page for a reference collection together with its provenance
    /// signals (source provenance §4). A fetcher that reads no markup, such
    /// as a test fake, says so in the signals rather than inventing any.
    async fn fetch_reference_page(&self, url: &str) -> Result<ReferencePage> {
        let output = self.fetch_reference_content(url).await?;
        Ok(ReferencePage {
            output,
            signals: ProvenanceSignals::NotExtracted {
                reason: NO_PROVENANCE_READER.to_string(),
            },
            fetched_at: chrono::Utc::now().timestamp(),
        })
    }

    /// Validate URL for security (SSRF prevention)
    ///
    /// Blocks:
    /// - localhost and loopback addresses
    /// - Private IP ranges (RFC 1918)
    /// - Link-local addresses
    /// - Metadata endpoints (169.254.169.254)
    ///
    /// # Arguments
    /// * `url` - URL to validate
    ///
    /// # Returns
    /// Ok if URL is safe, Err if blocked
    fn validate_url(&self, url: &str) -> Result<()>;
}
