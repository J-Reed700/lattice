//! # QA feature
//!
//! Retrieval-Augmented Generation (RAG) question answering. Combines
//! HyDE-based query interpretation, vector retrieval, and LLM completion
//! to answer questions with inline citations. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::qa::dto` — QA DTOs
//! - `crate::features::qa::use_cases` — QA use cases
//! - `crate::features::qa::conversational_service` — ConversationalQAService
//! - `crate::features::qa::commands` — Tauri command handlers
//! - `crate::features::qa::plugin::init()` — Tauri plugin
//!
//! ## Kept as shared namespaces (redirects retained)
//!
//! - `crate::domain::qa::*` (domain types like HyDEInterpretation, QueryType)
//!   — consumed broadly by conversation chat retrieval, etc.
//! - `crate::infrastructure::qa` (QA engine modules) — consumed by
//!   conversation chat retrieval
//! - `crate::infrastructure::services::hyde` — HyDE retrieval, consumed
//!   by conversation chat retrieval
//! - `trait_def` (traits.rs) and `mocks` remain via shared
//!   `infrastructure::services::{traits,mocks}` aggregators
//! - `tests/conversational_service.rs` remains via
//!   `infrastructure::services::tests` aggregator
//!
//! Shared ports (EmbeddingPort, LLMPort, VectorSearchPort, etc.) stay
//! in `application/ports/`.

pub mod commands;
pub mod conversational_service;
pub mod dto;
pub mod plugin;
pub mod use_cases;
