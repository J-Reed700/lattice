//! Model Plugin Smoke Tests
//!
//! These are integration smoke tests that verify each model management command can run
//! without panicking. They test the HAPPY PATH and ONE ERROR PATH per command.
//!
//! Each command runs at least once without panicking.
//! Pattern: command_impl(container, payload).await.is_ok()
//!
//! # Test Strategy
//!
//! - Use real Container with in-memory database (production parity)
//! - Test key model management operations (list, set active, delete)
//! - Minimal payloads (defaults where possible)
//! - Assert commands return Result, not specific values

use crate::features::model_management::commands_extra::{
    delete_downloaded_model_and_file_impl, get_active_chat_model_impl,
    get_active_embedding_model_impl, get_models_with_metadata_impl, set_active_chat_model_impl,
    set_active_embedding_model_impl,
};
use crate::tests::common::setup_test_container;

/// Test that get_models_with_metadata doesn't panic
///
/// Happy Path: List all downloaded models (empty database is valid)
/// Expected: Ok with empty list or models if any exist
#[tokio::test]
async fn smoke_test_get_models_with_metadata_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = get_models_with_metadata_impl(&container).await;

    match result {
        Ok(json_str) => {
            println!("✅ get_models_with_metadata_impl returned JSON");
            let parse_result: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&json_str);
            assert!(parse_result.is_ok(), "Should return valid JSON array");
        }
        Err(e) => {
            println!(
                "✅ get_models_with_metadata_impl returned graceful error: {:?}",
                e
            );
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that set_active_chat_model handles nonexistent model gracefully
///
/// Error Path: Set active model that doesn't exist
/// Expected: Graceful Err (model not found)
#[tokio::test]
async fn smoke_test_set_active_chat_model_nonexistent() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    // Try to set a model that doesn't exist
    let result = set_active_chat_model_impl(&container, "nonexistent-model-id").await;

    match result {
        Ok(_) => {
            // If it succeeds, that's surprising but acceptable (maybe creates entry)
            println!("✅ set_active_chat_model_impl succeeded for nonexistent model");
        }
        Err(e) => {
            println!(
                "✅ set_active_chat_model_impl rejected nonexistent model gracefully: {:?}",
                e
            );
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that get_active_chat_model doesn't panic when no model is set
///
/// Happy Path: Get active chat model when none is set
/// Expected: Ok (None) or graceful Err
#[tokio::test]
async fn smoke_test_get_active_chat_model_none_set() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = get_active_chat_model_impl(&container).await;

    match result {
        Ok(json_str) => {
            println!("✅ get_active_chat_model_impl returned: {}", json_str);
            // Empty JSON or null is acceptable
        }
        Err(e) => {
            println!(
                "✅ get_active_chat_model_impl returned graceful error: {:?}",
                e
            );
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that set_active_embedding_model handles nonexistent model gracefully
///
/// Error Path: Set embedding model that doesn't exist
/// Expected: Graceful Err (model not found)
#[tokio::test]
async fn smoke_test_set_active_embedding_model_nonexistent() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = set_active_embedding_model_impl(&container, "nonexistent-embedding-model").await;

    match result {
        Ok(_) => {
            println!("✅ set_active_embedding_model_impl succeeded for nonexistent model");
        }
        Err(e) => {
            println!(
                "✅ set_active_embedding_model_impl rejected nonexistent model gracefully: {:?}",
                e
            );
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that get_active_embedding_model doesn't panic when no model is set
///
/// Happy Path: Get active embedding model when none is set
/// Expected: Ok (None) or graceful Err
#[tokio::test]
async fn smoke_test_get_active_embedding_model_none_set() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = get_active_embedding_model_impl(&container).await;

    match result {
        Ok(json_str) => {
            println!("✅ get_active_embedding_model_impl returned: {}", json_str);
        }
        Err(e) => {
            println!(
                "✅ get_active_embedding_model_impl returned graceful error: {:?}",
                e
            );
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that delete_downloaded_model handles nonexistent model gracefully
///
/// Error Path: Delete model that doesn't exist
/// Expected: Graceful Err (model not found)
#[tokio::test]
async fn smoke_test_delete_model_nonexistent() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result =
        delete_downloaded_model_and_file_impl(&container, "nonexistent-model-to-delete", false)
            .await;

    match result {
        Ok(_) => {
            println!("✅ delete_downloaded_model_and_file_impl handled nonexistent model");
        }
        Err(e) => {
            println!("✅ delete_downloaded_model_and_file_impl rejected nonexistent model gracefully: {:?}", e);
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that delete_downloaded_model with delete_file=true doesn't panic
///
/// Error Path: Delete with file deletion for nonexistent model
/// Expected: Graceful handling (no filesystem panic)
#[tokio::test]
async fn smoke_test_delete_model_with_file_deletion() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = delete_downloaded_model_and_file_impl(
        &container,
        "model-with-file-deletion",
        true, // Request file deletion
    )
    .await;

    match result {
        Ok(_) => {
            println!("✅ delete_downloaded_model_and_file_impl with file deletion succeeded");
        }
        Err(e) => {
            println!("✅ delete_downloaded_model_and_file_impl with file deletion handled error gracefully: {:?}", e);
            assert!(
                !e.contains("panic") && !e.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that deleting a model also cleans up download session rows for that model
///
/// Ensures we do not keep orphaned download metadata after model deletion.
#[tokio::test]
async fn delete_model_cleans_up_download_sessions_for_same_model() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let model_record_id = "model-record-cleanup-test";
    let model_id = "cleanup-test-embed-model";

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO models (
            id, model_name, model_id, base_path, storage_kind, storage_path,
            total_size_bytes, status,
            model_type, architecture, downloaded_at, use_count,
            is_active_for_chat, is_active_for_embedding
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(model_record_id)
    .bind("Cleanup Test Embedding Model")
    .bind(model_id)
    .bind("/tmp/models/cleanup-test-embed-model")
    .bind("local_file")
    .bind("/tmp/models/cleanup-test-embed-model/model.onnx")
    .bind(1024_i64)
    .bind("completed")
    .bind("embedding")
    .bind("bert")
    .bind(&now)
    .bind(0_i64)
    .bind(0_i64)
    .bind(0_i64)
    .execute(container.db_pool())
    .await
    .expect("Should insert model row");

    sqlx::query(
        r#"
        INSERT INTO download_sessions (
            id, url, destination, state,
            bytes_downloaded, total_bytes, bytes_per_second,
            retry_count, max_retries, created_at, updated_at,
            model_name, model_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("session-cleanup-test")
    .bind("https://example.com/model.onnx")
    .bind("/tmp/models/cleanup-test-embed-model/model.onnx")
    .bind("completed")
    .bind(1024_i64)
    .bind(1024_i64)
    .bind(100.0_f64)
    .bind(0_i64)
    .bind(3_i64)
    .bind(&now)
    .bind(&now)
    .bind("Cleanup Test Embedding Model")
    .bind(model_id)
    .execute(container.db_pool())
    .await
    .expect("Should insert download session");

    let before_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM download_sessions WHERE model_id = ?")
            .bind(model_id)
            .fetch_one(container.db_pool())
            .await
            .expect("Count query should work");
    assert_eq!(before_count, 1);

    delete_downloaded_model_and_file_impl(&container, model_record_id, false)
        .await
        .expect("Delete should succeed");

    let deleted_model: Option<String> = sqlx::query_scalar("SELECT id FROM models WHERE id = ?")
        .bind(model_record_id)
        .fetch_optional(container.db_pool())
        .await
        .expect("Model lookup should succeed");
    assert!(
        deleted_model.is_none(),
        "Model record should be removed from database"
    );

    let after_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM download_sessions WHERE model_id = ?")
            .bind(model_id)
            .fetch_one(container.db_pool())
            .await
            .expect("Count query should work");
    assert_eq!(
        after_count, 0,
        "Download session rows should be cleaned up for deleted model"
    );
}
