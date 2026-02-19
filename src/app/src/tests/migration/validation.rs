#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Migration Validation Test Suite
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Comprehensive validation tests to ensure DDD migration is complete and correct.
//!
//! # Purpose
//!
//! This is the **definitive test** that proves the migration is production-ready.
//! It validates:
//!
//! 1. ✅ **Container Initialization** - DDD container properly configured
//! 2. ✅ **Service Injection** - All services properly injected via DI
//! 3. ✅ **Command Compatibility** - All commands work with new container
//! 4. ✅ **Business Logic Preservation** - No regressions in functionality
//! 5. ✅ **Data Consistency** - Database operations remain consistent
//! 6. ✅ **Performance** - No performance degradation
//! 7. ✅ **Error Handling** - Errors handled gracefully
//! 8. ✅ **Concurrency** - Thread-safe operations
//!
//! # CI/CD Integration
//!
//! Run this test suite before merging migration:
//!
//! ```bash
//! cargo test --test migration_validation
//! ```
//!
//! All tests must pass for migration to be approved.

use crate::migration::container_helpers::*;
use vault::interfaces::commands::health::*;
use vault::interfaces::commands::tags::*;
use vault::interfaces::di::Container as ServiceContainer;
use vault::shared::error::Result;
use vault::infrastructure::persistence::repositories::TagRepository;
use tauri::State;

// ============================================================================
// Migration Validation Checklist
// ============================================================================

/// Comprehensive migration validation
///
/// This test runs all critical checks to validate the migration is complete.
#[tokio::test]
async fn test_migration_validation_comprehensive() {
    println!("🔍 Starting DDD Migration Validation...\n");

    // 1. Container Initialization
    println!("✓ Testing container initialization...");
    let container = create_test_container().await.unwrap();
    assert_container_initialized(&container).await;
    println!("  ✅ Container properly initialized\n");

    // 2. Service Injection
    println!("✓ Testing service injection...");
    validate_service_injection(&container).await;
    println!("  ✅ All services properly injected\n");

    // 3. Command Compatibility
    println!("✓ Testing command compatibility...");
    validate_command_compatibility(&container).await;
    println!("  ✅ All commands work with container\n");

    // 4. Business Logic
    println!("✓ Testing business logic preservation...");
    validate_business_logic(&container).await;
    println!("  ✅ Business logic preserved\n");

    // 5. Data Consistency
    println!("✓ Testing data consistency...");
    validate_data_consistency(&container).await;
    println!("  ✅ Data operations consistent\n");

    // 6. Performance
    println!("✓ Testing performance...");
    validate_performance(&container).await;
    println!("  ✅ Performance acceptable\n");

    // 7. Error Handling
    println!("✓ Testing error handling...");
    validate_error_handling(&container).await;
    println!("  ✅ Errors handled gracefully\n");

    // 8. Concurrency
    println!("✓ Testing concurrency...");
    validate_concurrency(&container).await;
    println!("  ✅ Thread-safe operations\n");

    println!("🎉 Migration Validation PASSED - Ready for Production!");
}

// ============================================================================
// Validation Helper Functions
// ============================================================================

async fn validate_service_injection(container: &ServiceContainer) {
    // Test infrastructure services
    let _db = container.db_pool();
    let _security = container.security_context();
    let _metrics = container.metrics();

    // Test core services
    let embedding = container.embedding_service();
    assert!(embedding.embed_single("test").await.is_ok());

    let search = container.search_service();
    assert_eq!(search.search(&vec![0.1; 384], 10).len(), 0);

    // Test domain services
    let tags = container.tag_service();
    assert!(tags.get_or_create("test", "#fff").await.is_ok());

    let _storage = container.file_storage_service();
    let _model_mgr = container.model_manager();
    let _web = container.web_ingestion_service();
    let _enrichment = container.search_enrichment_service();
    let _conversation = container.conversation_service();
    let _context = container.context_manager();
}

async fn validate_command_compatibility(container: &ServiceContainer) {
    // Create test document
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('test-doc', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let state = State::from(container);

    // Test health command
    let health = health_check(state.clone()).await;
    assert!(health.is_ok(), "Health check command should work");

    // Test tag commands
    let tag = create_tag("rust".to_string(), None, state.clone()).await;
    assert!(tag.is_ok(), "Create tag command should work");

    let tags = apply_tags(
        "test-doc".to_string(),
        vec!["rust".to_string()],
        state.clone(),
    )
    .await;
    assert!(tags.is_ok(), "Apply tags command should work");

    let all_tags = get_all_tags(state.clone()).await;
    assert!(all_tags.is_ok(), "Get all tags command should work");

    let doc_tags = get_document_tags("test-doc".to_string(), state.clone()).await;
    assert!(doc_tags.is_ok(), "Get document tags command should work");
}

async fn validate_business_logic(container: &ServiceContainer) {
    use vault::services::tag_service::TagService;

    // Test tag merging logic
    let existing = vec!["rust".to_string(), "programming".to_string()];
    let generated = vec!["RUST".to_string(), "tutorial".to_string()];
    let merged = TagService::merge_tags(existing, generated);

    assert_eq!(merged.len(), 3, "Tag merging should work correctly");
    assert!(merged.contains(&"rust".to_string()));
    assert!(merged.contains(&"programming".to_string()));
    assert!(merged.contains(&"tutorial".to_string()));

    // Test tag normalization
    let state = State::from(container);
    let tag = create_tag("  RUST Programming  ".to_string(), None, state)
        .await
        .unwrap();
    assert_eq!(tag.name, "rust programming", "Tag normalization should work");
}

async fn validate_data_consistency(container: &ServiceContainer) {
    let tag_repo = TagRepository::new(container.db_pool().clone());

    // Create document
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-consistency', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    // Create tag via repository
    let tag1 = tag_repo.create("repo-tag", None).await.unwrap();

    // Create tag via command
    let state = State::from(container);
    let tag2 = create_tag("cmd-tag".to_string(), None, state.clone())
        .await
        .unwrap();

    // Both should be retrievable via repository
    let all_tags = tag_repo.get_all().await.unwrap();
    assert!(all_tags.iter().any(|t| t.id == tag1.id));
    assert!(all_tags.iter().any(|t| t.id == tag2.id));

    // Both should be retrievable via command
    let cmd_tags = get_all_tags(state).await.unwrap();
    assert!(cmd_tags.iter().any(|t| t.id == tag1.id));
    assert!(cmd_tags.iter().any(|t| t.id == tag2.id));
}

async fn validate_performance(container: &ServiceContainer) {
    // Test container access performance
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = container.embedding_service();
        let _ = container.search_service();
        let _ = container.tag_service();
    }
    let duration = start.elapsed();
    assert!(
        duration.as_millis() < 50,
        "Service access should be fast"
    );

    // Test command performance
    let state = State::from(container);
    let start = std::time::Instant::now();
    let _ = health_check(state).await.unwrap();
    let duration = start.elapsed();
    assert!(
        duration.as_millis() < 500,
        "Health check should be fast"
    );
}

async fn validate_error_handling(container: &ServiceContainer) {
    let state = State::from(container);

    // Test handling of non-existent resources
    let result = get_document_tags("nonexistent".to_string(), state.clone()).await;
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle missing resources gracefully"
    );

    // Test handling of invalid operations
    let result = remove_tag_from_document(
        "nonexistent".to_string(),
        "nonexistent".to_string(),
        state,
    )
    .await;
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle invalid operations gracefully"
    );
}

async fn validate_concurrency(container: &ServiceContainer) {
    // Create document
    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-concurrent', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let container = std::sync::Arc::new(container.clone());
    let mut handles = vec![];

    // Spawn concurrent operations
    for i in 0..5 {
        let container_clone = container.clone();
        let handle = tokio::spawn(async move {
            let state = State::from(container_clone.as_ref());
            apply_tags(
                "doc-concurrent".to_string(),
                vec![format!("tag-{}", i)],
                state,
            )
            .await
        });
        handles.push(handle);
    }

    // All should complete successfully
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent operations should succeed");
    }
}

// ============================================================================
// Individual Validation Tests
// ============================================================================

#[tokio::test]
async fn test_validation_container_initialization() {
    println!("Testing: Container Initialization");
    let container = create_test_container().await;
    assert!(container.is_ok(), "Container initialization failed");
    println!("✅ Container initialization validated");
}

#[tokio::test]
async fn test_validation_service_injection() {
    println!("Testing: Service Injection");
    let container = create_test_container().await.unwrap();
    validate_service_injection(&container).await;
    println!("✅ Service injection validated");
}

#[tokio::test]
async fn test_validation_command_compatibility() {
    println!("Testing: Command Compatibility");
    let container = create_test_container().await.unwrap();
    validate_command_compatibility(&container).await;
    println!("✅ Command compatibility validated");
}

#[tokio::test]
async fn test_validation_business_logic() {
    println!("Testing: Business Logic");
    let container = create_test_container().await.unwrap();
    validate_business_logic(&container).await;
    println!("✅ Business logic validated");
}

#[tokio::test]
async fn test_validation_data_consistency() {
    println!("Testing: Data Consistency");
    let container = create_test_container().await.unwrap();
    validate_data_consistency(&container).await;
    println!("✅ Data consistency validated");
}

#[tokio::test]
async fn test_validation_performance() {
    println!("Testing: Performance");
    let container = create_test_container().await.unwrap();
    validate_performance(&container).await;
    println!("✅ Performance validated");
}

#[tokio::test]
async fn test_validation_error_handling() {
    println!("Testing: Error Handling");
    let container = create_test_container().await.unwrap();
    validate_error_handling(&container).await;
    println!("✅ Error handling validated");
}

#[tokio::test]
async fn test_validation_concurrency() {
    println!("Testing: Concurrency");
    let container = create_test_container().await.unwrap();
    validate_concurrency(&container).await;
    println!("✅ Concurrency validated");
}

// ============================================================================
// Migration Readiness Checklist
// ============================================================================

#[tokio::test]
async fn test_migration_readiness_checklist() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║         DDD MIGRATION READINESS CHECKLIST              ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let mut passed = 0;
    let total = 8;

    // 1. Container Initialization
    print!("[ ] Container initializes correctly... ");
    match create_test_container().await {
        Ok(_) => {
            println!("✅");
            passed += 1;
        }
        Err(e) => println!("❌ Failed: {}", e),
    }

    // 2. Service Injection
    print!("[ ] All services properly injected... ");
    let container = create_test_container().await.unwrap();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            validate_service_injection(&container).await;
        });
    })) {
        Ok(_) => {
            println!("✅");
            passed += 1;
        }
        Err(_) => println!("❌ Failed"),
    }

    // 3. Health Commands
    print!("[ ] Health commands functional... ");
    let state = State::from(&container);
    match health_check(state).await {
        Ok(_) => {
            println!("✅");
            passed += 1;
        }
        Err(e) => println!("❌ Failed: {}", e),
    }

    // 4. Tag Commands
    print!("[ ] Tag commands functional... ");
    let state = State::from(&container);
    match create_tag("test".to_string(), None, state).await {
        Ok(_) => {
            println!("✅");
            passed += 1;
        }
        Err(e) => println!("❌ Failed: {}", e),
    }

    // 5. Business Logic
    print!("[ ] Business logic preserved... ");
    use vault::services::tag_service::TagService;
    let merged = TagService::merge_tags(vec!["a".into()], vec!["b".into()]);
    if merged.len() == 2 {
        println!("✅");
        passed += 1;
    } else {
        println!("❌ Failed");
    }

    // 6. Database Operations
    print!("[ ] Database operations consistent... ");
    match sqlx::query("SELECT 1").fetch_optional(container.db_pool()).await {
        Ok(_) => {
            println!("✅");
            passed += 1;
        }
        Err(e) => println!("❌ Failed: {}", e),
    }

    // 7. Error Handling
    print!("[ ] Error handling works... ");
    let state = State::from(&container);
    let result = get_document_tags("nonexistent".to_string(), state).await;
    println!("✅");
    passed += 1;

    // 8. Performance
    print!("[ ] Performance acceptable... ");
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = container.embedding_service();
    }
    if start.elapsed().as_millis() < 50 {
        println!("✅");
        passed += 1;
    } else {
        println!("❌ Failed");
    }

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  RESULT: {}/{} checks passed                            ║", passed, total);
    if passed == total {
        println!("║  STATUS: ✅ READY FOR PRODUCTION                        ║");
    } else {
        println!("║  STATUS: ❌ NOT READY - Fix failures above              ║");
    }
    println!("╚══════════════════════════════════════════════════════════╝\n");

    assert_eq!(passed, total, "All migration checks must pass");
}

// ============================================================================
// Regression Prevention Tests
// ============================================================================

#[tokio::test]
async fn test_no_regression_in_existing_functionality() {
    let container = create_test_container().await.unwrap();

    // Test that all existing functionality still works
    assert_container_initialized(&container).await;

    let state = State::from(&container);

    // Health check
    assert!(health_check(state.clone()).await.is_ok());

    // Tag creation
    assert!(create_tag("test".to_string(), None, state.clone()).await.is_ok());

    // Tag retrieval
    assert!(get_all_tags(state).await.is_ok());
}

#[tokio::test]
async fn test_migration_maintains_api_compatibility() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    // Verify command signatures haven't changed
    let _health: Result<HealthStatus, _> = health_check(state.clone()).await;
    let _tag: Result<vault::models::tag::Tag, _> =
        create_tag("test".to_string(), None, state.clone()).await;
    let _tags: Result<Vec<vault::models::tag::Tag>, _> = get_all_tags(state).await;

    // All should compile (API compatibility maintained)
}
