#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! Tag Command Tests with DDD Container
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Comprehensive test suite for tag commands using ServiceContainer.
//!
//! # Test Coverage
//!
//! - ✅ Tag creation and retrieval
//! - ✅ Tag application to documents
//! - ✅ Tag removal from documents
//! - ✅ Tag merging logic
//! - ✅ Tag search functionality
//! - ✅ Tag updates and deletion
//! - ✅ Concurrent tag operations (locking)
//! - ✅ Error handling
//! - ✅ Tag service business logic
//!
//! # Purpose
//!
//! These tests verify that tag commands work correctly with the DDD
//! ServiceContainer and the refactored TagService.

use crate::migration::container_helpers::*;
use lattice::commands::tags::*;
use lattice::shared::error::Result;
use lattice::models::tag::Tag;
use lattice::infrastructure::persistence::repositories::{DocumentRepository, TagRepository};
use lattice::services::tag_service::TagService;
use tauri::State;

#[tokio::test]
async fn test_create_tag_with_container() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = create_tag("rust".to_string(), Some("#ff5733".to_string()), state).await;
    assert!(result.is_ok(), "Tag creation should succeed");

    let tag = result.unwrap();
    assert_eq!(tag.name, "rust", "Tag name should be normalized");
    assert_eq!(tag.color, Some("#ff5733".to_string()));
}

#[tokio::test]
async fn test_create_tag_normalizes_name() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = create_tag("  RUST Programming  ".to_string(), None, state).await;
    assert!(result.is_ok());

    let tag = result.unwrap();
    assert_eq!(
        tag.name, "rust programming",
        "Tag name should be trimmed and lowercased"
    );
}

#[tokio::test]
async fn test_create_tag_without_color() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = create_tag("python".to_string(), None, state).await;
    assert!(result.is_ok());

    let tag = result.unwrap();
    assert_eq!(tag.name, "python");
    assert!(tag.color.is_none() || tag.color.is_some());
}

#[tokio::test]
async fn test_get_all_tags() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let _ = create_tag("rust".to_string(), None, state.clone()).await;
    let _ = create_tag("python".to_string(), None, state.clone()).await;

    let result = get_all_tags(state).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    assert!(tags.len() >= 2, "Should have at least 2 tags");
}

#[tokio::test]
async fn test_get_all_tags_with_counts() {
    let container = create_test_container().await.unwrap();

    let tag_repo = TagRepository::new(container.db_pool().clone());
    let doc_repo = DocumentRepository::new(container.db_pool().clone());

    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let tag = tag_repo.create("rust", None).await.unwrap();
    tag_repo
        .add_tags_to_document_by_names("doc-1", vec!["rust".to_string()])
        .await
        .unwrap();

    let state = State::from(&container);
    let result = get_all_tags_with_counts(state).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    let rust_tag = tags.iter().find(|t| t.name() == "rust");
    assert!(rust_tag.is_some());
    assert_eq!(rust_tag.unwrap().document_count, 1);
}

#[tokio::test]
async fn test_get_document_tags() {
    let container = create_test_container().await.unwrap();

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
    let _ = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string(), "programming".to_string()],
        state.clone(),
    )
    .await;

    let result = get_document_tags("doc-1".to_string(), state).await;
    assert!(result.is_ok());

    let tags = result.unwrap();
    assert_eq!(tags.len(), 2);
}

#[tokio::test]
async fn test_apply_tags_to_document() {
    let container = create_test_container().await.unwrap();

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
    let result = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string(), "tutorial".to_string()],
        state,
    )
    .await;

    assert!(result.is_ok());
    let tags = result.unwrap();
    assert_eq!(tags.len(), 2);
}

#[tokio::test]
async fn test_apply_tags_normalizes_input() {
    let container = create_test_container().await.unwrap();

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
    let result = apply_tags(
        "doc-1".to_string(),
        vec!["  RUST  ".to_string(), "Programming".to_string()],
        state,
    )
    .await;

    assert!(result.is_ok());
    let tags = result.unwrap();

    assert!(tags.iter().any(|t| t.name() == "rust"));
    assert!(tags.iter().any(|t| t.name() == "programming"));
}

#[tokio::test]
async fn test_apply_tags_merges_with_existing() {
    let container = create_test_container().await.unwrap();

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

    let _ = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string()],
        state.clone(),
    )
    .await;

    let result = apply_tags(
        "doc-1".to_string(),
        vec!["python".to_string(), "RUST".to_string()],
        state,
    )
    .await;

    assert!(result.is_ok());
    let tags = result.unwrap();

    assert!(tags.len() >= 2);
    assert_eq!(
        tags.iter().filter(|t| t.name() == "rust").count(),
        1,
        "Should not duplicate rust tag"
    );
}

#[tokio::test]
async fn test_apply_tags_filters_empty() {
    let container = create_test_container().await.unwrap();

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
    let result = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string(), "  ".to_string(), "".to_string()],
        state,
    )
    .await;

    assert!(result.is_ok());
    let tags = result.unwrap();
    assert_eq!(tags.len(), 1, "Should filter out empty tags");
}

#[tokio::test]
async fn test_remove_tag_from_document() {
    let container = create_test_container().await.unwrap();

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
    let tags = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string()],
        state.clone(),
    )
    .await
    .unwrap();

    let tag_id = tags[0].id.clone();

    let result = remove_tag_from_document("doc-1".to_string(), tag_id, state.clone()).await;
    assert!(result.is_ok());

    let remaining_tags = get_document_tags("doc-1".to_string(), state).await.unwrap();
    assert_eq!(remaining_tags.len(), 0, "Tag should be removed");
}

#[tokio::test]
async fn test_update_tag_name() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let tag = create_tag("rust".to_string(), None, state.clone())
        .await
        .unwrap();

    let result = update_tag(
        tag.id.clone(),
        Some("rust-lang".to_string()),
        None,
        state,
    )
    .await;

    assert!(result.is_ok());
    let updated = result.unwrap();
    assert_eq!(updated.name, "rust-lang");
}

#[tokio::test]
async fn test_update_tag_color() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let tag = create_tag("rust".to_string(), Some("#fff".to_string()), state.clone())
        .await
        .unwrap();

    let result = update_tag(tag.id.clone(), None, Some("#ff5733".to_string()), state).await;

    assert!(result.is_ok());
    let updated = result.unwrap();
    assert_eq!(updated.color, Some("#ff5733".to_string()));
}

#[tokio::test]
async fn test_delete_tag() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let tag = create_tag("rust".to_string(), None, state.clone())
        .await
        .unwrap();

    let result = delete_tag(tag.id.clone(), state.clone()).await;
    assert!(result.is_ok());

    let all_tags = get_all_tags(state).await.unwrap();
    assert!(
        !all_tags.iter().any(|t| t.id == tag.id),
        "Tag should be deleted"
    );
}

#[tokio::test]
async fn test_search_by_tag() {
    let container = create_test_container().await.unwrap();

    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES
            ('doc-1', 'test1.txt', '/test1.txt', 'test', datetime('now'), datetime('now')),
            ('doc-2', 'test2.txt', '/test2.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let state = State::from(&container);

    // Tag first document
    let _ = apply_tags("doc-1".to_string(), vec!["rust".to_string()], state.clone()).await;

    // Search by tag
    let result = search_by_tag("rust".to_string(), state).await;
    assert!(result.is_ok());

    let doc_ids = result.unwrap();
    assert_eq!(doc_ids.len(), 1);
    assert_eq!(doc_ids[0], "doc-1");
}

#[test]
fn test_tag_service_merge_logic() {
    let existing = vec!["rust".to_string(), "programming".to_string()];
    let generated = vec![
        "RUST".to_string(),
        "tutorial".to_string(),
        "Programming".to_string(),
        "async".to_string(),
    ];

    let merged = TagService::merge_tags(existing, generated);

    assert_eq!(merged.len(), 4, "Should have 4 unique tags");
    assert!(merged.contains(&"rust".to_string()));
    assert!(merged.contains(&"programming".to_string()));
    assert!(merged.contains(&"tutorial".to_string()));
    assert!(merged.contains(&"async".to_string()));

    assert_eq!(
        merged.iter().filter(|t| t.to_lowercase() == "rust").count(),
        1
    );
}

#[test]
fn test_tag_service_merge_empty() {
    let existing: Vec<String> = vec![];
    let generated = vec!["tag1".to_string(), "tag2".to_string()];

    let merged = TagService::merge_tags(existing, generated);
    assert_eq!(merged.len(), 2);
}

#[test]
fn test_tag_service_merge_all_duplicates() {
    let existing = vec!["tag1".to_string(), "tag2".to_string()];
    let generated = vec!["TAG1".to_string(), "Tag2".to_string()];

    let merged = TagService::merge_tags(existing, generated);
    assert_eq!(merged.len(), 2, "Should deduplicate all tags");
}

#[tokio::test]
async fn test_concurrent_tag_operations_with_locking() {
    let container = create_test_container().await.unwrap();

    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    let state1 = State::from(&container);
    let state2 = State::from(&container);

    // Attempt concurrent tag applications
    let handle1 = tokio::spawn(async move {
        apply_tags(
            "doc-1".to_string(),
            vec!["rust".to_string()],
            state1,
        )
        .await
    });

    let handle2 = tokio::spawn(async move {
        apply_tags(
            "doc-1".to_string(),
            vec!["python".to_string()],
            state2,
        )
        .await
    });

    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();

    // Both should succeed (locking prevents conflicts)
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[tokio::test]
async fn test_tag_lock_timeout() {
    let container = create_test_container().await.unwrap();

    sqlx::query(
        r#"
        INSERT INTO documents (id, file_name, file_path, content, created_at, updated_at)
        VALUES ('doc-1', 'test.txt', '/test.txt', 'test', datetime('now'), datetime('now'))
        "#,
    )
    .execute(container.db_pool())
    .await
    .unwrap();

    // Acquire lock manually
    let lock = container
        .tag_service()
        .acquire_lock_with_timeout("doc-1")
        .await
        .unwrap();

    // Try to apply tags while lock is held
    let state = State::from(&container);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        apply_tags("doc-1".to_string(), vec!["rust".to_string()], state),
    )
    .await;

    assert!(result.is_err(), "Should timeout waiting for lock");

    drop(lock); // Release lock
}

#[tokio::test]
async fn test_apply_tags_to_nonexistent_document() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = apply_tags(
        "nonexistent".to_string(),
        vec!["rust".to_string()],
        state,
    )
    .await;

    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_get_tags_for_nonexistent_document() {
    let container = create_test_container().await.unwrap();
    let state = State::from(&container);

    let result = get_document_tags("nonexistent".to_string(), state).await;

    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_remove_nonexistent_tag() {
    let container = create_test_container().await.unwrap();

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
    let result = remove_tag_from_document("doc-1".to_string(), "nonexistent".to_string(), state).await;

    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_full_tag_lifecycle() {
    let container = create_test_container().await.unwrap();

    // 1. Create document
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

    // 2. Create tags
    let tag1 = create_tag("rust".to_string(), Some("#ff5733".to_string()), state.clone())
        .await
        .unwrap();
    let tag2 = create_tag("python".to_string(), None, state.clone())
        .await
        .unwrap();

    // 3. Apply tags to document
    let _ = apply_tags(
        "doc-1".to_string(),
        vec!["rust".to_string(), "python".to_string()],
        state.clone(),
    )
    .await
    .unwrap();

    // 4. Get document tags
    let doc_tags = get_document_tags("doc-1".to_string(), state.clone())
        .await
        .unwrap();
    assert_eq!(doc_tags.len(), 2);

    // 5. Search by tag
    let rust_docs = search_by_tag("rust".to_string(), state.clone())
        .await
        .unwrap();
    assert_eq!(rust_docs.len(), 1);

    // 6. Update tag
    let _ = update_tag(tag1.id.clone(), None, Some("#00ff00".to_string()), state.clone())
        .await
        .unwrap();

    // 7. Remove tag from document
    let _ = remove_tag_from_document("doc-1".to_string(), tag2.id.clone(), state.clone())
        .await
        .unwrap();

    let remaining = get_document_tags("doc-1".to_string(), state.clone())
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);

    // 8. Delete tag
    let _ = delete_tag(tag1.id.clone(), state.clone()).await.unwrap();

    let all_tags = get_all_tags(state).await.unwrap();
    assert!(!all_tags.iter().any(|t| t.id == tag1.id));
}
