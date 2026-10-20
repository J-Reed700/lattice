//! # Embedding feature
//!
//! Text embedding generation via ONNX models (local) or remote APIs,
//! plus persistence of embeddings tied to document chunks. One of the
//! most cross-consumed features in the app — search, indexing, qa,
//! and mentions all depend on the shared ports this feature provides.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::embedding::dto` — embedding DTOs
//! - `crate::features::embedding::use_cases` — embedding use cases
//! - `crate::features::embedding::onnx_service` — OnnxEmbeddingService
//! - `crate::features::embedding::generator` — EmbeddingGenerator, ModelConfig
//! - `crate::features::embedding::late_chunking` — late-chunking strategy + pooling rules
//! - `crate::features::embedding::prefixes` — per-family query/document instruction prefixes
//! - `crate::features::embedding::qwen3_encoder` — batched, cache-free Qwen3 forward pass
//! - `crate::features::embedding::validator` — validation utilities
//! - `crate::features::embedding::repository` — Embedding, EmbeddingRepository (port impl)
//! - `crate::features::embedding::service` — EmbeddingService
//! - `crate::features::embedding::commands` — Tauri command handlers
//! - `crate::features::embedding::plugin::init()` — Tauri plugin
//!
//! Ports (EmbeddingPort, EmbeddingRepositoryPort, MockEmbeddingPort)
//! stay in `application/ports/`.

pub(crate) mod activity;
pub mod candle_service;
pub mod commands;
pub mod di;
pub mod dto;
pub mod generator;
pub mod input_policy;
pub mod late_chunking;
pub(crate) mod loading;
pub mod plugin;
pub mod prefixes;
pub mod qwen3_encoder;
pub mod repository;
pub(crate) mod runtime;
pub mod service;
pub mod sparse_head;
pub mod use_cases;
pub mod validator;

#[cfg(test)]
pub mod mocks;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock_repository;
