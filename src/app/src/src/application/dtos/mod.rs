//! # Application DTOs
//!
//! Data Transfer Objects for crossing application boundaries.
//!
//! DTOs provide a flat, serialization-friendly representation of data
//! that is independent of domain models and infrastructure concerns.
//!
//! ## Purpose
//!
//! - **Decoupling**: DTOs decouple the API from domain models
//! - **Serialization**: DTOs use simple types (String, i64) for JSON serialization
//! - **Stability**: DTO changes don't affect domain logic
//! - **Versioning**: DTOs can evolve independently for API compatibility
//!
//! ## Organization
//!
//! - `search_dto` - Search requests and responses
//! - `indexing_dto` - Document indexing operations
//! - `qa_dto` - Question-answering operations
//! - `tag_dto` - Tag management
//! - `document_dto` - Document metadata and operations
//! - `file_dto` - File management operations
//! - `conversation_dto` - Conversation management
//! - `mention_dto` - Mention extraction and management
//! - `credential_dto` - Credential storage and retrieval
//! - `cache_dto` - Cache statistics and metrics
//! - `backup_dto` - Backup and restore operations
//! - `update_dto` - Update checking
//! - `metric_dto` - Application metrics

// Vertical-slice migration (backup): DTO physically lives in features/backup/dto.rs.
#[path = "../../features/backup/dto.rs"]
pub mod backup_dto;
#[path = "modules/batch_dto.rs"]
pub mod batch_dto;
#[path = "modules/cache_dto.rs"]
pub mod cache_dto;
#[path = "modules/conversation_dto.rs"]
pub mod conversation_dto;
#[path = "modules/conversation_message_bookmark_dto.rs"]
pub mod conversation_message_bookmark_dto;
#[path = "modules/conversation_space_dto.rs"]
pub mod conversation_space_dto;
#[path = "modules/credential_dto.rs"]
pub mod credential_dto;
#[path = "modules/document_dto.rs"]
pub mod document_dto;
#[path = "modules/embedding_dto.rs"]
pub mod embedding_dto;
#[path = "modules/extraction_dto.rs"]
pub mod extraction_dto;
// Vertical-slice migration (favorites): DTO physically lives in features/favorites/dto.rs.
#[path = "../../features/favorites/dto.rs"]
pub mod favorite_dto;
#[path = "modules/file_dto.rs"]
pub mod file_dto;
#[path = "modules/function_calling_dto.rs"]
pub mod function_calling_dto;
#[path = "modules/health_dto.rs"]
pub mod health_dto;
#[path = "modules/indexing_dto.rs"]
pub mod indexing_dto;
#[path = "modules/initialization_dto.rs"]
pub mod initialization_dto;
#[path = "modules/llm_dto.rs"]
pub mod llm_dto;
#[path = "modules/mention_dto.rs"]
pub mod mention_dto;
// Vertical-slice migration (metrics): DTO physically lives in features/metrics/dto.rs.
#[path = "../../features/metrics/dto.rs"]
pub mod metric_dto;
// Vertical-slice migration (qa): DTO physically lives in features/qa/dto.rs.
#[path = "../../features/qa/dto.rs"]
pub mod qa_dto;
#[path = "modules/recent_dto.rs"]
pub mod recent_dto;
#[path = "modules/search_dto.rs"]
pub mod search_dto;
pub mod settings;
#[path = "modules/tag_dto.rs"]
pub mod tag_dto;
// Vertical-slice migration (updates): DTO physically lives in features/updates/dto.rs.
// This `#[path]` redirect keeps the legacy `crate::application::dtos::update_dto::*`
// import surface intact (Strangler Fig).
#[path = "../../features/updates/dto.rs"]
pub mod update_dto;
#[path = "modules/web_dto.rs"]
pub mod web_dto;

// Re-export commonly used DTOs
pub use backup_dto::*;
pub use batch_dto::*;
pub use cache_dto::*;
pub use conversation_dto::*;
pub use conversation_message_bookmark_dto::*;
pub use conversation_space_dto::*;
pub use credential_dto::*;
pub use document_dto::*;
pub use embedding_dto::*;
pub use extraction_dto::*;
pub use favorite_dto::*;
pub use file_dto::*;
pub use function_calling_dto::*;
pub use health_dto::*;
pub use indexing_dto::*;
pub use initialization_dto::*;
pub use llm_dto::*;
pub use mention_dto::*;
pub use metric_dto::*;
pub use qa_dto::*;
pub use recent_dto::*;
pub use search_dto::*;
pub use settings::*;
pub use tag_dto::*;
pub use update_dto::*;
pub use web_dto::*;
