//! Repository implementations.
//!
//! This module contains standalone repositories and domain-scoped transactional
//! repository modules.

pub mod batch_job_repository;
pub mod chunk_repository;
// Vertical-slice migration (conversation): repository lives in features/conversation/repository.rs.
#[path = "../../../features/conversation/repository.rs"]
pub mod conversation_repository;
pub mod document_repository;
// Vertical-slice migration (settings): repository lives in features/settings/repository.rs.
#[path = "../../../features/settings/repository.rs"]
pub mod settings_repository;
pub mod summary_repository;

// Transaction-aware repository implementations (Tx modules)
pub mod batch_job;
pub mod chunk;
pub mod document;
// Vertical-slice migration (model_management): tx-wrapper repository lives in features/model_management/repository_tx/.
#[path = "../../../features/model_management/repository_tx/mod.rs"]
pub mod model;
pub mod model_file;
// Vertical-slice migration (search): tx-wrapper repository lives in features/search/repository_tx/.
#[path = "../../../features/search/repository_tx/mod.rs"]
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
// Document removed - use crate::domain::entities::Document (DDD)
pub use document_repository::DocumentRepository; // Repository only, not the old Document type
pub use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
pub use crate::features::embedding::repository::{Embedding, EmbeddingRepository};
pub use crate::features::mentions::repository::MentionRepository;
pub use settings_repository::SettingsRepository;
pub use summary_repository::SummaryRepository;
pub use crate::features::tags::repository::TagRepository;

// Type aliases for DI container compatibility
pub type DocumentRepositoryImpl = DocumentRepository;
pub type ChunkRepositoryImpl = ChunkRepository;
pub type TagRepositoryImpl = TagRepository;
