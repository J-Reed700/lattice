//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::features::function_calling::dto::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput,
};
use crate::shared::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of web URL ingestion operation
///
/// Contains details about the ingested article and processing statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebIngestionResult {
    /// Document ID assigned to the ingested article
    pub document_id: String,

    /// Original URL of the article
    pub url: String,

    /// Article title
    pub title: String,

    /// Word count of article text
    pub word_count: usize,

    /// Number of text chunks created
    pub chunks_created: usize,

    /// Site name (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,

    /// Article author (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Estimated reading time in minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_time_minutes: Option<i64>,
}

#[async_trait]
pub trait WebIngestionServiceTrait: Send + Sync {
    /// Ingest content from a web URL
    ///
    /// This method performs the complete web ingestion pipeline:
    /// 1. Fetches web content using WebFetcher
    /// 2. Extracts and cleans text content
    /// 3. Chunks content using semantic chunking
    /// 4. Generates embeddings for all chunks
    /// 5. Stores document, chunks, and embeddings atomically in database
    ///
    /// # Arguments
    /// * `url` - The URL to ingest (must be valid HTTP/HTTPS URL)
    ///
    /// # Returns
    /// WebIngestResult containing:
    /// - document_id: Unique identifier for the ingested document
    /// - url: The ingested URL
    /// - title: Extracted page title
    /// - word_count: Total word count
    /// - chunks: Number of chunks created
    /// - site_name: Optional site name (from Open Graph)
    /// - author: Optional author name
    /// - reading_time_minutes: Estimated reading time
    ///
    /// # Errors
    /// - `AppError::Network` if URL fetch fails
    /// - `AppError::Other` if content extraction fails
    /// - `AppError::EmbeddingFailed` if embedding generation fails
    /// - `AppError::Database` if storage fails
    ///
    /// # Example
    /// ```rust
    /// let result = service.ingest_url("https://example.com/article").await?;
    /// println!("Indexed {} with {} chunks", result.title, result.chunks_created);
    /// ```
    async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult>;
}

/// Trait for conversation management operations
///
/// Provides complete conversation lifecycle management including creating conversations,
/// managing messages, tracking context, and pruning history for token limits.
///
/// # Implementations
/// - `ConversationService`: Production implementation with SQLite repositories
/// - `MockConversationService`: In-memory mock for testing
#[async_trait]
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
