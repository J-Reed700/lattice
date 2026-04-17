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

#[path = "modules/document_dto.rs"]
pub mod document_dto;

// Re-export commonly used DTOs
pub use document_dto::*;
