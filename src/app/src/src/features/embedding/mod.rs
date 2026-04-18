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
//! - `crate::features::embedding::entity` — `Embedding` domain entity
//! - `crate::features::embedding::use_cases` — embedding use cases
//! - `crate::features::embedding::onnx_service` — OnnxEmbeddingService
//! - `crate::features::embedding::remote_service` — RemoteEmbeddingService
//! - `crate::features::embedding::generator` — EmbeddingGenerator, ModelConfig
//! - `crate::features::embedding::validator` — validation utilities
//! - `crate::features::embedding::persistence_mapper` — EmbeddingMapper, EmbeddingDTO
//! - `crate::features::embedding::repository` — Embedding, EmbeddingRepository (port impl)
//! - `crate::features::embedding::repository_tx` — tx-wrapper
//! - `crate::features::embedding::service` — EmbeddingService
//! - `crate::features::embedding::commands` — Tauri command handlers
//! - `crate::features::embedding::plugin::init()` — Tauri plugin
//! - `crate::features::embedding::EmbeddingServiceTrait` — service trait
//!
//! Ports (EmbeddingPort, EmbeddingRepositoryPort, MockEmbeddingPort)
//! stay in `application/ports/`.

pub mod commands;
pub mod dto;
pub mod entity;
pub mod generator;
pub mod onnx_service;
pub mod persistence_mapper;
pub mod plugin;
pub mod remote_service;
pub mod repository;
pub mod repository_tx;
pub mod service;
pub mod trait_def;
pub mod use_cases;
pub mod validator;

#[cfg(test)]
pub mod mocks;

pub use trait_def::EmbeddingServiceTrait;
