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
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::infrastructure::model_cache_adapter::ModelCacheAdapter;
//! use lattice::infrastructure::huggingface_adapter::HuggingFaceAdapter;
//! use lattice::application::ports::ModelCatalogPort;
//! use sqlx::SqlitePool;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pool = SqlitePool::connect("sqlite::memory:").await?;
//!     let huggingface = Arc::new(HuggingFaceAdapter::new());
//!     let cached = ModelCacheAdapter::new(pool, huggingface).await?;
//!
//!     // First call: cache miss, hits API
//!     let models = cached.search_models("llama", 10).await?;
//!     println!("Found {} models (cache miss)", models.len());
//!
//!     // Second call: cache hit, no API call
//!     let models = cached.search_models("llama", 10).await?;
//!     println!("Found {} models (cache hit)", models.len());
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::application::ports::{ExternalModelMetadata, ModelCatalogPort};
use crate::shared::error::AppError;

/// Cache-key schema version.
///
/// Bump this when cached payload semantics change to avoid stale result reuse.
const CACHE_KEY_VERSION: &str = "v4";

/// Cache entry for model catalog queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    query: String,
    results: Vec<ExternalModelMetadata>,
    cached_at: i64,
    expires_at: i64,
}

/// Model catalog cache adapter.
///
/// Wraps a `ModelCatalogPort` with SQLite-based caching.
pub struct ModelCacheAdapter {
    pool: SqlitePool,
    upstream: Arc<dyn ModelCatalogPort>,
    ttl_seconds: i64,
}

impl ModelCacheAdapter {
    /// Create a new cache adapter.
    ///
    /// # Arguments
    /// - `pool` - SQLite connection pool
    /// - `upstream` - Upstream model catalog port (e.g., HuggingFaceAdapter)
    ///
    /// # Returns
    /// Cache adapter or error.
    pub async fn new(
        pool: SqlitePool,
        upstream: Arc<dyn ModelCatalogPort>,
    ) -> Result<Self, AppError> {
        let adapter = Self {
            pool,
            upstream,
            ttl_seconds: 3600, // 1 hour
        };

        adapter.initialize_schema().await?;

        Ok(adapter)
    }

    /// Create with custom TTL.
    ///
    /// # Arguments
    /// - `pool` - SQLite connection pool
    /// - `upstream` - Upstream model catalog port
    /// - `ttl_seconds` - Cache TTL in seconds
    pub async fn with_ttl(
        pool: SqlitePool,
        upstream: Arc<dyn ModelCatalogPort>,
        ttl_seconds: i64,
    ) -> Result<Self, AppError> {
        let adapter = Self {
            pool,
            upstream,
            ttl_seconds,
        };

        adapter.initialize_schema().await?;

        Ok(adapter)
    }

    /// Initialize cache schema.
    async fn initialize_schema(&self) -> Result<(), AppError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS model_catalog_cache (
                query TEXT PRIMARY KEY,
                results_json TEXT NOT NULL,
                cached_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_model_catalog_cache_expires
            ON model_catalog_cache(expires_at);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to initialize cache schema: {}", e)))?;

        Ok(())
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

        let row = sqlx::query(
            r#"
            SELECT results_json, expires_at
            FROM model_catalog_cache
            WHERE query = ? AND expires_at > ?
            "#,
        )
        .bind(&cache_key)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Cache lookup failed: {}", e)))?;

        if let Some(row) = row {
            let results_json: String = row.get("results_json");
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

        sqlx::query(
            r#"
            INSERT INTO model_catalog_cache (query, results_json, cached_at, expires_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(query) DO UPDATE SET
                results_json = excluded.results_json,
                cached_at = excluded.cached_at,
                expires_at = excluded.expires_at
            "#,
        )
        .bind(&cache_key)
        .bind(&results_json)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to store cache: {}", e)))?;

        tracing::debug!("Cached results for query: {}", cache_key);
        Ok(())
    }

    /// Clear expired cache entries.
    pub async fn clear_expired(&self) -> Result<u64, AppError> {
        let now = Self::now();

        let result = sqlx::query(
            r#"
            DELETE FROM model_catalog_cache
            WHERE expires_at <= ?
            "#,
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to clear expired cache: {}", e)))?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            tracing::info!("Cleared {} expired cache entries", deleted);
        }

        Ok(deleted)
    }

    /// Clear all cache entries.
    pub async fn clear_all(&self) -> Result<u64, AppError> {
        let result = sqlx::query("DELETE FROM model_catalog_cache")
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to clear cache: {}", e)))?;

        let deleted = result.rows_affected();
        tracing::info!("Cleared {} cache entries", deleted);

        Ok(deleted)
    }

    /// Get cache statistics.
    pub async fn get_stats(&self) -> Result<ModelCatalogStats, AppError> {
        let now = Self::now();

        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_entries,
                SUM(CASE WHEN expires_at > ? THEN 1 ELSE 0 END) as valid_entries,
                SUM(CASE WHEN expires_at <= ? THEN 1 ELSE 0 END) as expired_entries
            FROM model_catalog_cache
            "#,
        )
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get cache stats: {}", e)))?;

        Ok(ModelCatalogStats {
            total_entries: row.get::<i64, _>("total_entries") as u64,
            valid_entries: row.get::<i64, _>("valid_entries") as u64,
            expired_entries: row.get::<i64, _>("expired_entries") as u64,
        })
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

        // Store in cache (best effort, don't fail if cache write fails)
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
    use crate::application::ports::MockModelCatalogPort;
    use tokio::time::Duration;

    async fn create_test_cache() -> ModelCacheAdapter {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        let mock = Arc::new(MockModelCatalogPort::new());
        ModelCacheAdapter::new(pool, mock as Arc<dyn ModelCatalogPort>)
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
        assert_eq!(normalized, "v4:llama:10");
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

        // Add some cache entries
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

        // Add entries
        cache.search_models("llama", 5).await.unwrap();
        cache.search_models("phi", 5).await.unwrap();

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.valid_entries, 2);
        assert_eq!(stats.expired_entries, 0);
    }

    #[tokio::test]
    async fn test_clear_expired() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        let mock = Arc::new(MockModelCatalogPort::new());

        // Create cache with 1-second TTL
        let cache = ModelCacheAdapter::with_ttl(pool, mock as Arc<dyn ModelCatalogPort>, 1)
            .await
            .expect("Failed to create cache");

        // Add entry
        cache.search_models("llama", 5).await.unwrap();

        let stats = cache.get_stats().await.unwrap();
        assert_eq!(stats.valid_entries, 1);

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
