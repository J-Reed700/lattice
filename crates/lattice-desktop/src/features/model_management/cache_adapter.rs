//! # Model Catalog Cache Adapter
//!
//! Caching layer for external model catalog queries.
//!
//! ## Architecture
//!
//! Wraps `ModelCatalogPort` with SQLite-based caching to reduce API calls.
//!
//! ## Features
//!
//! - **SQLite Storage**: Persistent cache across application restarts
//! - **TTL Management**: 1-hour expiration for cached results
//! - **Query Normalization**: Case-insensitive query matching
//! - **Cache Hit Metrics**: Track cache effectiveness
//!
//! ## Schema
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS model_catalog_cache (
//!     query TEXT PRIMARY KEY,
//!     results_json TEXT NOT NULL,
//!     cached_at INTEGER NOT NULL,  -- Unix timestamp
//!     expires_at INTEGER NOT NULL  -- Unix timestamp
//! );
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lattice_core::application::ports::{ExternalModelMetadata, ModelCatalogPort};
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;

/// Cache-key schema version.
///
/// Bump this when cached payload semantics change to avoid stale result reuse.
const CACHE_KEY_VERSION: &str = "v6";

/// Model catalog cache adapter.
///
/// Wraps a `ModelCatalogPort` with SQLite-based caching.
pub struct ModelCacheAdapter {
    database: Database,
    upstream: Arc<dyn ModelCatalogPort>,
    ttl_seconds: i64,
}

impl ModelCacheAdapter {
    /// Create a new cache adapter.
    pub async fn new(
        database: Database,
        upstream: Arc<dyn ModelCatalogPort>,
    ) -> Result<Self, AppError> {
        let adapter = Self {
            database,
            upstream,
            ttl_seconds: 3600, // 1 hour
        };

        adapter.initialize_schema().await?;

        Ok(adapter)
    }

    /// Create with custom TTL.
    pub async fn with_ttl(
        database: Database,
        upstream: Arc<dyn ModelCatalogPort>,
        ttl_seconds: i64,
    ) -> Result<Self, AppError> {
        let adapter = Self {
            database,
            upstream,
            ttl_seconds,
        };

        adapter.initialize_schema().await?;

        Ok(adapter)
    }

    /// Initialize cache schema.
    async fn initialize_schema(&self) -> Result<(), AppError> {
        super::repository::initialize_cache_schema(&self.database).await
    }

    /// Get current Unix timestamp.
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    /// Normalize query for cache key.
    fn normalize_query(query: &str, limit: usize) -> String {
        format!(
            "{}:{}:{}",
            CACHE_KEY_VERSION,
            query.to_lowercase().trim(),
            limit
        )
    }

    /// Get cached results.
    async fn get_cached(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Option<Vec<ExternalModelMetadata>>, AppError> {
        let cache_key = Self::normalize_query(query, limit);
        let now = Self::now();

        let row = super::repository::cached_results_json(&self.database, &cache_key, now).await?;

        if let Some(results_json) = row {
            let results: Vec<ExternalModelMetadata> =
                serde_json::from_str(&results_json).map_err(|e| {
                    AppError::Serialization(format!("Failed to deserialize cache: {}", e))
                })?;

            tracing::debug!("Cache hit for query: {}", cache_key);
            Ok(Some(results))
        } else {
            tracing::debug!("Cache miss for query: {}", cache_key);
            Ok(None)
        }
    }

    /// Store results in cache.
    async fn store_cached(
        &self,
        query: &str,
        limit: usize,
        results: &[ExternalModelMetadata],
    ) -> Result<(), AppError> {
        let cache_key = Self::normalize_query(query, limit);
        let now = Self::now();
        let expires_at = now + self.ttl_seconds;

        let results_json = serde_json::to_string(results)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize results: {}", e)))?;

        super::repository::store_results_json(
            &self.database,
            &cache_key,
            &results_json,
            now,
            expires_at,
        )
        .await?;

        tracing::debug!("Cached results for query: {}", cache_key);
        Ok(())
    }

    /// Clear expired cache entries.
    pub async fn clear_expired(&self) -> Result<u64, AppError> {
        let now = Self::now();

        let deleted = super::repository::delete_expired(&self.database, now).await?;
        if deleted > 0 {
            tracing::info!("Cleared {} expired cache entries", deleted);
        }

        Ok(deleted)
    }

    /// Clear all cache entries.
    pub async fn clear_all(&self) -> Result<u64, AppError> {
        let deleted = super::repository::delete_all(&self.database).await?;
        tracing::info!("Cleared {} cache entries", deleted);

        Ok(deleted)
    }

    /// Get cache statistics.
    pub async fn get_stats(&self) -> Result<ModelCatalogStats, AppError> {
        let now = Self::now();

        super::repository::cache_stats(&self.database, now).await
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ModelCatalogStats {
    pub total_entries: u64,
    pub valid_entries: u64,
    pub expired_entries: u64,
}

#[async_trait]
impl ModelCatalogPort for ModelCacheAdapter {
    async fn get_model_variants(
        &self,
        model_id: &str,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        // A repository's default-file cache cannot answer a variant listing.
        self.upstream.get_model_variants(model_id).await
    }

    async fn search_models(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ExternalModelMetadata>, AppError> {
        // Try cache first
        if let Some(cached) = self.get_cached(query, limit).await? {
            if !cached.is_empty() {
                return Ok(cached);
            }

            // Empty cache entries can be produced by transient upstream failures.
            // Revalidate instead of serving empty data for the full TTL window.
            tracing::warn!(
                "Empty cache entry for query='{}' limit={} - revalidating upstream",
                query,
                limit
            );
        }

        // Cache miss (or empty cache): fetch from upstream
        let results = self.upstream.search_models(query, limit).await?;

        // Avoid persisting empty responses to prevent cache poisoning.
        if results.is_empty() {
            tracing::warn!(
                "Skipping cache store for empty upstream result query='{}' limit={}",
                query,
                limit
            );
            return Ok(results);
        }

        if let Err(e) = self.store_cached(query, limit, &results).await {
            tracing::warn!("Failed to cache results: {}", e);
        }

        Ok(results)
    }

    async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<ExternalModelMetadata>, AppError> {
        // Model ID lookups are not cached (less common, more specific)
        self.upstream.get_model_by_id(model_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::MockModelCatalogPort;
    use sqlx::SqlitePool;
    use tokio::time::Duration;

    async fn create_test_cache() -> ModelCacheAdapter {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        let mock = Arc::new(MockModelCatalogPort::new());
        ModelCacheAdapter::new(Database::new(pool), mock as Arc<dyn ModelCatalogPort>)
            .await
            .expect("Failed to create cache adapter")
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = create_test_cache().await;
        assert_eq!(cache.ttl_seconds, 3600);
    }

    #[tokio::test]
    async fn test_normalize_query() {
        let normalized = ModelCacheAdapter::normalize_query("  LLaMa  ", 10);
        assert_eq!(normalized, format!("{CACHE_KEY_VERSION}:llama:10"));
    }

    #[tokio::test]

    async fn test_cache_miss_and_hit() {
        let cache = create_test_cache().await;

        // First call: cache miss (MockModelCatalogPort has 3 llama models)
        let results1 = cache.search_models("llama", 5).await.unwrap();
        assert_eq!(results1.len(), 3); // tinyllama, llama-3.2-3b, llama-3.2-7b

        // Second call: cache hit
        let results2 = cache.search_models("llama", 5).await.unwrap();
        assert_eq!(results2.len(), 3);
        assert_eq!(results1[0].id, results2[0].id);
    }

    #[tokio::test]

    async fn test_cache_with_different_queries() {
        let cache = create_test_cache().await;

        let results1 = cache.search_models("llama", 5).await.unwrap();
        let results2 = cache.search_models("phi", 5).await.unwrap();

        assert_eq!(results1.len(), 3); // MockModelCatalogPort has 3 llama models
        assert_eq!(results2.len(), 1); // MockModelCatalogPort has 1 phi model
        assert_ne!(results1[0].id, results2[0].id);
    }

    #[tokio::test]

    async fn test_cache_with_different_limits() {
        let cache = create_test_cache().await;

        // Different limits should be different cache entries
        let results1 = cache.search_models("llama", 5).await.unwrap();
        let results2 = cache.search_models("llama", 10).await.unwrap();

        assert_eq!(results1.len(), 3); // MockModelCatalogPort has 3 llama models
        assert_eq!(results2.len(), 3); // Same 3 models with different limit
    }

    #[tokio::test]
    async fn test_empty_results_not_cached() {
        let cache = create_test_cache().await;

        let results = cache
            .search_models("this-query-will-not-match", 5)
            .await
            .unwrap();
        assert!(results.is_empty());

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_clear_all() {
        let cache = create_test_cache().await;

        cache.search_models("llama", 5).await.unwrap();
        cache.search_models("phi", 5).await.unwrap();

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 2);

        // Clear all
        let deleted = cache.clear_all().await.unwrap();
        assert_eq!(deleted, 2);

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = create_test_cache().await;

        // Initially empty
        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.valid_entries, 0);
        assert_eq!(stats.expired_entries, 0);

        cache.search_models("llama", 5).await.unwrap();
        cache.search_models("phi", 5).await.unwrap();

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.valid_entries, 2);
        assert_eq!(stats.expired_entries, 0);
    }

    /// Builds a cache over its own in-memory database with the given TTL.
    async fn cache_with_ttl(ttl_seconds: i64) -> ModelCacheAdapter {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");
        let mock = Arc::new(MockModelCatalogPort::new());
        ModelCacheAdapter::with_ttl(
            Database::new(pool),
            mock as Arc<dyn ModelCatalogPort>,
            ttl_seconds,
        )
        .await
        .expect("Failed to create cache")
    }

    /// A fresh entry counts as valid.
    ///
    /// Split out of `test_clear_expired`, which used a one-second TTL and then
    /// asserted the entry was *still valid* before sleeping. That is a race the
    /// test cannot win: it only holds if fewer than a second passes between the
    /// write and the read, and on a loaded CI runner it does not, which is how
    /// this failed on Windows with `left: 0, right: 1`. Nothing here needs a
    /// short TTL, so the window is made far larger than any scheduling stall
    /// instead of being tuned against one.
    #[tokio::test]
    async fn a_fresh_entry_is_valid() {
        let cache = cache_with_ttl(3600).await;

        cache.search_models("llama", 5).await.unwrap();

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.valid_entries, 1);
        assert_eq!(stats.expired_entries, 0);
    }

    #[tokio::test]
    async fn test_clear_expired() {
        // A short TTL is only needed for the half of this that waits, and here
        // the wait is safe in one direction: a slow runner can only make the
        // entry more expired, never less.
        let cache = cache_with_ttl(1).await;

        cache.search_models("llama", 5).await.unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Clear expired
        let deleted = cache.clear_expired().await.unwrap();
        assert_eq!(deleted, 1);

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.valid_entries, 0);
        assert_eq!(stats.expired_entries, 0);
    }

    #[tokio::test]
    async fn test_get_model_by_id_not_cached() {
        let cache = create_test_cache().await;

        // get_model_by_id should not be cached (use ID from MockModelCatalogPort)
        let model1 = cache.get_model_by_id("llama-3.2-7b").await.unwrap();
        let model2 = cache.get_model_by_id("llama-3.2-7b").await.unwrap();

        assert!(model1.is_some());
        assert!(model2.is_some());

        // Cache stats should be 0 (no caching for get_model_by_id)
        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 0);
    }
}
