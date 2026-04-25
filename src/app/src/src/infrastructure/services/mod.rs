// Domain service modules moved under domains/ for filesystem organization.
#[path = "domains/article_extractor.rs"]
pub mod article_extractor;
#[path = "domains/context_manager.rs"]
pub mod context_manager;
// Vertical-slice migration (conversation): service lives in features/conversation/service.rs.
#[path = "../../features/conversation/service.rs"]
pub mod conversation_service;
#[path = "domains/file_cleanup.rs"]
pub mod file_cleanup;
#[path = "domains/file_type_detector.rs"]
pub mod file_type_detector;
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
// Validated path used by FileCleanupService (download manager dep). Distinct
// from `shared::domain_types::ValidatedFilePath`; do not consolidate without
// migrating FileCleanupService's API expectations.
#[path = "domains/validated_path.rs"]
pub mod validated_path;

// Directory-backed service modules
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
pub use crate::features::tags::service_impl::TagServiceImpl;

// Re-export function calling services
pub use article_extractor::ArticleExtractorService;
pub use crate::features::batch::services::file_import::BatchFileImportService;
pub use crate::features::batch::services::url_import::BatchUrlImportService;
pub use crate::features::function_calling::executor::FunctionExecutor;
pub use crate::features::function_calling::registry::{
    init_function_registry, register_custom_query_tools, FunctionRegistry,
};
pub use crate::features::web::services::archive::WebArchiveService;
pub use crate::features::web::services::capture::WebCaptureService;
pub use crate::features::web::services::ingestion::{WebIngestionConfig, WebIngestionService, WebIngestionServiceBuilder};
pub use crate::features::web::services::web::WebService;

// Re-export search services
pub use crate::infrastructure::search::hybrid::HybridSearchService;
