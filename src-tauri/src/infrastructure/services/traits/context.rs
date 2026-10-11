//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::shared::error::Result;
use async_trait::async_trait;

/// Trait for enriching search results with document metadata
#[async_trait]
pub trait SearchEnrichmentServiceTrait: Send + Sync {
    /// Enrich search results with document metadata
    ///
    /// Given chunk IDs from search results, fetches associated metadata including
    /// document titles, file paths, creation dates, and content snippets.
    ///
    /// # Performance
    /// - Processes chunks in batches of 900 (SQLite parameter limit is 999)
    /// - Uses parallel batch processing with concurrency=4
    /// - Expected speedup: 3-4x over sequential processing
    ///
    /// # Arguments
    /// * `chunk_ids` - Slice of chunk IDs to enrich (can be any size)
    ///
    /// # Returns
    /// HashMap mapping chunk ID to DocumentMetadata containing:
    /// - snippet: Content preview (max 200 chars)
    /// - metadata: Hash map with filename, file_type, file_size, created_at, updated_at
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    ///
    /// # Example
    /// ```rust
    /// let chunk_ids: Vec<String> = search_results.iter().map(|r| r.id.clone()).collect();
    /// let enriched = service.enrich_results(&chunk_ids).await?;
    ///
    /// for result in search_results {
    ///     if let Some(metadata) = enriched.get(&result.id) {
    ///         println!("Snippet: {}", metadata.snippet);
    ///     }
    /// }
    /// ```
    async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<
        std::collections::HashMap<
            String,
            crate::features::search::enrichment_service::DocumentMetadata,
        >,
    >;
}
