//! # Conversation feature
//!
//! Persistent chat conversations: CRUD on conversations and messages,
//! the conversation summarizer (background saga), the conversation
//! service (infra), and the full chat-with-RAG controller pipeline
//! (`chat.rs` + `chat/`).
//!
//! ## Public surface
//!
//! - `crate::features::conversation::dto` — ConversationDto
//! - `crate::features::conversation::message_bookmark_dto`
//! - `crate::features::conversation::space_dto`
//! - `crate::features::conversation::mapper` — application mapper
//! - `crate::features::conversation::summarizer` — ConversationSummarizer
//! - `crate::features::conversation::use_cases` — CRUD use cases
//! - `crate::features::conversation::commands` — basic CRUD commands
//! - `crate::features::conversation::plugin_impl` — plugin implementation
//! - `crate::features::conversation::chat` + `chat/` — chat controller
//!   pipeline (with submodule `retrieval/` — 15-file RAG pipeline)
//! - `crate::features::conversation::plugin::init()` — Tauri plugin
//! - `crate::features::conversation::ConversationServiceTrait` — service trait
//!
//! ## Kept as shared
//!
//! - `crate::domain::conversation` and `crate::domain::conversation_summary`
//!   — consumed through the domain namespace
//! - `crate::infrastructure::sagas::conversation_summary_saga`
//! - `crate::infrastructure::persistence::repositories::conversation_repository`
//! - `crate::infrastructure::persistence::mappers::conversation_mapper`
//!   (persistence mapper)
//! - `crate::infrastructure::events::conversation_events`
//! - `crate::infrastructure::services::conversation_service`
//!
//! `ConversationRepositoryPort` stays in `application/ports/`.

pub mod chat;
pub mod commands;
pub mod di;
pub mod dto;
pub mod mapper;
pub mod message_bookmark_dto;
pub mod plugin;
pub mod plugin_impl;
pub mod space_dto;
pub mod summarizer;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use trait_def::ConversationServiceTrait;
