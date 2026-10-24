use async_trait::async_trait;
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

/// Result of web URL ingestion operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebIngestionResult {
    /// Document ID assigned to the ingested article.
    pub document_id: String,
    /// Original URL of the article.
    pub url: String,
    /// Article title.
    pub title: String,
    /// Word count of article text.
    pub word_count: usize,
    /// Number of text chunks created.
    pub chunks_created: usize,
    /// Site name if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    /// Article author if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Estimated reading time in minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_time_minutes: Option<i64>,
}

#[async_trait]
pub trait WebIngestionServiceTrait: Send + Sync {
    /// Ingest content from a web URL into the index.
    async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult>;
}
