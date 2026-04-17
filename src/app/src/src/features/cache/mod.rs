//! # Cache feature
//!
//! In-memory LLM response cache (tag results, query results) plus an
//! adapter implementing `CachePort`. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::cache::dto` — cache DTOs (CacheStatsDto, etc.)
//! - `crate::features::cache::use_cases` — cache management use cases
//! - `crate::features::cache::adapter::CacheAdapter` — impl of `CachePort`
//! - `crate::features::cache::query_cache` — query result cache
//! - `crate::features::cache::llm_cache::LlmCache` — LLM response cache
//! - `crate::features::cache::commands` — Tauri command handlers
//! - `crate::features::cache::plugin::init()` — Tauri plugin
//!
//! `CachePort` stays in `application/ports/`.
//!
//! Model-related caches (`model_cache_adapter`, `model_catalog_cache`)
//! are part of the model_management feature, not this one.

pub mod adapter;
pub mod commands;
pub mod dto;
pub mod llm_cache;
pub mod plugin;
pub mod query_cache;
pub mod use_cases;
