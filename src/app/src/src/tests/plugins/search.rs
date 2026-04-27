//! Search Plugin Smoke Tests
//!
//! These are integration smoke tests that verify each search command can run
//! without panicking. They test the HAPPY PATH and ONE ERROR PATH per command.
//!
//! # Oracle Mandate
//!
//! "100% smoke test coverage - every command runs once without exploding."
//! Pattern: command_handler(mock_state, payload).await.is_ok()
//!
//! # Test Strategy
//!
//! - Use real Container with in-memory database (production parity)
//! - Minimal payloads (defaults where possible)
//! - No embedding model loaded (smoke tests shouldn't require GPU/API)
//! - Assert commands return Result, not specific values

use crate::features::search::dto::{SearchModeDto, SearchRequestDto};
use crate::features::search::commands::{hybrid_search_impl, semantic_search_impl};
use crate::shared::api_result::ApiResult;
use crate::tests::common::setup_test_container;

/// Test that semantic_search doesn't panic with minimal payload
///
/// Happy Path: Query string provided, no embedding model loaded
/// Expected: Either Ok (if fallback exists) or graceful Err (no panic)
#[tokio::test]
async fn smoke_test_semantic_search_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    // Minimal payload: query + default limit
    let request = SearchRequestDto {
        query: "test query".to_string(),
        limit: Some(10),
        threshold: None,
        mode: SearchModeDto::Vector,
    };

    let result = semantic_search_impl(&container, request).await;

    // Assert: Command completes without panic
    // May return Err (no embedding model loaded), but should not panic
    match result {
        ApiResult::Success { .. } => {
            // Success path - command worked
            println!("✅ semantic_search_impl returned results");
        }
        ApiResult::Error { error, .. } => {
            // Graceful error - acceptable for smoke test
            println!(
                "✅ semantic_search_impl returned graceful error: {:?}",
                error
            );
            let err_msg = format!("{:?}", error);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that semantic_search handles empty query gracefully
///
/// Error Path: Empty query string
/// Expected: Graceful Err (validation failure or empty results)
#[tokio::test]
async fn smoke_test_semantic_search_empty_query() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let request = SearchRequestDto {
        query: String::new(), // Empty query
        limit: Some(10),
        threshold: None,
        mode: SearchModeDto::Vector,
    };

    let result = semantic_search_impl(&container, request).await;

    // Assert: Command handles empty query without panic
    match result {
        ApiResult::Success { data: response, .. } => {
            // Empty results is acceptable
            println!(
                "✅ semantic_search_impl handled empty query, returned {} results",
                response.results.len()
            );
        }
        ApiResult::Error { error, .. } => {
            // Validation error is acceptable
            println!(
                "✅ semantic_search_impl rejected empty query gracefully: {:?}",
                error
            );
            let err_msg = format!("{:?}", error);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that hybrid_search doesn't panic with minimal payload
#[tokio::test]
async fn smoke_test_hybrid_search_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = hybrid_search_impl(
        &container,
        "test query".to_string(),
        10,
        "hybrid".to_string(),
    )
    .await;

    // Assert: Command completes without panic
    match result {
        ApiResult::Success { .. } => {
            println!("✅ hybrid_search_impl returned results");
        }
        ApiResult::Error { error, .. } => {
            println!("✅ hybrid_search_impl returned graceful error: {:?}", error);
            let err_msg = format!("{:?}", error);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that hybrid_search handles empty query gracefully
#[tokio::test]
async fn smoke_test_hybrid_search_empty_query() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = hybrid_search_impl(&container, String::new(), 10, "hybrid".to_string()).await;

    match result {
        ApiResult::Success { data: results, .. } => {
            println!(
                "✅ hybrid_search_impl handled empty query, returned {} results",
                results.len()
            );
        }
        ApiResult::Error { error, .. } => {
            println!(
                "✅ hybrid_search_impl rejected empty query gracefully: {:?}",
                error
            );
            let err_msg = format!("{:?}", error);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that keyword_search doesn't panic with minimal payload
#[tokio::test]
async fn smoke_test_keyword_search_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = hybrid_search_impl(
        &container,
        "test query".to_string(),
        10,
        "keyword".to_string(),
    )
    .await;

    // Assert: Command completes without panic
    match result {
        ApiResult::Success { .. } => {
            println!("✅ keyword_search (via hybrid_search_impl) returned results");
        }
        ApiResult::Error { error, .. } => {
            println!(
                "✅ keyword_search (via hybrid_search_impl) returned graceful error: {:?}",
                error
            );
            let err_msg = format!("{:?}", error);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that keyword_search handles empty query gracefully
#[tokio::test]
async fn smoke_test_keyword_search_empty_query() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = hybrid_search_impl(&container, String::new(), 10, "keyword".to_string()).await;

    match result {
        ApiResult::Success { data: results, .. } => {
            println!("✅ keyword_search (via hybrid_search_impl) handled empty query, returned {} results", results.len());
        }
        ApiResult::Error { error, .. } => {
            println!(
                "✅ keyword_search (via hybrid_search_impl) rejected empty query gracefully: {:?}",
                error
            );
            let err_msg = format!("{:?}", error);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}
