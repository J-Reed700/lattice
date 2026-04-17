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

pub mod conversation_mapper;
pub mod document_mapper;
// Vertical-slice migration (favorites): mapper lives in features/favorites/mapper.rs.
#[path = "../../features/favorites/mapper.rs"]
pub mod favorite_mapper;
pub mod indexing_mapper;
// Vertical-slice migration (mentions): mapper lives in features/mentions/mapper.rs.
#[path = "../../features/mentions/mapper.rs"]
pub mod mention_mapper;
// Vertical-slice migration (recent): mapper lives in features/recent/mapper.rs.
#[path = "../../features/recent/mapper.rs"]
pub mod recent_document_mapper;
pub mod search_mapper;
// Vertical-slice migration (settings): mapper lives in features/settings/mapper.rs.
#[path = "../../features/settings/mapper.rs"]
pub mod settings_mapper;
// Vertical-slice migration (tags): mapper lives in features/tags/mapper.rs.
#[path = "../../features/tags/mapper.rs"]
pub mod tag_mapper;

// Re-export mappers
pub use conversation_mapper::{ConversationMapper, MessageMapper};
pub use document_mapper::DocumentMapper;
pub use favorite_mapper::FavoriteMapper;
pub use indexing_mapper::IndexingMapper;
pub use mention_mapper::MentionMapper;
pub use recent_document_mapper::RecentDocumentMapper;
pub use search_mapper::SearchMapper;
pub use settings_mapper::SettingsMapper;
pub use tag_mapper::TagMapper;
