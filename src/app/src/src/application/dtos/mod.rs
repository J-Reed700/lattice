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

// Vertical-slice migration (conversation): DTOs live in features/conversation/.
#[path = "../../features/conversation/dto.rs"]
pub mod conversation_dto;
#[path = "../../features/conversation/message_bookmark_dto.rs"]
pub mod conversation_message_bookmark_dto;
#[path = "../../features/conversation/space_dto.rs"]
pub mod conversation_space_dto;
#[path = "modules/document_dto.rs"]
pub mod document_dto;
// Vertical-slice migration (embedding): DTO lives in features/embedding/dto.rs.
#[path = "../../features/embedding/dto.rs"]
pub mod embedding_dto;
// Vertical-slice migration (extraction): DTO lives in features/extraction/dto.rs.
#[path = "../../features/extraction/dto.rs"]
pub mod extraction_dto;
// Vertical-slice migration (file): DTO lives in features/file/dto.rs.
#[path = "../../features/file/dto.rs"]
pub mod file_dto;
// Vertical-slice migration (function_calling): DTO lives in features/function_calling/dto.rs.
#[path = "../../features/function_calling/dto.rs"]
pub mod function_calling_dto;
// Vertical-slice migration (indexing): DTO lives in features/indexing/dto.rs.
#[path = "../../features/indexing/dto.rs"]
pub mod indexing_dto;
// Vertical-slice migration (llm): DTO lives in features/llm/dto.rs.
#[path = "../../features/llm/dto.rs"]
pub mod llm_dto;
// Vertical-slice migration (qa): DTO physically lives in features/qa/dto.rs.
#[path = "../../features/qa/dto.rs"]
pub mod qa_dto;
// Vertical-slice migration (search): DTO lives in features/search/dto.rs.
#[path = "../../features/search/dto.rs"]
pub mod search_dto;
// Vertical-slice migration (settings): DTO lives in features/settings/dto/.
#[path = "../../features/settings/dto/mod.rs"]
pub mod settings;
// Vertical-slice migration (web): DTO lives in features/web/dto.rs.
#[path = "../../features/web/dto.rs"]
pub mod web_dto;

// Re-export commonly used DTOs
pub use conversation_dto::*;
pub use conversation_message_bookmark_dto::*;
pub use conversation_space_dto::*;
pub use document_dto::*;
pub use embedding_dto::*;
pub use extraction_dto::*;
pub use file_dto::*;
pub use function_calling_dto::*;
pub use indexing_dto::*;
pub use llm_dto::*;
pub use qa_dto::*;
pub use search_dto::*;
pub use settings::*;
pub use web_dto::*;
