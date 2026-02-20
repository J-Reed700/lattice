//! Cache Plugin - Search query cache management
//!
//! NOTE: Cache commands are synchronous (no async) because cache is a global singleton.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

use crate::interfaces::commands::cache as cache_commands;

// Re-export types and commands from the cache command module.
pub use crate::interfaces::commands::cache::{
    cache_operation, clear_cache, clear_search_cache, get_cache_metrics, get_cache_stats,
    CacheMetrics, SearchCacheStats,
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("cache")
        .invoke_handler(tauri::generate_handler![
            cache_commands::clear_cache,
            cache_commands::get_cache_stats,
            cache_commands::get_cache_metrics,
            cache_commands::clear_search_cache,
            cache_commands::cache_operation,
        ])
        .build()
}
