//! Consolidated Command Handlers
//!
//! This module provides consolidated command handlers that reduce the total number
//! of registered Tauri commands from 119 to approximately 25-30.
//!
//! Each consolidated handler uses tagged enums to dispatch to the appropriate
//! operation, maintaining type safety while reducing the command surface area.

// Re-export consolidated operations from individual modules
pub use super::cache::{cache_operation, CacheOperation, CacheResponse};
pub use super::embeddings::{embedding_operation, EmbeddingOperation, EmbeddingResponse};
pub use super::favorites::{favorite_operation, FavoriteOperation, FavoriteResponse};
pub use super::recent_documents::{
    recent_document_operation, RecentDocumentOperation, RecentDocumentResponse,
};
