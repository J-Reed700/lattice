//! Service trait definitions for dependency injection
//!
//! This module defines the trait interfaces for service types in the system.
//! Services represent business logic and coordinate between multiple repositories.
//!
//! # Architecture
//!
//! Following the "bricks and studs" philosophy:
//! - **Studs (Public Interface)**: Service trait methods define operations
//! - **Bricks (Implementations)**: Concrete services implement traits
//! - **Regeneratable**: Can swap implementations (e.g., ONNX vs mock embeddings)
//!
//! # Usage
//!
//! ```rust
//! use crate::infrastructure::services::traits::EmbeddingServiceTrait;
//!
//! async fn process_text<E: EmbeddingServiceTrait>(service: &E, text: &str) -> Result<Vec<f32>> {
//!     service.embed_single(text).await
//! }
//! ```

mod article_extractor;
// Vertical-slice migration (batch): traits live in features/batch/services/.
#[path = "../../../features/batch/services/file_import_trait.rs"]
mod batch_file_import;
#[path = "../../../features/batch/services/url_import_trait.rs"]
mod batch_url_import;
mod chunk; // Migration comment only - trait removed
mod context;
// Vertical-slice migration (conversation): trait lives in features/conversation/trait_def.rs.
#[path = "../../../features/conversation/trait_def.rs"]
mod conversation;
mod document;
// Vertical-slice migration (embedding): trait lives in features/embedding/trait_def.rs.
#[path = "../../../features/embedding/trait_def.rs"]
mod embedding;
mod file_storage;
// Vertical-slice migration (function_calling): trait lives in features/function_calling/trait_def.rs.
#[path = "../../../features/function_calling/trait_def.rs"]
mod function;
// Vertical-slice migration (indexing): trait lives in features/indexing/trait_def.rs.
#[path = "../../../features/indexing/trait_def.rs"]
mod indexing;
// Vertical-slice migration (mentions): trait lives in features/mentions/trait_def.rs.
#[path = "../../../features/mentions/trait_def.rs"]
mod mention;
mod model;
// Vertical-slice migration (qa): trait lives in features/qa/traits.rs.
#[path = "../../../features/qa/traits.rs"]
mod qa;
// Vertical-slice migration (search): trait lives in features/search/trait_def.rs.
#[path = "../../../features/search/trait_def.rs"]
mod search;
// Vertical-slice migration (web): traits live in features/web/traits/.
#[path = "../../../features/web/traits/web.rs"]
mod web;
#[path = "../../../features/web/traits/archive.rs"]
mod web_archive;
#[path = "../../../features/web/traits/capture.rs"]
mod web_capture;

// Re-export all traits
pub use article_extractor::*;
pub use batch_file_import::*;
pub use batch_url_import::*;
pub use context::*;
pub use conversation::*;
pub use embedding::*;
pub use file_storage::*;
pub use function::*;
pub use indexing::*;
pub use model::*;
pub use qa::*;
pub use search::*;
pub use web::*;
pub use web_archive::*;
pub use web_capture::*;
// chunk::* removed - ChunkRepositoryTrait migrated to DDD ports
// document::* removed - migrated to DDD ports
// mention::* removed - migrated to DDD ports

// Re-export service mocks (avoid conflicts with repository mocks)
#[cfg(test)]
pub use crate::infrastructure::services::mocks::{
    MockContextManager, MockConversationService, MockConversationalQAService, MockEmbeddingService,
    MockFileStorageService, MockFunctionExecutor, MockFunctionRegistry, MockIndexingService,
    MockModelManager, MockQAEngine, MockSearchEnrichmentService, MockSearchService,
    MockWebArchiveService, MockWebCaptureService, MockWebIngestionService, MockWebService,
};
