//! Persistent chat conversations: CRUD on conversations and messages, the
//! conversation service and repository, and the chat-with-RAG pipeline
//! (`chat`). Domain types live in `lattice_core::domain::conversation`.

pub mod activity;
mod branching;
pub mod branching_dto;
pub mod chat;
pub mod commands;
pub mod compaction;
pub mod context_repository;
pub mod di;
pub mod dto;
pub(crate) mod handoff;
pub mod knowledge_dto;
pub mod mapper;
pub mod memory;
pub mod memory_details;
pub mod memory_dto;
pub mod message_bookmark_dto;
mod model_availability;
pub mod plugin;
pub mod plugin_impl;
pub mod space_dto;
pub mod space_repository;
pub mod starters;
pub mod starters_dto;
mod synthesis;
pub mod tangent_dto;
pub mod trait_def;
pub mod use_cases;
pub mod workspace_dto;

#[cfg(test)]
pub mod mocks;

pub use trait_def::ConversationServiceTrait;
pub mod persistence_mapper;
pub mod repository;
pub mod service;
