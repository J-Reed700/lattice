// Domain service modules moved under domains/ for filesystem organization.
pub mod article_extractor;
pub mod context_manager;
pub mod file_cleanup;
pub mod file_type_detector;
pub mod metadata_extraction;
pub mod model_manager;
pub mod router;
pub mod startup_reconciliation;
// Validated path used by FileCleanupService (download manager dep). Distinct
// from `shared::domain_types::ValidatedFilePath`; do not consolidate without
// migrating FileCleanupService's API expectations.
pub mod validated_path;

// Directory-backed service modules
#[cfg(test)]
pub mod mocks;

// Service trait definitions for dependency injection
pub mod traits;

// Tests module
#[cfg(test)]
pub mod tests;

pub use metadata_extraction::MetadataExtractor;
pub use router::RouterService;

pub use crate::features::tags::service_impl::TagServiceImpl;

pub use crate::features::batch::services::file_import::BatchFileImportService;
pub use crate::features::batch::services::url_import::BatchUrlImportService;
pub use crate::features::function_calling::executor::FunctionExecutor;
pub use crate::features::function_calling::registry::{
    init_function_registry, register_custom_query_tools, FunctionRegistry,
};
pub use crate::features::web::services::archive::WebArchiveService;
pub use crate::features::web::services::capture::WebCaptureService;
pub use crate::features::web::services::ingestion::{
    WebIngestionConfig, WebIngestionService, WebIngestionServiceBuilder,
};
pub use crate::features::web::services::web::WebService;
pub use article_extractor::ArticleExtractorService;

pub use crate::features::search::engine::hybrid::HybridSearchService;
