//! Search value objects (mode + query).
//!
//! **Do not declare `pub mod mode;` / `pub mod query;` here.** The
//! files are loaded at canonical paths
//! `crate::domain::value_objects::search_mode` and
//! `crate::domain::value_objects::search_query` via Strangler Fig
//! `#[path]` redirects in `domain/value_objects/mod.rs`. Declaring
//! them here would cause Rust to load each file at two module paths
//! and silently duplicate types.
