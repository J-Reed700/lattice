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
//! - `crate::features::qa::engine` — QA engine modules
//! - `crate::features::qa::hyde` — HyDE retrieval
//!
//! Domain types (HyDEInterpretation, QueryType, ChatResponse, ...) live in
//! `crate::domain::qa` because the application layer depends on them.
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
pub mod engine;
pub mod hyde;
#[cfg(test)]
mod tests;
