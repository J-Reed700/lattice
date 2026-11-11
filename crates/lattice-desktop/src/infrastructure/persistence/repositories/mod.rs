//! Repository implementations.
//!
//! This module contains standalone repositories and domain-scoped transactional
//! repository modules.

pub mod document_scope;
pub mod file_library;
pub mod mentions;
pub use lattice_store::workspace_notes;

// Transaction-aware repository implementations (Tx modules)
pub mod chunk;
pub mod document;
pub mod downloaded_models;
pub mod embedding;
pub mod model;
pub mod model_file;
pub(crate) mod models_table_type;

// Shared repository support.
#[cfg(any(test, feature = "test-utils"))]
pub mod mocks;
pub mod traits;

// Chunk removed - use lattice_core::domain::entities::chunk::Chunk (DDD)
pub use chunk::ChunkRepository;
// Document removed - use lattice_core::domain::entities::Document (DDD)
pub use document::SqliteDocumentRepository as DocumentRepository; // Repository only, not the old Document type

// Type aliases for DI container compatibility
pub type DocumentRepositoryImpl = DocumentRepository;
pub type ChunkRepositoryImpl = ChunkRepository;
