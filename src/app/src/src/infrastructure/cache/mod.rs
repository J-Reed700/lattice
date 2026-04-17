//! Cache infrastructure.
//!
//! Vertical-slice migration: concrete cache implementations physically
//! live in `features/cache/`. This module keeps the legacy
//! `crate::infrastructure::cache::*` paths resolvable (Strangler Fig).

#[path = "../../features/cache/adapter.rs"]
pub mod cache_adapter;

#[path = "../../features/cache/query_cache.rs"]
pub mod query_cache;
