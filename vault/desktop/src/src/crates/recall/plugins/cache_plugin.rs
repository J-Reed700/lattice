//! Cache Plugin - Search query cache management
//!
//! Migrated from ipc/domains/cache.rs as part of Operation Scorched Earth Batch 2
//!
//! NOTE: Cache commands are synchronous (no async) because cache is a global singleton

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

// Re-export types and commands from the main cache commands module
pub use crate::interfaces::commands::cache::{
    cache_operation, clear_cache, clear_search_cache, get_cache_metrics, get_cache_stats,
    CacheMetrics, SearchCacheStats,
};

// Import the tauri command macros that were generated
use crate::interfaces::commands::cache::{
    __cmd__cache_operation, __cmd__clear_cache, __cmd__clear_search_cache,
    __cmd__get_cache_metrics, __cmd__get_cache_stats,
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("cache")
        .invoke_handler(tauri::generate_handler![
            clear_cache,
            get_cache_stats,
            get_cache_metrics,
            clear_search_cache,
            cache_operation,
        ])
        .build()
}
