// Domain service modules moved under domains/ for filesystem organization.
#[path = "domains/article_extractor.rs"]
pub mod article_extractor;
// Vertical-slice migration (batch): services live in features/batch/services/.
#[path = "../../features/batch/services/file_import.rs"]
pub mod batch_file_import;
#[path = "../../features/batch/services/url_import.rs"]
pub mod batch_url_import;
#[path = "domains/context_manager.rs"]
pub mod context_manager;
// Vertical-slice migration (conversation): service lives in features/conversation/service.rs.
#[path = "../../features/conversation/service.rs"]
pub mod conversation_service;
// Vertical-slice migration (qa): conversational service lives in features/qa/.
#[path = "../../features/qa/conversational_service.rs"]
pub mod conversational_qa_service;
#[path = "domains/database.rs"]
pub mod database;
// Vertical-slice migration (download): engine + manager live in features/download/.
#[path = "../../features/download/engine.rs"]
pub mod download_engine;
#[path = "../../features/download/manager.rs"]
pub mod download_manager;
#[path = "domains/file_cleanup.rs"]
pub mod file_cleanup;
#[path = "domains/file_type_detector.rs"]
pub mod file_type_detector;
#[path = "domains/file_watch.rs"]
pub mod file_watch;
// Vertical-slice migration (function_calling): executor + registry live in features/function_calling/.
#[path = "../../features/function_calling/executor.rs"]
pub mod function_executor;
#[path = "../../features/function_calling/registry.rs"]
pub mod function_registry;
#[path = "domains/metadata_extraction.rs"]
pub mod metadata_extraction;
#[path = "domains/model_manager.rs"]
pub mod model_manager;
#[path = "domains/router.rs"]
pub mod router;
// Vertical-slice migration (search): enrichment service lives in features/search/enrichment_service.rs.
#[path = "../../features/search/enrichment_service.rs"]
pub mod search_enrichment_service;
#[path = "domains/startup_reconciliation.rs"]
pub mod startup_reconciliation;
#[path = "domains/sync.rs"]
pub mod sync;
// Vertical-slice migration (tags): services live in features/tags/.
#[path = "../../features/tags/service.rs"]
pub mod tag_service;
#[path = "../../features/tags/service_impl.rs"]
pub mod tag_service_impl;
#[path = "domains/validated_path.rs"]
pub mod validated_path;
// Vertical-slice migration (web): services live in features/web/services/.
#[path = "../../features/web/services/archive.rs"]
pub mod web_archive_service;
#[path = "../../features/web/services/capture.rs"]
pub mod web_capture;
#[path = "../../features/web/services/ingestion.rs"]
pub mod web_ingestion;
#[path = "../../features/web/services/web.rs"]
pub mod web_service;

// Directory-backed service modules
// Vertical-slice migration (embedding): service directory lives in features/embedding/service/.
#[path = "../../features/embedding/service/mod.rs"]
pub mod embedding;
pub mod file_storage;
// Vertical-slice migration (qa): HyDE retrieval lives in features/qa/hyde/.
#[path = "../../features/qa/hyde/mod.rs"]
pub mod hyde;
pub mod mocks;

// Service trait definitions for dependency injection
pub mod traits;

// Tests module
#[cfg(test)]
pub mod tests;

// Re-export search enrichment types
pub use search_enrichment_service::{DocumentMetadata, SearchEnrichmentService};

// Re-export metadata extraction service
pub use metadata_extraction::MetadataExtractor;
pub use router::RouterService;

// Re-export conversation service
pub use conversation_service::ConversationService;

// Re-export tag service implementation
pub use tag_service_impl::TagServiceImpl;

// Re-export function calling services
pub use article_extractor::ArticleExtractorService;
pub use batch_file_import::BatchFileImportService;
pub use batch_url_import::BatchUrlImportService;
pub use function_executor::FunctionExecutor;
pub use function_registry::{
    init_function_registry, register_custom_query_tools, FunctionRegistry,
};
pub use web_archive_service::WebArchiveService;
pub use web_capture::WebCaptureService;
pub use web_ingestion::{WebIngestionConfig, WebIngestionService, WebIngestionServiceBuilder};
pub use web_service::WebService;

// Re-export search services
pub use crate::infrastructure::search::hybrid::HybridSearchService;
