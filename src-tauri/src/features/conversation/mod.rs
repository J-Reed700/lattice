//! # Conversation feature
//!
//! Persistent chat conversations: CRUD on conversations and messages,
//! the conversation service (infra), and the full chat-with-RAG controller pipeline
//! (`chat.rs` + `chat/`).
//!
//! ## Public surface
//!
//! - `crate::features::conversation::dto` — ConversationDto
//! - `crate::features::conversation::message_bookmark_dto`
//! - `crate::features::conversation::space_dto`
//! - `crate::features::conversation::mapper` — application mapper
//! - `crate::features::conversation::use_cases` — CRUD use cases
//! - `crate::features::conversation::commands` — basic CRUD commands
//! - `crate::features::conversation::plugin_impl` — plugin implementation
//! - `crate::features::conversation::chat` + `chat/` — chat controller
//!   pipeline (with submodule `retrieval/` — 15-file RAG pipeline)
//! - `crate::features::conversation::plugin::init()` — Tauri plugin
//! - `crate::features::conversation::ConversationServiceTrait` — service trait
//!
//! - `crate::features::conversation::repository` — SQLite persistence
//! - `crate::features::conversation::persistence_mapper`
//! - `crate::features::conversation::service` — ConversationService
//!
//! Domain types (Conversation, ConversationMessage, MessageRole, ...) live in
//! `crate::domain::conversation` because the application layer depends on them.

mod branching;
pub mod branching_dto;
pub mod chat;
pub mod commands;
pub mod di;
pub mod dto;
pub mod mapper;
pub mod message_bookmark_dto;
pub mod plugin;
pub mod plugin_impl;
pub mod space_dto;
pub mod space_repository;
mod synthesis;
pub mod trait_def;
pub mod use_cases;
pub mod workspace_dto;

#[cfg(test)]
pub mod mocks;

pub use trait_def::ConversationServiceTrait;
pub mod persistence_mapper;
pub mod repository;
pub mod service;
