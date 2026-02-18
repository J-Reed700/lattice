//! Repository Implementations
//!
//! This module contains concrete implementations of repository ports defined
//! in the application layer. Repositories encapsulate data access logic.
//!
//! # Migrated Modules
//! - [x] document_repository.rs - Migrated from `repositories/document_repository.rs`
//! - [x] chunk_repository.rs - Migrated from `repositories/chunk_repository.rs`
//! - [x] conversation_repository.rs - Conversation management with messages
//! - [x] settings_repository.rs - File-based JSON settings storage
//! - [x] daily_notes_repository.rs - Daily notes with date-based navigation
//! - [ ] embedding_repository.rs - TODO: Migrate from `repositories/embedding_repository.rs`
//! - [ ] tag_repository.rs - TODO: Migrate from `repositories/tag_repository.rs`
//! - [ ] mention_repository.rs - TODO: Migrate from `repositories/mention_repository.rs`
//! - [x] favorites_repository.rs - Favorites management
//! - [x] recent_documents_repository.rs - Recent document tracking

pub mod batch_job_repository;
pub mod chunk_repository;
pub mod conversation_repository;
pub mod custom_model_repository;
// DELETED: pub mod daily_notes_repository; - Feature removed
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

// UnitOfWork pattern implementation
pub mod unit_of_work;

// Trait definitions for dependency injection
pub mod traits;

// Mock implementations for testing (also used in production DI container)
pub mod mocks;

// Re-export main types for easier access
pub use batch_job_repository::BatchJobRepository;
// Chunk removed - use crate::domain::entities::chunk::Chunk (DDD)
pub use chunk_repository::ChunkRepository; // Repository only, not the old Chunk type
pub use conversation_repository::ConversationRepository;
pub use custom_model_repository::{
    CustomModelRepository, CustomModelRepositoryTrait, MockCustomModelRepository,
};
// DELETED: pub use daily_notes_repository::DailyNotesRepository; - Feature removed
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
