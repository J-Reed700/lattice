//! # Cache feature
//!
//! In-memory caches (LLM responses + search query results). Tauri
//! commands operate on search's global `QUERY_CACHE` singleton directly —
//! no DI, no use cases, no adapter.
//!
//! ## Public surface
//!
//! - `crate::features::cache::dto` — cache DTOs (CacheMetricsDto, etc.)
//! - `crate::features::cache::llm_cache::LlmCache` — LLM response cache
//! - `crate::features::cache::commands` — Tauri command handlers
//! - `crate::features::cache::plugin::init()` — Tauri plugin
//!
//! Model-related caches (`model_cache_adapter`, `model_catalog_cache`)
//! are part of the model_management feature, not this one.

pub mod commands;
pub mod dto;
pub mod llm_cache;
pub mod plugin;
