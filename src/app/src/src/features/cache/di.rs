//! Cache feature dependency injection.

use std::sync::Arc;

use crate::application::ports::CachePort;
use crate::features::cache::adapter::CacheAdapter;
use crate::features::cache::llm_cache::LlmCache;
use crate::features::cache::use_cases::{
    ClearCacheUseCase, GetCacheSizeUseCase, GetCacheStatsUseCase,
};

#[derive(Clone)]
pub struct CacheDi {
    pub cache: Arc<dyn CachePort>,
    pub clear_cache_use_case: Arc<ClearCacheUseCase>,
    pub get_cache_size_use_case: Arc<GetCacheSizeUseCase>,
    pub get_cache_stats_use_case: Arc<GetCacheStatsUseCase>,
}

pub fn build() -> CacheDi {
    let cache = Arc::new(CacheAdapter::new(LlmCache::new())) as Arc<dyn CachePort>;

    CacheDi {
        clear_cache_use_case: Arc::new(ClearCacheUseCase::new(cache.clone())),
        get_cache_size_use_case: Arc::new(GetCacheSizeUseCase::new(cache.clone())),
        get_cache_stats_use_case: Arc::new(GetCacheStatsUseCase::new(cache.clone())),
        cache,
    }
}
