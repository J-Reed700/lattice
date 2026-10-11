//! Repository implementations.
//!
//! This module contains standalone repositories and domain-scoped transactional
//! repository modules.

pub mod chunk_repository;
pub mod document_scope;
pub mod file_library;

// Transaction-aware repository implementations (Tx modules)
pub mod chunk;
pub mod document;
pub mod model_file;
pub mod system;

// Shared repository support.
#[cfg(any(test, feature = "test-utils"))]
pub mod mocks;
pub mod traits;
pub mod unit_of_work;

// Chunk removed - use crate::domain::entities::chunk::Chunk (DDD)
pub use chunk_repository::ChunkRepository; // Repository only, not the old Chunk type
                                           // Document removed - use crate::domain::entities::Document (DDD)
pub use document::SqliteDocumentRepository as DocumentRepository; // Repository only, not the old Document type

// Type aliases for DI container compatibility
pub type DocumentRepositoryImpl = DocumentRepository;
pub type ChunkRepositoryImpl = ChunkRepository;
