//! Repository implementations.
//!
//! This module contains standalone repositories and domain-scoped transactional
//! repository modules.

pub mod batch_job_repository;
pub mod chunk_repository;
pub mod conversation_repository;
pub mod custom_model_repository;
pub mod document_repository;
pub mod downloaded_model_repository;
pub mod embedding_repository;
pub mod favorites_repository;
pub mod mention_repository;
pub mod recent_documents_repository;
pub mod settings_repository;
pub mod summary_repository;
pub mod tag_repository;

// Transaction-aware repository implementations (Tx modules)
pub mod batch_job;
pub mod chunk;
pub mod document;
pub mod embedding;
pub mod model;
pub mod model_file;
pub mod search;
pub mod system;

// Support modules grouped under support/ for filesystem organization.
#[path = "support/mocks.rs"]
pub mod mocks;
#[path = "support/traits.rs"]
pub mod traits;
#[path = "support/unit_of_work.rs"]
pub mod unit_of_work;

// Re-export main types for easier access
pub use batch_job_repository::BatchJobRepository;
// Chunk removed - use crate::domain::entities::chunk::Chunk (DDD)
pub use chunk_repository::ChunkRepository; // Repository only, not the old Chunk type
pub use conversation_repository::ConversationRepository;
pub use custom_model_repository::{
    CustomModelRepository, CustomModelRepositoryTrait, MockCustomModelRepository,
};
// Document removed - use crate::domain::entities::Document (DDD)
pub use document_repository::DocumentRepository; // Repository only, not the old Document type
pub use downloaded_model_repository::DownloadedModelRepository;
pub use embedding_repository::{Embedding, EmbeddingRepository};
pub use favorites_repository::FavoritesRepository;
pub use mention_repository::MentionRepository;
pub use recent_documents_repository::RecentDocumentsRepository;
pub use settings_repository::SettingsRepository;
pub use summary_repository::SummaryRepository;
pub use tag_repository::TagRepository;

// Type aliases for DI container compatibility
pub type DocumentRepositoryImpl = DocumentRepository;
pub type ChunkRepositoryImpl = ChunkRepository;
pub type TagRepositoryImpl = TagRepository;
