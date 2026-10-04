//! Shared business entities, value objects, and rules.
//!
//! `conversation`, `download`, and `models` group domain-specific concepts.
//! `entities`, `value_objects`, and `services` hold shared document and search
//! primitives. Repository traits and external-access ports define boundaries;
//! implementations belong to infrastructure or the owning feature.

pub mod conversation;
pub mod download;
pub mod entities;
pub mod error;
pub mod models;
pub mod ports;
pub mod qa;
pub mod repositories;
pub mod services;
pub mod value_objects;

pub use entities::document::{Document, DocumentStatus};

pub use entities::{Chunk, SearchResult};

pub use value_objects::{
    Checksum, ChunkingStrategy, FileMetadata, IndexingOutcome, SearchMode, SearchQuery,
};

pub use value_objects::metadata::ValidatedMetadata;

pub use services::ChunkingService;

pub use error::DomainError;

pub use models::embedding_defaults::{
    DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_CURATED_ID,
    DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, DEFAULT_EMBEDDING_MODEL_NAME,
};

pub use conversation::{
    CompactionRecord, Conversation, ConversationAggregate, ConversationMessage, DocumentReference,
    LLMMessage, MessageRole,
};

pub use qa::{
    ChatResponse, ChunkMetadata, DocumentChunk, EnrichedContext, HyDEInterpretation, QueryType,
    ResponseMetadata, SearchStrategy, Source, ToolIntent,
};

pub use models::selection::{
    CompatibilityLevel, CompatibilityScore, CompatibilityScorer, CpuArchitecture, GpuAcceleration,
    GpuType, ModelCategory, ModelFormat, ModelMetadata as ModelManagementMetadata,
    ModelRecommendation, PerformanceTier, SystemCapabilities,
};

pub use models::curated::{
    get_all_curated_models, get_curated_embedding_models, get_curated_llm_models,
    get_curated_models_by_category, get_curated_ocr_models,
};

pub use models::catalog::{ModelCatalogService, ModelSearchResult, ModelSource, SearchFilters};

pub use download::{
    Checksum as DownloadChecksum, ChecksumAlgorithm, DownloadError, DownloadProgress,
    DownloadSession, DownloadState,
};

pub use download::snapshot::{
    BatchSnapshot, DownloadStateSnapshot, DownloadStatus, FileSnapshot, FileStatus,
    SingleFileSnapshot,
};

pub use models::downloaded::DownloadedModel;

pub use models::metadata::{ModelFileMetadata, ModelMetadata, ModelType};

pub use models::classifier::{
    ClassificationStrategy, ModelIdentifier, ModelTypeClassification, ModelTypeClassifier,
};

pub use models::paths::ModelPaths;
pub use models::validation::ModelFileValidator;
