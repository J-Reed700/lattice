//! Search Cache Management Commands
//!
//! Thin command controllers for managing the in-memory search result cache.
//! The cache stores recent search results to avoid redundant embedding generation and
//! database queries, significantly improving search performance for repeated queries.
//!
//! # Commands
//!
//! - `clear_search_cache` - Manually clear all cached search results
//! - `get_cache_stats` - Get detailed cache statistics (size, capacity, hit rate)
//! - `get_cache_metrics` - Get cache performance metrics (hits, misses, time saved)
//!
//! # Cache Architecture
//!
//! - **Type**: In-memory LRU cache (Least Recently Used eviction)
//! - **Capacity**: Configurable (default: 100 queries)
//! - **Key**: Search query text (normalized)
//! - **Value**: Search results with embeddings and scores
//! - **Thread-Safe**: One cache per app, held by the container, with internal locking
//!
//! # Performance Benefits
//!
//! - **Embedding Reuse**: Avoid regenerating embeddings for repeated queries (~50-200ms saved)
//! - **Query Reuse**: Skip database search for cached results (~10-50ms saved)
//! - **Hit Rate**: Typically 30-60% for interactive search sessions
//!
//! # Cache Invalidation
//!
//! Cache is automatically cleared on:
//! - Document indexing operations (new documents added)
//! - Document deletion
//! - Reindexing operations
//!
//! Manual clearing available via `clear_search_cache` command.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchCacheStats {
    pub size: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

/// Manually clears all cached search results
///
/// Removes all entries from the in-memory search cache, forcing subsequent searches
/// to regenerate embeddings and query the database. Cache is automatically cleared
/// after indexing operations, but manual clearing may be useful for testing or
/// troubleshooting.
///
/// # Side Effects
///
/// - All cached search results removed
/// - Next searches will be slower (no cache hits)
/// - Cache metrics reset to 0
///
/// # Architecture
///
/// Clears the app's query cache, which the container holds
#[tauri::command]
#[specta::specta]
pub fn clear_search_cache(container: State<'_, Container>) -> Result<(), AppError> {
    container.query_cache().clear();
    tracing::info!("Search cache manually cleared");
    Ok(())
}

/// Returns detailed cache statistics including size and performance metrics
///
/// Provides comprehensive statistics about the search cache including current size,
/// capacity, hit/miss counts, time saved, and calculated hit rate. Useful for
/// monitoring cache effectiveness and tuning cache size.
///
/// # Statistics Breakdown
///
/// - **size**: Current number of cached entries
/// - **capacity**: Maximum entries before LRU eviction
/// - **hits**: Number of cache hits since startup
/// - **misses**: Number of cache misses since startup
/// - **total_time_saved_ms**: Estimated time saved by cache hits
/// - **hit_rate**: hits / (hits + misses), 0.0 to 1.0
///
/// # Architecture
///
/// Reads the statistics of the app's query cache, which the container holds
#[tauri::command]
#[specta::specta]
pub fn get_cache_stats(container: State<'_, Container>) -> Result<SearchCacheStats, AppError> {
    let stats = container.query_cache().stats();
    Ok(SearchCacheStats {
        size: stats.size,
        capacity: stats.capacity,
        hits: stats.hits,
        misses: stats.misses,
        total_time_saved_ms: stats.total_time_saved_ms,
        hit_rate: stats.hit_rate,
    })
}

/// Returns cache performance metrics (subset of stats)
///
/// Provides focused performance metrics without size/capacity information. Lighter
/// weight than `get_cache_stats` for frequent polling or metric collection.
///
/// # Metrics Breakdown
///
/// - **hits**: Number of cache hits since startup
/// - **misses**: Number of cache misses since startup
/// - **total_time_saved_ms**: Estimated time saved by cache
/// - **hit_rate**: hits / (hits + misses), 0.0 to 1.0
///
/// # Difference from get_cache_stats
///
/// - **Lighter**: Excludes size/capacity information
/// - **Focused**: Performance metrics only
/// - **Use Case**: Frequent polling, metrics dashboards
///
/// # Architecture
///
/// Reads the metrics of the app's query cache, which the container holds
#[tauri::command]
#[specta::specta]
pub fn get_cache_metrics(container: State<'_, Container>) -> Result<CacheMetrics, AppError> {
    let metrics = container.query_cache().metrics();
    Ok(CacheMetrics {
        hits: metrics.hits,
        misses: metrics.misses,
        total_time_saved_ms: metrics.total_time_saved_ms,
        hit_rate: metrics.hit_rate,
    })
}

/// Convenience wrapper for clear_search_cache with shorter name
///
/// Provides backward compatibility for frontend code using the generic `clear_cache` name.
/// Delegates to `clear_search_cache` which clears the search query cache.
///
/// # Note
///
/// This is a convenience alias for `clear_search_cache`. Both commands perform
/// the same operation. Use whichever name is more convenient for your use case.
#[tauri::command]
#[specta::specta]
pub fn clear_cache(container: State<'_, Container>) -> Result<(), AppError> {
    clear_search_cache(container)
}
