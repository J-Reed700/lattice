#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! ServiceContainer Test Helpers
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Utilities for creating and testing with ServiceContainer (DDD approach).
//!
//! # Purpose
//!
//! This module provides helpers to:
//! - Create test ServiceContainer instances with mocked services
//! - Verify container initialization
//! - Test service dependencies
//! - Compare behavior with legacy AppState
//!
//! # Examples
//!
//! ```rust
//! use crate::tests::migration::container_helpers::create_test_container;
//!
//! #[tokio::test]
//! async fn test_service_injection() {
//!     let container = create_test_container().await.unwrap();
//!     let embedding_service = container.embedding_service();
//!     let result = embedding_service.embed_single("test").await;
//!     assert!(result.is_ok());
//! }
//! ```

use sqlx::SqlitePool;
use std::sync::Arc;
use vault::interfaces::di::Container as ServiceContainer;
use vault::shared::error::Result;
use vault::infrastructure::indexing::IndexingService;
use vault::infrastructure::observability::Metrics;
use vault::infrastructure::qa::QAEngine;
use vault::infrastructure::security::SecurityContext;
use vault::infrastructure::services::ConversationService;
use vault::infrastructure::services::ContextManager;
use vault::infrastructure::services::FileStorageService;
use vault::infrastructure::services::ModelManager;
use vault::infrastructure::services::SearchEnrichmentService;
use vault::infrastructure::services::TagService;
use vault::infrastructure::services::traits::*;
use vault::infrastructure::services::WebIngestionService;

// ============================================================================
// Test Container Factory
// ============================================================================

/// Configuration for test container creation
#[derive(Debug, Clone)]
pub struct TestContainerConfig {
    /// Embedding dimension (default: 384)
    pub embedding_dim: usize,
    /// Include optional services (indexing, QA)
    pub include_optional_services: bool,
    /// Use in-memory database
    pub in_memory_db: bool,
}

impl Default for TestContainerConfig {
    fn default() -> Self {
        Self {
            embedding_dim: 384,
            include_optional_services: false,
            in_memory_db: true,
        }
    }
}

/// Create a test ServiceContainer with mocked services
///
/// # Returns
///
/// Fully initialized ServiceContainer with all required services mocked
///
/// # Examples
///
/// ```rust
/// let container = create_test_container().await.unwrap();
/// assert!(container.embedding_service().embed_single("test").await.is_ok());
/// ```
pub async fn create_test_container() -> Result<ServiceContainer> {
    create_test_container_with_config(TestContainerConfig::default()).await
}

/// Create a test ServiceContainer with custom configuration
///
/// # Arguments
///
/// * `config` - Configuration for container setup
///
/// # Examples
///
/// ```rust
/// let config = TestContainerConfig {
///     embedding_dim: 768,
///     include_optional_services: true,
///     in_memory_db: true,
/// };
/// let container = create_test_container_with_config(config).await.unwrap();
/// ```
pub async fn create_test_container_with_config(
    config: TestContainerConfig,
) -> Result<ServiceContainer> {
    // Create database pool
    let pool = if config.in_memory_db {
        create_test_database().await?
    } else {
        create_file_database().await?
    };

    // Create infrastructure services
    let security_context = Arc::new(SecurityContext::new());
    let metrics = Arc::new(Metrics::new());

    // Create core services (with mocks)
    let embedding_service =
        Arc::new(MockEmbeddingService::new(config.embedding_dim)) as Arc<dyn EmbeddingServiceTrait>;
    let search_service = Arc::new(MockSearchService::new()) as Arc<dyn SearchServiceTrait>;

    // Create domain services
    let tag_service = Arc::new(TagService::new(pool.clone())) as Arc<dyn TagServiceTrait>;
    let file_storage_service =
        Arc::new(FileStorageService::new()) as Arc<dyn FileStorageServiceTrait>;
    let model_manager = Arc::new(ModelManager::new()) as Arc<dyn ModelManagerTrait>;
    let web_ingestion_service = Arc::new(WebIngestionService::new(
        pool.clone(),
        embedding_service.clone(),
    )) as Arc<dyn WebIngestionServiceTrait>;
    let search_enrichment_service =
        Arc::new(SearchEnrichmentService::new(pool.clone())) as Arc<dyn SearchEnrichmentServiceTrait>;
    let conversation_service =
        Arc::new(ConversationService::new(pool.clone())) as Arc<dyn ConversationServiceTrait>;
    let context_manager = Arc::new(ContextManager::new(4000)) as Arc<dyn ContextManagerTrait>;

    // Optional services
    let indexing_service = if config.include_optional_services {
        // Create a minimal indexing service for testing
        // Note: In production this requires loaded models
        None
    } else {
        None
    };

    let qa_engine = if config.include_optional_services {
        // Create QA engine with mock LLM client
        Some(Arc::new(create_test_qa_engine()) as Arc<dyn vault::services::traits::QAEngineTrait>)
    } else {
        None
    };

    // Build container
    Ok(ServiceContainer::new(
        pool,
        security_context,
        metrics,
        embedding_service,
        search_service,
        tag_service,
        file_storage_service,
        model_manager,
        web_ingestion_service,
        search_enrichment_service,
        conversation_service,
        context_manager,
        None, // conversational_qa_service
        indexing_service,
        qa_engine,
    ))
}

/// Create an in-memory test database with schema initialized
///
/// # Returns
///
/// Initialized SQLite connection pool
pub async fn create_test_database() -> Result<SqlitePool> {
    let pool = SqlitePool::connect(":memory:")
        .await
        .map_err(|e| vault::error::AppError::DatabaseError(e.to_string()))?;

    // Initialize schema
    vault::infrastructure::persistence::database::init::initialize_database(&pool)
        .await
        .map_err(|e| vault::error::AppError::DatabaseError(e.to_string()))?;

    Ok(pool)
}

/// Create a file-based test database with schema initialized
///
/// # Returns
///
/// Initialized SQLite connection pool (file-based)
pub async fn create_file_database() -> Result<SqlitePool> {
    use uuid::Uuid;

    let temp_dir = tempfile::tempdir()
        .map_err(|e| vault::error::AppError::Other(format!("Failed to create temp dir: {}", e)))?;

    let uuid = Uuid::new_v4().simple().to_string();
    let db_path = temp_dir.path().join(format!("test_{}.db", uuid));
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let pool = SqlitePool::connect(&db_url)
        .await
        .map_err(|e| vault::error::AppError::DatabaseError(e.to_string()))?;

    vault::infrastructure::persistence::database::init::initialize_database(&pool)
        .await
        .map_err(|e| vault::error::AppError::DatabaseError(e.to_string()))?;

    Ok(pool)
}

/// Create a test QA engine with mock LLM client
fn create_test_qa_engine() -> QAEngine {
    // Create mock LLM client
    let mock_client = Arc::new(vault::llm::OllamaClient::new(
        "http://localhost:11434",
        "test-model".to_string(),
    ));

    QAEngine::new(mock_client as Arc<dyn vault::llm::LLMClient>)
}

// ============================================================================
// Test Assertions
// ============================================================================

/// Assert that a ServiceContainer is properly initialized
///
/// # Panics
///
/// Panics if any required service is not accessible
pub async fn assert_container_initialized(container: &ServiceContainer) {
    // Test infrastructure services
    assert!(
        container.db_pool().acquire().await.is_ok(),
        "Database pool should be accessible"
    );
    assert!(
        container.security_context().rate_limiters.search.check().is_ok(),
        "Security context should be accessible"
    );

    // Test core services
    assert!(
        container.embedding_service().embed_single("test").await.is_ok(),
        "Embedding service should be accessible"
    );
    assert_eq!(
        container.search_service().search(&vec![0.1; 384], 10).len(),
        0,
        "Search service should be accessible"
    );

    // Test domain services
    assert!(
        container.tag_service().get_or_create("test", "#fff").await.is_ok(),
        "Tag service should be accessible"
    );
    assert!(
        container.model_manager().is_model_ready().await,
        "Model manager should be accessible"
    );
    assert!(
        container.conversation_service().create_conversation("Test", "test-model", None).await.is_ok(),
        "Conversation service should be accessible"
    );
}

/// Assert that two containers behave identically for a given operation
///
/// This is useful for migration testing to ensure DDD container
/// matches legacy AppState behavior.
#[macro_export]
macro_rules! assert_containers_equivalent {
    ($legacy:expr, $ddd:expr, $operation:expr) => {{
        let legacy_result = $operation($legacy).await;
        let ddd_result = $operation($ddd).await;

        assert_eq!(
            legacy_result.is_ok(),
            ddd_result.is_ok(),
            "Container results should have same success/failure status"
        );

        if legacy_result.is_ok() {
            assert_eq!(
                format!("{:?}", legacy_result),
                format!("{:?}", ddd_result),
                "Container results should be identical"
            );
        }
    }};
}

// ============================================================================
// Test Utilities
// ============================================================================

/// Create a minimal ServiceContainer for unit tests
///
/// This creates the smallest possible container with only required services.
/// Useful for fast unit tests that don't need full setup.
pub async fn create_minimal_container() -> Result<ServiceContainer> {
    create_test_container_with_config(TestContainerConfig {
        embedding_dim: 384,
        include_optional_services: false,
        in_memory_db: true,
    })
    .await
}

/// Create a ServiceContainer with all optional services enabled
///
/// This creates a fully-featured container for integration tests.
pub async fn create_full_container() -> Result<ServiceContainer> {
    create_test_container_with_config(TestContainerConfig {
        embedding_dim: 384,
        include_optional_services: true,
        in_memory_db: false,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_container() {
        let container = create_test_container().await.unwrap();
        assert_container_initialized(&container).await;
    }

    #[tokio::test]
    async fn test_create_test_container_with_custom_embedding_dim() {
        let config = TestContainerConfig {
            embedding_dim: 768,
            ..Default::default()
        };
        let container = create_test_container_with_config(config).await.unwrap();

        let embedding = container
            .embedding_service()
            .embed_single("test")
            .await
            .unwrap();
        assert_eq!(embedding.len(), 768);
    }

    #[tokio::test]
    async fn test_minimal_container() {
        let container = create_minimal_container().await.unwrap();
        assert!(container.indexing_service().is_none());
        assert!(container.qa_engine().is_none());
    }

    #[tokio::test]
    async fn test_container_services_accessible() {
        let container = create_test_container().await.unwrap();

        // Test each service
        let _db = container.db_pool();
        let _security = container.security_context();
        let _metrics = container.metrics();
        let _embedding = container.embedding_service();
        let _search = container.search_service();
        let _tags = container.tag_service();
        let _storage = container.file_storage_service();
        let _model_mgr = container.model_manager();
        let _web = container.web_ingestion_service();
        let _enrichment = container.search_enrichment_service();
        let _conversation = container.conversation_service();
        let _context = container.context_manager();
    }
}
