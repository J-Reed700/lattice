//! # Model Management feature
//!
//! Model lifecycle: tracking which models are downloaded, which are
//! the active chat / embedding models, model metadata catalog, and
//! adapters bridging the LLM/HuggingFace catalogs to the persistent
//! model repository. Tauri "model" plugin lives here.
//!
//! ## Public surface
//!
//! - `crate::features::model_management::domain`
//! - `crate::features::model_management::use_cases`
//! - `crate::features::model_management::commands` (basic)
//! - `crate::features::model_management::commands_extra` (extended)
//! - `crate::features::model_management::repository_tx` (tx-wrapper)
//! - `crate::features::model_management::huggingface_adapter` —
//!   ModelCatalogPort impl backed by HuggingFace
//! - `crate::features::model_management::cache_adapter` —
//!   ModelCacheAdapter (in-memory model cache)
//! - `crate::features::model_management::catalog_cache` —
//!   persistent model catalog cache
//! - `crate::features::model_management::plugin::init()` — Tauri plugin
//!
//! No DTO, port, or service traits — commands speak in domain types
//! directly.

pub mod cache_adapter;
pub mod catalog_cache;
pub mod commands;
pub mod commands_extra;
pub mod di;
pub mod domain;
pub mod huggingface_adapter;
pub mod plugin;
pub mod repository_tx;
pub mod use_cases;
