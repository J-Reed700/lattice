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
// Vertical-slice migration (batch): DTO lives in features/batch/dto.rs.
#[path = "../../features/batch/dto.rs"]
pub mod batch_dto;
// Vertical-slice migration (cache): DTO physically lives in features/cache/dto.rs.
#[path = "../../features/cache/dto.rs"]
pub mod cache_dto;
#[path = "modules/conversation_dto.rs"]
pub mod conversation_dto;
#[path = "modules/conversation_message_bookmark_dto.rs"]
pub mod conversation_message_bookmark_dto;
#[path = "modules/conversation_space_dto.rs"]
pub mod conversation_space_dto;
// Vertical-slice migration (credentials): DTO physically lives in features/credentials/dto.rs.
#[path = "../../features/credentials/dto.rs"]
pub mod credential_dto;
#[path = "modules/document_dto.rs"]
pub mod document_dto;
#[path = "modules/embedding_dto.rs"]
pub mod embedding_dto;
// Vertical-slice migration (extraction): DTO lives in features/extraction/dto.rs.
#[path = "../../features/extraction/dto.rs"]
pub mod extraction_dto;
// Vertical-slice migration (favorites): DTO physically lives in features/favorites/dto.rs.
#[path = "../../features/favorites/dto.rs"]
pub mod favorite_dto;
// Vertical-slice migration (file): DTO lives in features/file/dto.rs.
#[path = "../../features/file/dto.rs"]
pub mod file_dto;
// Vertical-slice migration (function_calling): DTO lives in features/function_calling/dto.rs.
#[path = "../../features/function_calling/dto.rs"]
pub mod function_calling_dto;
// Vertical-slice migration (health): DTO physically lives in features/health/dto.rs.
#[path = "../../features/health/dto.rs"]
pub mod health_dto;
#[path = "modules/indexing_dto.rs"]
pub mod indexing_dto;
// Vertical-slice migration (initialization): DTO lives in features/initialization/dto.rs.
#[path = "../../features/initialization/dto.rs"]
pub mod initialization_dto;
#[path = "modules/llm_dto.rs"]
pub mod llm_dto;
// Vertical-slice migration (mentions): DTO physically lives in features/mentions/dto.rs.
#[path = "../../features/mentions/dto.rs"]
pub mod mention_dto;
// Vertical-slice migration (metrics): DTO physically lives in features/metrics/dto.rs.
#[path = "../../features/metrics/dto.rs"]
pub mod metric_dto;
// Vertical-slice migration (qa): DTO physically lives in features/qa/dto.rs.
#[path = "../../features/qa/dto.rs"]
pub mod qa_dto;
// Vertical-slice migration (recent): DTO physically lives in features/recent/dto.rs.
#[path = "../../features/recent/dto.rs"]
pub mod recent_dto;
#[path = "modules/search_dto.rs"]
pub mod search_dto;
// Vertical-slice migration (settings): DTO lives in features/settings/dto/.
#[path = "../../features/settings/dto/mod.rs"]
pub mod settings;
// Vertical-slice migration (tags): DTO physically lives in features/tags/dto.rs.
#[path = "../../features/tags/dto.rs"]
pub mod tag_dto;
// Vertical-slice migration (updates): DTO physically lives in features/updates/dto.rs.
// This `#[path]` redirect keeps the legacy `crate::application::dtos::update_dto::*`
// import surface intact (Strangler Fig).
#[path = "../../features/updates/dto.rs"]
pub mod update_dto;
// Vertical-slice migration (web): DTO lives in features/web/dto.rs.
#[path = "../../features/web/dto.rs"]
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
