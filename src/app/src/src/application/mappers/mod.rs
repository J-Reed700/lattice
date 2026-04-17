//! # Application Mappers
//!
//! Mappers for converting between domain models and DTOs.
//!
//! ## Purpose
//!
//! Mappers provide clean conversion between:
//! - Domain models (rich business logic, invariants, behavior)
//! - DTOs (flat data structures for serialization)
//!
//! This separation ensures:
//! - **Decoupling**: API changes don't affect domain models
//! - **Flexibility**: Can evolve DTOs for API versioning
//! - **Testing**: Easy to test conversions in isolation
//!
//! ## Organization
//!
//! - `search_mapper` - SearchResult ↔ SearchResultDto
//! - `document_mapper` - DocumentAggregate ↔ DocumentDto
//! - `tag_mapper` - Tag models ↔ TagDto
//! - `indexing_mapper` - ChunkingStrategy ↔ ChunkingStrategyDto
//! - `conversation_mapper` - Conversation models ↔ ConversationDto
//! - `mention_mapper` - Mention data ↔ MentionDto
//! - `favorite_mapper` - Favorite data ↔ FavoriteDto
//! - `recent_document_mapper` - Recent document data ↔ RecentDocumentDto

pub mod document_mapper;

// Re-export mappers
pub use crate::features::conversation::mapper::{ConversationMapper, MessageMapper};
pub use document_mapper::DocumentMapper;
