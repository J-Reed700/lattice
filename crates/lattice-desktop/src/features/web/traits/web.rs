//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use async_trait::async_trait;
use lattice_core::shared::error::Result;
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
