//! Search cache commands.

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

#[tauri::command]
#[specta::specta]
pub fn clear_search_cache(container: State<'_, Container>) -> Result<(), AppError> {
    container.query_cache().clear();
    tracing::info!("Search cache manually cleared");
    Ok(())
}

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

#[tauri::command]
#[specta::specta]
pub fn clear_cache(container: State<'_, Container>) -> Result<(), AppError> {
    clear_search_cache(container)
}
