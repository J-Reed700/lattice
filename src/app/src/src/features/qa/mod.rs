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
//! - `crate::features::qa::{QAEngineTrait, ConversationalQAServiceTrait}` — service traits
//!
//! ## Kept as shared namespaces
//!
//! - `crate::domain::qa::*` (domain types like HyDEInterpretation, QueryType)
//! - `crate::infrastructure::qa` (QA engine modules)
//! - `crate::infrastructure::services::hyde` — HyDE retrieval
//! - `tests/conversational_service.rs` remains via
//!   `infrastructure::services::tests` aggregator
//!
//! Shared ports stay in `application/ports/`.

pub mod commands;
pub mod conversational_service;
pub mod di;
pub mod dto;
pub mod plugin;
pub mod starters;
pub mod starters_dto;
pub mod traits;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use traits::{ConversationalQAServiceTrait, QAEngineTrait};
