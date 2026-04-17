//! Cache feature — use cases.

pub mod clear;
pub mod get_size;
pub mod get_stats;

pub use clear::ClearCacheUseCase;
pub use get_size::GetCacheSizeUseCase;
pub use get_stats::GetCacheStatsUseCase;
