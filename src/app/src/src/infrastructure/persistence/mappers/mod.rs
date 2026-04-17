//! # Persistence Mappers
//!
//! Maps between database models (anemic DTOs) and domain entities (rich models).
//!
//! ## Architecture Pattern
//!
//! The mapper layer implements the **Data Mapper** pattern:
//!
//! ```text
//! Domain Entity (rich) ←→ Mapper ←→ DB Model (anemic)
//! ```
//!
//! - **Domain Entities**: Rich models with business logic (domain layer)
//! - **DB Models**: Anemic structs for database persistence (infrastructure layer)
//! - **Mappers**: Bidirectional conversion logic (infrastructure layer)
//!
//! ## Key Principles
//!
//! 1. **Unidirectional Dependency**: Mappers depend on domain, not vice versa
//! 2. **Error Handling**: Validation errors during mapping return `Result<T>`
//! 3. **Type Safety**: Use domain types (DocumentId, TagName, etc.)
//! 4. **No Leakage**: DB models NEVER escape the infrastructure layer
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use vault_desktop::infrastructure::persistence::mappers::DocumentMapper;
//! use vault_desktop::domain::entities::Document;
//!
//! // DB model → Domain entity
//! let db_model = /* from SQLx query */;
//! let entity: Document = DocumentMapper::to_entity(&db_model)?;
//!
//! // Domain entity → DB model
//! let entity = Document::new(/* ... */);
//! let db_model = DocumentMapper::to_model(&entity);
//! ```

pub mod chunk_mapper;
pub mod conversation_mapper;
pub mod document_mapper;
// Vertical-slice migration (embedding): persistence mapper lives in features/embedding/.
#[path = "../../../features/embedding/persistence_mapper.rs"]
pub mod embedding_mapper;
// Vertical-slice migration (tags): persistence mapper lives in features/tags/persistence_mapper.rs.
#[path = "../../../features/tags/persistence_mapper.rs"]
pub mod tag_mapper;
// TODO: Add mention_mapper when mention entity structure is finalized

// Re-export mappers (public API)
pub use chunk_mapper::ChunkMapper;
pub use conversation_mapper::{
    ConversationMapper, ConversationMessageMapper, DocumentReferenceMapper,
};
pub use document_mapper::DocumentMapper;
pub use embedding_mapper::EmbeddingMapper;
pub use tag_mapper::TagMapper;

// DB models are private to infrastructure crate (prevent leakage)
pub(crate) use chunk_mapper::ChunkModel;
pub(crate) use conversation_mapper::{
    ConversationMessageModel, ConversationModel, DocumentReferenceModel,
};
pub(crate) use document_mapper::DocumentModel;
pub(crate) use tag_mapper::TagModel;
