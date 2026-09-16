//! Repository implementations.
//!
//! This module contains standalone repositories and domain-scoped transactional
//! repository modules.

pub mod batch_job_repository;
pub mod chunk_repository;
pub mod document_repository;

// Transaction-aware repository implementations (Tx modules)
pub mod batch_job;
pub mod chunk;
pub mod document;
pub mod model_file;
pub mod system;

// Support modules grouped under support/ for filesystem organization.
pub mod mocks;
pub mod traits;
pub mod unit_of_work;

pub use batch_job_repository::BatchJobRepository;
// Chunk removed - use crate::domain::entities::chunk::Chunk (DDD)
pub use chunk_repository::ChunkRepository; // Repository only, not the old Chunk type
                                           // Document removed - use crate::domain::entities::Document (DDD)
pub use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
pub use crate::features::embedding::repository::{Embedding, EmbeddingRepository};
pub use crate::features::mentions::repository::MentionRepository;
pub use crate::features::settings::repository::SettingsRepository;
pub use crate::features::tags::repository::TagRepository;
pub use document_repository::DocumentRepository; // Repository only, not the old Document type

// Type aliases for DI container compatibility
pub type DocumentRepositoryImpl = DocumentRepository;
pub type ChunkRepositoryImpl = ChunkRepository;
pub type TagRepositoryImpl = TagRepository;
