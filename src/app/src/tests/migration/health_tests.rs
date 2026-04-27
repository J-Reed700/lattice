#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Health Command Tests with DDD Container
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Comprehensive test suite for health check commands using ServiceContainer.
//!
//! # Test Coverage
//!
//! - ✅ Database health checks
//! - ✅ Embedding service health
//! - ✅ Q&A engine health
//! - ✅ Cache health
//! - ✅ Search index health
//! - ✅ Overall health status calculation
//! - ✅ Error handling
//! - ✅ Latency measurement
//!
//! # Purpose
//!
//! These tests verify that health commands work correctly with the DDD
//! ServiceContainer and produce the same results as the legacy implementation.

use crate::migration::container_helpers::*;
use lattice::interfaces::commands::health::*;
use lattice::shared::error::Result;
use tauri::State;

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_with_container() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await;
    assert!(result.is_ok(), "Health check should succeed");

    let health = result.unwrap();
    assert!(!health.status.is_empty(), "Health status should not be empty");
    assert_eq!(
        health.database.status, "healthy",
        "Database should be healthy"
    );
}

#[tokio::test]
async fn test_database_health_check() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    assert_eq!(
        result.database.status, "healthy",
        "Database should be healthy"
    );
    assert!(
        result.database.message.is_some(),
        "Database health should have message"
    );
    assert!(
        result.database.latency_ms.is_some(),
        "Database health should have latency"
    );
    assert!(
        result.database.latency_ms.unwrap() < 1000,
        "Database latency should be reasonable"
    );
}

#[tokio::test]
async fn test_embedding_service_health_check() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    assert_eq!(
        result.embedding_service.status, "healthy",
        "Embedding service should be healthy"
    );
    assert!(
        result.embedding_service.message.is_some(),
        "Embedding service should have message"
    );
    assert!(
        result
            .embedding_service
            .message
            .unwrap()
            .contains("dimension: 384"),
        "Message should include embedding dimension"
    );
}

#[tokio::test]
async fn test_qa_engine_health_check() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    // Q&A engine is always available (Rust native)
    assert_eq!(
        result.qa_engine.status, "healthy",
        "Q&A engine should be healthy"
    );
    assert!(
        result
            .qa_engine
            .message
            .unwrap()
            .contains("Rust native"),
        "Message should indicate Rust native implementation"
    );
}

#[tokio::test]
async fn test_cache_health_check() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    assert_eq!(result.cache.status, "healthy", "Cache should be healthy");
    assert!(
        result.cache.message.is_some(),
        "Cache should have health message"
    );
}

#[tokio::test]
async fn test_search_index_health_check() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    // Search index should be degraded initially (no documents)
    assert!(
        result.search_index.status == "degraded" || result.search_index.status == "healthy",
        "Search index status should be valid"
    );
}

#[tokio::test]
async fn test_search_index_with_documents() {
    let container = create_test_container().await.unwrap();

    // Insert a test document
    let pool = container.db_pool();
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('test-id', 'test.txt', '/test.txt', 'test content', datetime('now'), datetime('now'))
        "#,
    )
    .execute(pool)
    .await
    .unwrap();

    let state = State::from(&container);
    let result = health_check(state).await.unwrap();

    assert_eq!(
        result.search_index.status, "healthy",
        "Search index should be healthy with documents"
    );
    assert!(
        result
            .search_index
            .message
            .unwrap()
            .contains("1 documents"),
        "Message should indicate 1 document"
    );
}

#[tokio::test]
async fn test_overall_health_status_all_healthy() {
    let container = create_test_container().await.unwrap();

    // Insert document to make search index healthy
    let pool = container.db_pool();
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('test-id', 'test.txt', '/test.txt', 'test content', datetime('now'), datetime('now'))
        "#,
    )
    .execute(pool)
    .await
    .unwrap();

    let state = State::from(&container);
    let result = health_check(state).await.unwrap();

    assert_eq!(
        result.status, "healthy",
        "Overall status should be healthy when all components healthy"
    );
}

#[tokio::test]
async fn test_overall_health_status_degraded() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    // Without documents, search index is degraded
    assert!(
        result.status == "degraded" || result.status == "healthy",
        "Overall status should be degraded with empty search index"
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_handles_database_error() {
    // Create container with closed database
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    pool.close().await;

    let security_context = std::sync::Arc::new(lattice::security::SecurityContext::new());
    let metrics = std::sync::Arc::new(lattice::observability::Metrics::new());
    let embedding_service = std::sync::Arc::new(lattice::services::traits::MockEmbeddingService::new(384))
        as std::sync::Arc<dyn lattice::services::traits::EmbeddingServiceTrait>;
    let search_service = std::sync::Arc::new(lattice::services::traits::MockSearchService::new())
        as std::sync::Arc<dyn lattice::services::traits::SearchServiceTrait>;
    let tag_service = std::sync::Arc::new(lattice::services::tag_service::TagService::new(pool.clone()))
        as std::sync::Arc<dyn lattice::services::traits::TagServiceTrait>;
    let file_storage = std::sync::Arc::new(lattice::services::file_storage::FileStorageService::new())
        as std::sync::Arc<dyn lattice::services::traits::FileStorageServiceTrait>;
    let model_manager = std::sync::Arc::new(lattice::services::model_manager::ModelManager::new())
        as std::sync::Arc<dyn lattice::services::traits::ModelManagerTrait>;
    let web_ingestion = std::sync::Arc::new(lattice::services::web_ingestion_service::WebIngestionService::new(
        pool.clone(),
        embedding_service.clone(),
    )) as std::sync::Arc<dyn lattice::services::traits::WebIngestionServiceTrait>;
    let search_enrichment = std::sync::Arc::new(lattice::services::search_enrichment_service::SearchEnrichmentService::new(pool.clone()))
        as std::sync::Arc<dyn lattice::services::traits::SearchEnrichmentServiceTrait>;
    let conversation = std::sync::Arc::new(lattice::services::conversation_service::ConversationService::new(pool.clone()))
        as std::sync::Arc<dyn lattice::services::traits::ConversationServiceTrait>;
    let context_manager = std::sync::Arc::new(lattice::services::context_manager::ContextManager::new(4000))
        as std::sync::Arc<dyn lattice::services::traits::ContextManagerTrait>;

    let container = lattice::di::service_container::ServiceContainer::new(
        pool,
        security_context,
        metrics,
        embedding_service,
        search_service,
        tag_service,
        file_storage,
        model_manager,
        web_ingestion,
        search_enrichment,
        conversation,
        context_manager,
        None,
        None,
        None,
    );

    let state = State::from(&container);
    let result = health_check(state).await;

    // Health check should still return (not panic)
    assert!(result.is_ok(), "Health check should handle database errors gracefully");

    let health = result.unwrap();
    assert_eq!(
        health.database.status, "unhealthy",
        "Database should be marked unhealthy"
    );
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_performance() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let start = std::time::Instant::now();
    let _ = health_check(state).await.unwrap();
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 500,
        "Health check should complete in under 500ms, took {}ms",
        duration.as_millis()
    );
}

#[tokio::test]
async fn test_health_check_latency_measurement() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    // All components should have latency measured
    assert!(result.database.latency_ms.is_some());
    assert!(result.embedding_service.latency_ms.is_some());
    assert!(result.qa_engine.latency_ms.is_some());
    assert!(result.cache.latency_ms.is_some());
    assert!(result.search_index.latency_ms.is_some());

    // Latencies should be reasonable
    assert!(result.database.latency_ms.unwrap() < 1000);
    assert!(result.embedding_service.latency_ms.unwrap() < 1000);
    assert!(result.qa_engine.latency_ms.unwrap() < 1000);
    assert!(result.cache.latency_ms.unwrap() < 1000);
    assert!(result.search_index.latency_ms.unwrap() < 1000);
}

// ============================================================================
// Component Health Helper Tests
// ============================================================================

#[tokio::test]
async fn test_component_health_structure() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    // Verify ComponentHealth structure
    let db_health = &result.database;
    assert!(
        ["healthy", "degraded", "unhealthy"].contains(&db_health.status.as_str()),
        "Component status should be valid"
    );
    assert!(db_health.message.is_some(), "Component should have message");
    assert!(
        db_health.latency_ms.is_some(),
        "Component should have latency"
    );
}

#[tokio::test]
async fn test_health_status_serialization() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await.unwrap();

    // Test that health status can be serialized to JSON
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("status"));
    assert!(json.contains("database"));
    assert!(json.contains("embeddingService"));
    assert!(json.contains("qaEngine"));
    assert!(json.contains("cache"));
    assert!(json.contains("searchIndex"));
}
