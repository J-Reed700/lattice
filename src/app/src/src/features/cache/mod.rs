//! # Cache feature
//!
//! In-memory LLM response cache (tag results, query results) plus an
//! adapter implementing `CachePort` so use cases can clear or inspect
//! cache stats without knowing the backend.
//!
//! ## File layout
//!
//! | File              | Canonical module path                                      |
//! |-------------------|------------------------------------------------------------|
//! | `dto.rs`          | `crate::application::dtos::cache_dto`                      |
//! | `use_cases/`      | `crate::application::use_cases::cache`                     |
//! | `adapter.rs`      | `crate::infrastructure::cache::cache_adapter`              |
//! | `query_cache.rs`  | `crate::infrastructure::cache::query_cache`                |
//! | `llm_cache.rs`    | `crate::infrastructure::services::llm_cache`               |
//! | `commands.rs`     | `crate::interfaces::commands::cache` (aka `cache_commands`) |
//! | `plugin.rs`       | `crate::plugins::cache_plugin`                             |
//!
//! `CachePort` stays in `application/ports/`.
//!
//! Model-related caches (`infrastructure/model_cache_adapter.rs`,
//! `infrastructure/model_catalog_cache.rs`) are model-management
//! concerns, not cache-feature concerns. They'll graduate with the
//! model-management feature migration.
//!
//! The placeholder stub at `infrastructure/cache/llm_cache.rs` is an
//! unused orphan (not registered in the cache mod.rs) and is left alone.
