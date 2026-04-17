//! # Domain Module (Phase 1: Pure Domain Layer)
//!
//! Rich domain models implementing Domain-Driven Design (DDD) patterns.
//!
//! ## Architecture
//!
//! This module provides a pure domain layer with **zero external dependencies**
//! (only std, serde, chrono). All infrastructure concerns (database, file I/O,
//! ML models, external services) are handled by the infrastructure layer.
//!
//! ## Structure
//!
//! - **Aggregates**: Aggregate roots with business invariants
//! - **Entities**: Domain entities with identity
//! - **Value Objects**: Immutable, validated domain primitives
//! - **Services**: Pure business logic that doesn't fit in entities
//!
//! ## Aggregates
//!
//! - [`entities::Document`]: Rich document entity (aggregate root) with metadata, chunks, and tags
//!
//! ## Entities
//!
//! - [`Chunk`]: Text chunk within a document
//! - [`Embedding`]: Embedding metadata (no vectors)
//! - [`SearchResult`]: Search result with score
//!
//! ## Value Objects
//!
//! - [`FileMetadata`]: File system metadata
//! - [`Checksum`]: File content checksum
//! - [`ChunkingStrategy`]: Strategy for chunking text
//! - [`SearchQuery`]: Validated search query
//! - [`SearchMode`]: Search algorithm mode (Vector, BM25, Hybrid)
//! - [`IndexingOutcome`]: Result of file indexing operation
//!
//! ## Services
//!
//! - [`ChunkingService`]: Pure text chunking algorithms
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vault_desktop::domain::entities::Document;
//! use vault_desktop::domain::value_objects::ChunkingStrategy;
//! use vault_desktop::shared::domain_types::ValidatedFilePath;
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Document entities are typically created and managed through repositories
//! // See infrastructure::persistence::repositories::document for repository implementations
//! # Ok(())
//! # }
//! ```

// Directory-based domain modules
pub mod aggregates;
pub mod entities;
pub mod events;
pub mod ports;
// Vertical-slice migration (qa): domain models live in features/qa/domain/.
#[path = "../features/qa/domain/mod.rs"]
pub mod qa;
pub mod repositories;
pub mod services;
pub mod value_objects;

// Single-file domain modules moved under modules/ for filesystem organization
// Vertical-slice migration (conversation): domain modules live in features/conversation/domain/.
#[path = "../features/conversation/domain/conversation.rs"]
pub mod conversation;
#[path = "../features/conversation/domain/summary.rs"]
pub mod conversation_summary;
#[path = "modules/curated_models.rs"]
pub mod curated_models;
// Vertical-slice migration (download): domain modules live in features/download/domain/.
#[path = "../features/download/domain/download.rs"]
pub mod download;
#[path = "../features/download/domain/snapshot.rs"]
pub mod download_snapshot;
#[path = "../features/download/domain/downloaded_model.rs"]
pub mod downloaded_model;
#[path = "modules/embedding_constants.rs"]
pub mod embedding_constants;
#[path = "modules/error.rs"]
pub mod error;
#[path = "modules/metadata.rs"]
pub mod metadata;
#[path = "modules/model_catalog.rs"]
pub mod model_catalog;
#[path = "modules/model_file_validator.rs"]
pub mod model_file_validator;
// Vertical-slice migration (model_management): domain module lives in features/model_management/domain.rs.
#[path = "../features/model_management/domain.rs"]
pub mod model_management;
#[path = "modules/model_metadata.rs"]
pub mod model_metadata;
#[path = "modules/model_paths.rs"]
pub mod model_paths;
#[path = "modules/model_type_classifier.rs"]
pub mod model_type_classifier;

// ============================================================================
// Phase 1: New DDD Structure (Pure Domain Layer)
// ============================================================================

// Re-export aggregates
// Document consolidation COMPLETE - types now in entities::document
pub use entities::document::{Document, DocumentStatus};

// Re-export entities
pub use entities::{Chunk, SearchResult};

// Re-export value objects
pub use value_objects::{
    Checksum, ChunkingStrategy, FileMetadata, IndexingOutcome, SearchMode, SearchQuery,
};

// Re-export metadata
pub use metadata::ValidatedMetadata;

// Re-export services
pub use services::ChunkingService;

// Re-export errors
pub use error::DomainError;

// Re-export embedding defaults
pub use embedding_constants::{
    DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, DEFAULT_EMBEDDING_MODEL_NAME,
};

// ============================================================================
// Master Branch Additions (Conversation Feature)
// ============================================================================

// Re-export conversation types from master branch
pub use conversation::{
    Conversation, ConversationAggregate, ConversationMessage, DocumentReference, LLMMessage,
    MessageRole,
};

// Re-export conversation summary
pub use conversation_summary::ConversationSummary;

// Re-export function calling types

// ============================================================================
// Q&A and HyDE Types
// ============================================================================

// Re-export Q&A and HyDE types
pub use qa::{
    ChatResponse, ChunkMetadata, DocumentChunk, EnrichedContext, HyDEInterpretation, QueryType,
    ResponseMetadata, SearchStrategy, Source, ToolIntent,
};

// ============================================================================
// Model Management Types
// ============================================================================

// Re-export model management types
pub use model_management::{
    CompatibilityLevel, CompatibilityScore, CompatibilityScorer, CpuArchitecture, GpuAcceleration,
    GpuType, ModelCategory, ModelMetadata as ModelManagementMetadata, ModelRecommendation,
    PerformanceTier, SystemCapabilities,
};

// Re-export curated model catalog
pub use curated_models::{
    get_all_curated_models, get_curated_embedding_models, get_curated_llm_models,
    get_curated_models_by_category, get_curated_ocr_models,
};

// Re-export model catalog types
pub use model_catalog::{ModelCatalogService, ModelSearchResult, ModelSource, SearchFilters};

// ============================================================================
// Download Management Types
// ============================================================================

// Re-export download types
pub use download::{
    Checksum as DownloadChecksum, ChecksumAlgorithm, DownloadError, DownloadProgress,
    DownloadSession, DownloadState,
};

// Re-export download snapshot types
pub use download_snapshot::{
    BatchSnapshot, DownloadStateSnapshot, DownloadStatus, FileSnapshot, FileStatus,
    SingleFileSnapshot,
};

// Re-export downloaded model
pub use downloaded_model::DownloadedModel;

// Re-export model metadata types
pub use model_metadata::{ModelFileMetadata, ModelMetadata, ModelType};

// Re-export classifier
pub use model_type_classifier::{
    ClassificationStrategy, ModelIdentifier, ModelTypeClassification, ModelTypeClassifier,
};

// Re-export model paths and validator
pub use model_file_validator::ModelFileValidator;
pub use model_paths::ModelPaths;

// ============================================================================
// Test Modules
// ============================================================================
// Test modules deleted - Oracle Phase 1 Safety Net stabilization

// ============================================================================
// Verify Zero External Dependencies
// ============================================================================

// This will cause a compilation error if any unused external dependencies
// are present in the domain layer
