//! Repository implementations.
//!
//! This module contains standalone repositories and domain-scoped transactional
//! repository modules.

pub mod chunk_repository;
pub mod document_scope;
pub mod file_library;
pub mod workspace_notes;

// Transaction-aware repository implementations (Tx modules)
pub mod chunk;
pub mod document;
pub mod embedding;
pub mod model;
pub mod model_file;
pub mod system;

// Shared repository support.
#[cfg(any(test, feature = "test-utils"))]
pub mod mocks;
pub mod traits;
pub mod unit_of_work;

// Chunk removed - use lattice_core::domain::entities::chunk::Chunk (DDD)
pub use chunk_repository::ChunkRepository; // Repository only, not the old Chunk type
                                           // Document removed - use lattice_core::domain::entities::Document (DDD)
pub use document::SqliteDocumentRepository as DocumentRepository; // Repository only, not the old Document type

// Type aliases for DI container compatibility
pub type DocumentRepositoryImpl = DocumentRepository;
pub type ChunkRepositoryImpl = ChunkRepository;
