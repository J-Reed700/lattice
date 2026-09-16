//! Search Cache Management Commands
//!
//! Thin command controllers for managing the in-memory search result cache.
//! The cache stores recent search results to avoid redundant embedding generation and
//! database queries, significantly improving search performance for repeated queries.
//!
//! # Commands (4 total)
//!
//! - `cache_operation` - Unified cache operation dispatcher (clear/stats/metrics)
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
//! - **Thread-Safe**: Global singleton with internal locking
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

use crate::features::cache::query_cache::QUERY_CACHE;
use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize, specta::Type)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum CacheOperation {
    Clear,
    GetStats,
    GetMetrics,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum CacheResponse {
    Cleared,
    Stats(SearchCacheStats),
    Metrics(CacheMetrics),
}

/// Unified cache operation dispatcher using enum-based pattern
///
/// Single command handling multiple cache operations (clear, stats, metrics) via enum
/// dispatch pattern. Simplifies frontend API by consolidating related operations.
///
/// # Arguments
///
/// * `operation` - Enum specifying the operation to perform
///
/// # Returns
///
/// * `Ok(CacheResponse)` - Operation-specific response (Cleared/Stats/Metrics)
/// * `Err(AppError)` - Cache operation failed (rare)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Clear cache
/// const clearResult = await invoke('cache_operation', {
///   operation: { action: 'clear' }
/// });
/// console.log('Cache cleared');
///
/// // Get stats
/// const statsResult = await invoke<{ type: 'stats', data: CacheStats }>('cache_operation', {
///   operation: { action: 'getStats' }
/// });
/// console.log('Cache size:', statsResult.data.size);
/// console.log('Hit rate:', statsResult.data.hitRate);
///
/// // Get metrics
/// const metricsResult = await invoke<{ type: 'metrics', data: CacheMetrics }>('cache_operation', {
///   operation: { action: 'getMetrics' }
/// });
/// console.log('Hits:', metricsResult.data.hits);
/// console.log('Time saved:', metricsResult.data.totalTimeSavedMs, 'ms');
/// ```
///
/// # Performance
///
/// - **Clear**: ~1-10ms (depends on cache size)
/// - **Stats/Metrics**: ~1μs (atomic reads)
/// - **Synchronous**: Instant return
///
/// # Architecture
///
/// Enum dispatch pattern for unified API (alternative to separate commands)
#[tauri::command]
#[specta::specta]
pub fn cache_operation(operation: CacheOperation) -> Result<CacheResponse, AppError> {
    match operation {
        CacheOperation::Clear => {
            QUERY_CACHE.clear();
            tracing::info!("Search cache manually cleared");
            Ok(CacheResponse::Cleared)
        }
        CacheOperation::GetStats => {
            let stats = QUERY_CACHE.stats();
            Ok(CacheResponse::Stats(SearchCacheStats {
                size: stats.size,
                capacity: stats.capacity,
                hits: stats.hits,
                misses: stats.misses,
                total_time_saved_ms: stats.total_time_saved_ms,
                hit_rate: stats.hit_rate,
            }))
        }
        CacheOperation::GetMetrics => {
            let metrics = QUERY_CACHE.metrics();
            Ok(CacheResponse::Metrics(CacheMetrics {
                hits: metrics.hits,
                misses: metrics.misses,
                total_time_saved_ms: metrics.total_time_saved_ms,
                hit_rate: metrics.hit_rate,
            }))
        }
    }
}

/// Manually clears all cached search results
///
/// Removes all entries from the in-memory search cache, forcing subsequent searches
/// to regenerate embeddings and query the database. Cache is automatically cleared
/// after indexing operations, but manual clearing may be useful for testing or
/// troubleshooting.
///
/// # Returns
///
/// * `Ok(())` - Cache cleared successfully
/// * `Err(AppError)` - Cache clear failed (rare)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Clear cache manually
/// await invoke('clear_search_cache');
/// console.log('Search cache cleared');
///
/// // Clear cache after settings change
/// const handleSettingsChange = async () => {
///   await updateSettings();
///   await invoke('clear_search_cache'); // Force fresh results
///   refreshSearchResults();
/// };
///
/// // Clear cache button
/// const handleClearCache = async () => {
///   await invoke('clear_search_cache');
///   showNotification('Cache cleared. Next search will be fresh.');
/// };
/// ```
///
/// # Side Effects
///
/// - All cached search results removed
/// - Next searches will be slower (no cache hits)
/// - Cache metrics reset to 0
///
/// # Use Cases
///
/// - **Testing**: Verify search behavior without cache
/// - **Troubleshooting**: Clear stale cache entries
/// - **Settings Changes**: Force fresh results after config changes
/// - **Manual Maintenance**: User-initiated cache clearing
///
/// # Performance
///
/// - **Clear Time**: ~1-10ms (depends on cache size)
/// - **Thread-Safe**: Locked operation
/// - **Synchronous**: Blocks until complete
///
/// # Architecture
///
/// Direct accessor to global QUERY_CACHE singleton
#[tauri::command]
#[specta::specta]
pub fn clear_search_cache() -> Result<(), AppError> {
    QUERY_CACHE.clear();
    tracing::info!("Search cache manually cleared");
    Ok(())
}

/// Returns detailed cache statistics including size and performance metrics
///
/// Provides comprehensive statistics about the search cache including current size,
/// capacity, hit/miss counts, time saved, and calculated hit rate. Useful for
/// monitoring cache effectiveness and tuning cache size.
///
/// # Returns
///
/// * `Ok(CacheStats)` - Detailed cache statistics
/// * `Err(AppError)` - Stats retrieval failed (rare)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface CacheStats {
///   size: number;              // Current entries (e.g., 42)
///   capacity: number;          // Max entries (e.g., 100)
///   hits: number;              // Cache hits (e.g., 127)
///   misses: number;            // Cache misses (e.g., 85)
///   totalTimeSavedMs: number;  // Time saved by cache hits (e.g., 6350ms)
///   hitRate: number;           // Hit rate 0.0-1.0 (e.g., 0.599 = 59.9%)
/// }
///
/// // Get cache stats
/// const stats = await invoke<CacheStats>('get_cache_stats');
///
/// console.log(`Cache: ${stats.size}/${stats.capacity} entries`);
/// console.log(`Hit rate: ${(stats.hitRate * 100).toFixed(1)}%`);
/// console.log(`Time saved: ${(stats.totalTimeSavedMs / 1000).toFixed(1)}s`);
///
/// // Display in UI
/// updateCacheDisplay({
///   usage: `${stats.size} / ${stats.capacity}`,
///   hitRate: `${(stats.hitRate * 100).toFixed(1)}%`,
///   timeSaved: `${(stats.totalTimeSavedMs / 1000).toFixed(1)}s`
/// });
///
/// // Monitor cache effectiveness
/// if (stats.hitRate < 0.3) {
///   console.warn('Low cache hit rate. Consider increasing capacity.');
/// }
/// ```
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
/// # Use Cases
///
/// - **Dashboard**: Display cache performance in admin UI
/// - **Monitoring**: Track cache effectiveness over time
/// - **Tuning**: Determine optimal cache capacity
/// - **Diagnostics**: Troubleshoot search performance issues
///
/// # Performance
///
/// - **Access Time**: ~1μs (atomic counter reads)
/// - **No Locking**: Lock-free atomic operations
/// - **Synchronous**: Instant return
///
/// # Architecture
///
/// Direct accessor to global QUERY_CACHE singleton statistics
#[tauri::command]
#[specta::specta]
pub fn get_cache_stats() -> Result<SearchCacheStats, AppError> {
    let stats = QUERY_CACHE.stats();
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
/// # Returns
///
/// * `Ok(CacheMetrics)` - Performance metrics (hits, misses, time saved, hit rate)
/// * `Err(AppError)` - Metrics retrieval failed (rare)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface CacheMetrics {
///   hits: number;              // Cache hits
///   misses: number;            // Cache misses
///   totalTimeSavedMs: number;  // Time saved (ms)
///   hitRate: number;           // Hit rate 0.0-1.0
/// }
///
/// // Get cache metrics
/// const metrics = await invoke<CacheMetrics>('get_cache_metrics');
/// console.log(`Cache efficiency: ${(metrics.hitRate * 100).toFixed(1)}%`);
/// console.log(`Performance gain: ${metrics.totalTimeSavedMs}ms`);
///
/// // Periodic metrics collection
/// setInterval(async () => {
///   const metrics = await invoke<CacheMetrics>('get_cache_metrics');
///   logMetrics('cache.hits', metrics.hits);
///   logMetrics('cache.misses', metrics.misses);
///   logMetrics('cache.hit_rate', metrics.hitRate);
/// }, 60000); // Every minute
///
/// // Display compact metrics
/// updateMetricsBar({
///   hits: metrics.hits,
///   hitRate: `${(metrics.hitRate * 100).toFixed(0)}%`
/// });
/// ```
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
/// # Performance
///
/// - **Access Time**: ~1μs (atomic counter reads)
/// - **No Locking**: Lock-free atomic operations
/// - **Synchronous**: Instant return
///
/// # Architecture
///
/// Direct accessor to global QUERY_CACHE singleton metrics
#[tauri::command]
#[specta::specta]
pub fn get_cache_metrics() -> Result<CacheMetrics, AppError> {
    let metrics = QUERY_CACHE.metrics();
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
/// # Returns
///
/// * `Ok(())` - Cache cleared successfully
/// * `Err(AppError)` - Cache clear failed (rare)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Clear cache using short name
/// await invoke('clear_cache');
/// ```
///
/// # Note
///
/// This is a convenience alias for `clear_search_cache`. Both commands perform
/// the same operation. Use whichever name is more convenient for your use case.
#[tauri::command]
#[specta::specta]
pub fn clear_cache() -> Result<(), AppError> {
    clear_search_cache()
}
