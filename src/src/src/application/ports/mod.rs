//! Application layer ports (interfaces).
//!
//! This module defines the port interfaces that infrastructure implementations
//! must satisfy. Ports follow the Hexagonal Architecture pattern, allowing
//! the application core to remain independent of infrastructure details.
//!
//! # Port Types
//!
//! - **Embedding** - Text embedding generation (ONNX, OpenAI, etc.)
//! - **Vector Search** - Similarity search (USearch HNSW)
//! - **Text Search** - Keyword search (SQLite FTS, Tantivy, etc.)
//! - **LLM** - Language model interaction (Ollama, Claude, GPT, etc.)
//! - **File Storage** - File system operations with security validation
//! - **Repository** - Generic data persistence (SQLite, PostgreSQL, etc.)
//! - **Notification** - Event broadcasting (Tauri IPC, webhooks, logs, etc.)
//!
//! # Design Principles
//!
//! 1. **Dependency Inversion** - Application depends on ports, not implementations
//! 2. **Testability** - Mock implementations for testing
//! 3. **Flexibility** - Swap implementations without changing application logic
//! 4. **Thread Safety** - All ports are `Send + Sync`
//!
//! # Example: Using Ports
//!
//! ```rust
//! use crate::application::ports::{EmbeddingPort, VectorSearchPort};
//!
//! async fn index_and_search(
//!     embedder: &impl EmbeddingPort,
//!     searcher: &mut impl VectorSearchPort,
//!     document: &str,
//!     query: &str,
//! ) -> Result<Vec<SearchResult>> {
//!     // Use port interfaces, not concrete implementations
//!     let doc_embedding = embedder.embed_single(document).await?;
//!     searcher.add_embedding("doc-1".into(), doc_embedding)?;
//!
//!     let query_embedding = embedder.embed_single(query).await?;
//!     searcher.search(&query_embedding, 10, 0.7)
//! }
//! ```
//!
//! # Example: Implementing a Port
//!
//! ```rust
//! use crate::application::ports::EmbeddingPort;
//! use async_trait::async_trait;
//!
//! struct OnnxEmbedder {
//!     model: OnnxModel,
//!     dimension: usize,
//! }
//!
//! #[async_trait]
//! impl EmbeddingPort for OnnxEmbedder {
//!     async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
//!         // Infrastructure implementation
//!         self.model.encode(text).await
//!     }
//!
//!     async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
//!         self.model.encode_batch(texts).await
//!     }
//!
//!     fn dimension(&self) -> usize {
//!         self.dimension
//!     }
//! }
//! ```

pub mod backup_port;
pub mod backup_scheduler_port;
pub mod batch_job_repository_port;
pub mod chunk_repository_port;
pub mod content_addressed_storage_port;
pub mod content_extraction_port;
pub mod conversation_context;
pub mod conversation_history_port;
pub mod conversation_repository;
pub mod credentials_port;
pub mod database_stats_port;
pub mod document_repository_port;
pub mod document_scope;
pub mod embedding_port;
pub mod embedding_repository_port;
pub mod favorites_port;
pub mod file_library;
pub mod file_storage_port;
pub mod file_system_port;
pub mod llm_port;
pub mod loaded_chat_model;
pub mod loaded_embedding_model;
pub mod mention_repository_port;
pub mod metrics_port;
pub mod mock_embedding_port;
pub mod model_catalog;
pub mod model_storage;
pub mod notification_port;
pub mod ocr_port;
pub mod recent_documents_port;
pub mod repository_port;
pub mod settings_port;
pub mod settings_side_effects_port;
pub mod sparse_term_store_port;
pub mod system_info;
pub mod text_search_port;
pub mod transcription_port;
pub mod unit_of_work;
pub mod update_checker_port;
pub mod vector_search_port;

pub use backup_port::{BackupInfoData, BackupPort};
pub use backup_scheduler_port::BackupSchedulerPort;
pub use batch_job_repository_port::{
    BatchJobItem, BatchJobItemStatus, BatchJobRepositoryPort, BatchJobStatus,
};
pub use chunk_repository_port::ChunkRepositoryPort;
pub use content_addressed_storage_port::ContentAddressedStoragePort;
pub use content_extraction_port::{ContentExtractionPort, ExtractedContentData};
pub use conversation_history_port::ConversationHistoryPort;
pub use credentials_port::CredentialsPort;
pub use database_stats_port::DatabaseStatsPort;
pub use document_repository_port::DocumentRepositoryPort;
pub use embedding_port::EmbeddingPort;
pub use embedding_repository_port::EmbeddingRepositoryPort;
pub use favorites_port::FavoritesRepositoryPort;
pub use file_storage_port::{FileMetadata, FileStoragePort};
pub use file_system_port::FileSystemPort;
pub use llm_port::{LLMPort, StreamChunk, ToolCall, ToolDefinition};
pub use loaded_chat_model::LoadedChatModelPort;
pub use loaded_embedding_model::LoadedEmbeddingModelPort;
pub use mention_repository_port::{MentionData, MentionRepositoryPort, MentionWithContextData};
pub use metrics_port::{MetricsPort, MetricsSnapshotData};
pub use mock_embedding_port::MockEmbeddingPort;
pub use model_catalog::{ExternalModelMetadata, MockModelCatalogPort, ModelCatalogPort};
pub use model_storage::{DownloadedModel, ModelStoragePort};
pub use notification_port::{events, NotificationPort, SubscriptionHandle};
pub use ocr_port::{NoopOcr, OcrError, OcrPort};
pub use recent_documents_port::RecentDocumentsRepositoryPort;
pub use repository_port::{Filter, NoFilter, RepositoryPort};
pub(crate) use settings_port::merge_json_update;
pub use settings_port::{MockSettingsRepository, SettingsRepositoryPort};
pub use settings_side_effects_port::{NoopSettingsSideEffects, SettingsSideEffectsPort};
pub use sparse_term_store_port::{ChunkSparseTerms, SparseTermStorePort};
pub use system_info::{ComputeType, GpuInfo, SystemInfo, SystemInfoPort};
pub use text_search_port::TextSearchPort;
pub use transcription_port::{Transcript, TranscriptSegment, TranscriptionPort};
pub use unit_of_work::{UnitOfWork, UnitOfWorkFactory};
pub use update_checker_port::{UpdateCheckerPort, UpdateInfoData};
pub use vector_search_port::VectorSearchPort;

// Unified trait for document repository (combines generic + specific)
use crate::domain::entities::Document;

/// Unified document repository trait combining generic and specific interfaces.
///
/// This trait combines `RepositoryPort<Document>` (generic CRUD) with
/// `DocumentRepositoryPort` (document-specific operations) to enable
/// trait object usage without E0225 errors.
///
/// This is a marker trait - implementers must satisfy both parent traits.
///
/// # Purpose
///
/// Rust doesn't allow combining a generic trait with a specific trait in a
/// single trait object (e.g., `dyn RepositoryPort<Document> + DocumentRepositoryPort`
/// causes E0225). This unified trait solves the issue by creating a single
/// trait that extends both.
///
/// # Example
///
/// ```rust
/// // Instead of this (E0225 error):
/// // let repo: Arc<dyn RepositoryPort<Document> + DocumentRepositoryPort>;
///
/// // Use this:
/// let repo: Arc<dyn DocumentRepository>;
/// ```
pub trait DocumentRepository: RepositoryPort<Document> + DocumentRepositoryPort {
    // This is a marker trait - no additional methods needed.
    // Implementers must satisfy both parent traits.
}
