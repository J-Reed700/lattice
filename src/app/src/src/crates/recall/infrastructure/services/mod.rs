pub mod backup_scheduler;
pub mod conversation_service;
pub mod database;
pub mod download_engine;
pub mod download_manager;
pub mod embedding;
pub mod file_cleanup;
pub mod file_storage;
pub mod file_type_detector;
pub mod file_watch;
pub mod llm_cache;
pub mod metadata_extraction;
pub mod model_manager;
pub mod router;
pub mod startup_reconciliation;
pub mod sync;
pub mod tag_service;
pub mod tag_service_impl;
pub mod validated_path;

// Business logic services (extracted from fat controllers)
pub mod context_manager;
pub mod conversational_qa_service;
pub mod search_enrichment_service;

// Re-export search enrichment types
pub use search_enrichment_service::{DocumentMetadata, SearchEnrichmentService};

// Re-export metadata extraction service
pub use metadata_extraction::MetadataExtractor;
pub use router::RouterService;

// Function calling services (LLM tools integration)
pub mod article_extractor;
pub mod batch_file_import;
pub mod batch_url_import;
pub mod function_executor;
pub mod function_registry;
pub mod web_archive_service;
pub mod web_capture;
pub mod web_ingestion;
pub mod web_service;

// Service trait definitions for dependency injection
pub mod traits;

// Mock implementations for testing
pub mod mocks;

// Re-export conversation service
pub use backup_scheduler::BackupScheduler;
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

// HyDE query processing
pub mod hyde;

// Custom model services
pub mod custom_model;

// Tests module
#[cfg(test)]
pub mod tests;
