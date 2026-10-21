//! # Embedding feature
//!
//! Text embedding generation on a bundled llama-server, plus persistence of
//! embeddings tied to document chunks. One of the most cross-consumed
//! features in the app — search, indexing, qa, and mentions all depend on the
//! shared ports this feature provides. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::embedding::dto` — embedding DTOs
//! - `crate::features::embedding::use_cases` — embedding use cases
//! - `crate::features::embedding::llama_cpp` — LlamaEmbeddingService, the `EmbeddingPort` on llama-server
//! - `crate::features::embedding::input_policy` — tokenizer-based passage splitting
//! - `crate::features::embedding::prefixes` — per-family query/document instruction prefixes
//! - `crate::features::embedding::repository` — Embedding, EmbeddingRepository (port impl)
//! - `crate::features::embedding::service` — DynamicEmbedding, the port over the loaded model
//! - `crate::features::embedding::commands` — Tauri command handlers
//! - `crate::features::embedding::plugin::init()` — Tauri plugin
//!
//! Ports (EmbeddingPort, EmbeddingRepositoryPort, MockEmbeddingPort)
//! stay in `application/ports/`.

pub mod commands;
pub mod di;
pub mod dto;
pub mod input_policy;
pub mod llama_cpp;
pub(crate) mod loading;
pub mod plugin;
pub mod prefixes;
pub mod repository;
pub(crate) mod runtime;
pub mod service;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock_repository;
