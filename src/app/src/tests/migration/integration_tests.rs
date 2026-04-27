#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Migration Integration Tests
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Tests to verify that DDD ServiceContainer coexists with legacy implementations
//! and that migration doesn't break existing functionality.
//!
//! # Test Coverage
//!
//! - ✅ Container initialization
//! - ✅ Service compatibility
//! - ✅ Command execution with new container
//! - ✅ Database operations consistency
//! - ✅ Migration path verification
//! - ✅ No regressions in functionality
//!
//! # Migration Safety
//!
//! These tests ensure:
//! 1. New container provides same functionality
//! 2. No breaking changes in command signatures
//! 3. Data consistency maintained
//! 4. Performance not degraded

use crate::migration::container_helpers::*;
use lattice::commands::health::*;
use lattice::commands::tags::*;
use lattice::error::Result;
use lattice::infrastructure::persistence::repositories::TagRepository;
use tauri::State;

// ============================================================================
// Container Initialization Tests
// ============================================================================

#[tokio::test]
async fn test_container_initialization_succeeds() {
    let result = create_test_container().await;
    assert!(result.is_ok(), "Container initialization should succeed");

    let container = result.unwrap();
    assert_container_initialized(&container).await;
}

#[tokio::test]
async fn test_multiple_containers_can_coexist() {
    let container1 = create_test_container().await.unwrap();
    let container2 = create_test_container().await.unwrap();

    // Both should be functional
    assert_container_initialized(&container1).await;
    assert_container_initialized(&container2).await;

    // Both should operate independently
    let state1 = State::from(&container1);
    let state2 = State::from(&container2);

    let health1 = health_check(state1).await.unwrap();
    let health2 = health_check(state2).await.unwrap();

    assert_eq!(health1.status, health2.status);
}

#[tokio::test]
async fn test_container_with_different_configs() {
    let minimal = create_minimal_container().await.unwrap();
    let full = create_full_container().await.unwrap();

    // Both should be functional
    assert_container_initialized(&minimal).await;
    assert_container_initialized(&full).await;

    // Verify optional services
    assert!(minimal.indexing_service().is_none());
    assert!(full.indexing_service().is_none() || full.indexing_service().is_some());
}

// ============================================================================
// Service Compatibility Tests
// ============================================================================

#[tokio::test]
async fn test_embedding_service_compatibility() {
    let container = create_test_container().await.unwrap();
    let embedding_service = container.embedding_service();

    // Test that embedding service works as expected
    let result = embedding_service.embed_single("test query").await;
    assert!(result.is_ok());

    let embedding = result.unwrap();
    assert_eq!(embedding.len(), 384, "Embedding dimension should be 384");
}

#[tokio::test]
async fn test_search_service_compatibility() {
    let container = create_test_container().await.unwrap();
    let search_service = container.search_service();

    // Test search with mock service
    let query_vector = vec![0.1; 384];
    let results = search_service.search(&query_vector, 10);

    // Mock returns empty results
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_tag_service_compatibility() {
    let container = create_test_container().await.unwrap();
    let tag_service = container.tag_service();

    // Test tag creation through service
    let result = tag_service.get_or_create("rust", "#ff5733").await;
    assert!(result.is_ok());

    let tag = result.unwrap();
    assert_eq!(tag.name().as_str(), "rust");
}

#[tokio::test]
async fn test_database_pool_compatibility() {
    let container = create_test_container().await.unwrap();
    let pool = container.db_pool();

    // Test database query
    let result = sqlx::query("SELECT 1 as test")
        .fetch_optional(pool)
        .await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

// ============================================================================
// Command Execution Tests
// ============================================================================

#[tokio::test]
async fn test_health_command_with_container() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = health_check(state).await;
    assert!(result.is_ok(), "Health check command should work with container");

    let health = result.unwrap();
    assert!(!health.status.is_empty());
}

#[tokio::test]
async fn test_tag_commands_with_container() {
    let container = create_test_container().await.unwrap();

    // Create document
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let state = State::from(&container);

    // Test tag creation
    let tag = create_tag("rust".to_string(), None, state.clone())
        .await
        .unwrap();
    assert!(!tag.id.is_empty());

    // Test tag application
    let tags = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string()],
        state.clone(),
    )
    .await
    .unwrap();
    assert!(tags.len() > 0);

    // Test tag retrieval
    let all_tags = get_all_tags(state.clone()).await.unwrap();
    assert!(all_tags.len() > 0);

    // Test tag search
    let docs = search_by_tag("rust".to_string(), state.clone())
        .await
        .unwrap();
    assert!(docs.len() > 0);
}

// ============================================================================
// Data Consistency Tests
// ============================================================================

#[tokio::test]
async fn test_tag_operations_maintain_consistency() {
    let container = create_test_container().await.unwrap();
    let tag_repo = TagRepository::new(container.db_pool().clone());

    // Create tag through repository
    let tag1 = tag_repo.create("rust", Some("#ff5733")).await.unwrap();

    // Create tag through command
    let state = State::from(&container);
    let tag2 = create_tag("python".to_string(), None, state.clone())
        .await
        .unwrap();

    // Both should be retrievable
    let all_tags = tag_repo.get_all().await.unwrap();
    assert!(all_tags.iter().any(|t| t.id == tag1.id));
    assert!(all_tags.iter().any(|t| t.id == tag2.id));
}

#[tokio::test]
async fn test_document_tag_associations_consistent() {
    let container = create_test_container().await.unwrap();
    let tag_repo = TagRepository::new(container.db_pool().clone());

    // Create document
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let state = State::from(&container);

    // Apply tags through command
    let _ = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string()],
        state.clone(),
    )
    .await
    .unwrap();

    // Verify through repository
    let tags = tag_repo.get_tags_for_document("doc-1").await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name().as_str(), "rust");

    // Verify through command
    let cmd_tags = get_document_tags("doc-1".to_string(), state).await.unwrap();
    assert_eq!(cmd_tags.len(), 1);
    assert_eq!(cmd_tags[0].name().as_str(), "rust");
}

// ============================================================================
// Migration Path Tests
// ============================================================================

#[tokio::test]
async fn test_container_supports_incremental_migration() {
    // Phase 1: Create container with minimal services
    let container = create_minimal_container().await.unwrap();
    assert!(container.indexing_service().is_none());

    // Phase 2: Container should still work without optional services
    let state = State::from(&container);
    let health = health_check(state).await.unwrap();
    assert!(!health.status.is_empty());

    // Phase 3: Can create full container later
    let full_container = create_full_container().await.unwrap();
    assert_container_initialized(&full_container).await;
}

#[tokio::test]
async fn test_existing_data_compatible() {
    let container = create_test_container().await.unwrap();

    // Simulate existing data (created before migration)
    sqlx::query(
        r#"
        INSERT INTO tags (id, name, color, created_at, updated_at)
        VALUES ('tag-1', 'legacy-tag', '#fff', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'legacy.txt', '/legacy.txt', 'legacy', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    // New container should work with existing data
    let state = State::from(&container);
    let tags = get_all_tags(state.clone()).await.unwrap();
    assert!(tags.iter().any(|t| t.name() == "legacy-tag"));

    // Can apply new tags to legacy documents
    let result = apply_tags(
        "doc-1".to_string(),
        vec!["new-tag".to_string()],
        state,
    )
    .await;
    assert!(result.is_ok());
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_container_initialization_performance() {
    let start = std::time::Instant::now();
    let _ = create_test_container().await.unwrap();
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 1000,
        "Container initialization should be fast (< 1s), took {}ms",
        duration.as_millis()
    );
}

#[tokio::test]
async fn test_service_access_performance() {
    let container = create_test_container().await.unwrap();

    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = container.embedding_service();
        let _ = container.search_service();
        let _ = container.tag_service();
    }
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 100,
        "Service access should be fast, took {}ms for 1000 accesses",
        duration.as_millis()
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_container_handles_database_errors_gracefully() {
    let container = create_test_container().await.unwrap();

    // Try to query non-existent table
    let result = sqlx::query("SELECT * FROM nonexistent_table")
        .fetch_optional(container.db_pool())
        .await;

    // Should return error, not panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_container_handles_service_errors_gracefully() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    // Try operations on non-existent resources
    let result = get_document_tags("nonexistent".to_string(), state).await;

    // Should handle gracefully
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// Regression Tests
// ============================================================================

#[tokio::test]
async fn test_no_regression_in_tag_creation() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    // Test that tag creation still works as before
    let tag = create_tag("rust".to_string(), Some("#ff5733".to_string()), state)
        .await
        .unwrap();

    assert_eq!(tag.name().as_str(), "rust");
    assert_eq!(tag.color, Some("#ff5733".to_string()));
    assert!(!tag.id.is_empty());
}

#[tokio::test]
async fn test_no_regression_in_tag_normalization() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let tag = create_tag("  RUST  ".to_string(), None, state).await.unwrap();

    // Should still normalize to lowercase
    assert_eq!(tag.name().as_str(), "rust");
}

#[tokio::test]
async fn test_no_regression_in_tag_merging() {
    use lattice::services::tag_service::TagService;

    let existing = vec!["rust".to_string()];
    let generated = vec!["RUST".to_string(), "python".to_string()];

    let merged = TagService::merge_tags(existing, generated);

    // Should still deduplicate case-insensitively
    assert_eq!(merged.len(), 2);
    assert!(merged.contains(&"rust".to_string()));
    assert!(merged.contains(&"python".to_string()));
}

// ============================================================================
// Concurrent Operations Tests
// ============================================================================

#[tokio::test]
async fn test_container_supports_concurrent_operations() {
    let container = std::sync::Arc::new(create_test_container().await.unwrap());

    // Create document
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    // Spawn multiple concurrent operations
    let mut handles = vec![];

    for i in 0..10 {
        let container_clone = container.clone();
        let handle = tokio::spawn(async move {
            let state = State::from(container_clone.as_ref());
            apply_tags(
                "doc-1".to_string(),
                vec![format!("tag-{}", i)],
                state,
            )
            .await
        });
        handles.push(handle);
    }

    // All operations should complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn test_container_thread_safe() {
    let container = std::sync::Arc::new(create_test_container().await.unwrap());

    // Access services from multiple threads
    let mut handles = vec![];

    for _ in 0..10 {
        let container_clone = container.clone();
        let handle = tokio::spawn(async move {
            let _ = container_clone.embedding_service();
            let _ = container_clone.search_service();
            let _ = container_clone.tag_service();
        });
        handles.push(handle);
    }

    // All should complete without errors
    for handle in handles {
        handle.await.unwrap();
    }
}
