//! Embeddings Plugin Smoke Tests
//!
//! These are integration smoke tests that verify each embeddings command can run
//! without panicking. They test the HAPPY PATH and ONE ERROR PATH per command.
//!
//! Each command runs at least once without panicking.
//! Pattern: command_handler(mock_state, payload).await.is_ok()
//!
//! # Test Strategy
//!
//! - Use default EmbeddingState (no model loaded - acceptable for smoke tests)
//! - Minimal payloads (defaults where possible)
//! - Assert commands return Result, not specific values
//! - Acceptable outcomes: Ok (if CPU fallback exists) OR graceful Err (no GPU/model)
//!
//! # Note
//!
//! We test the `_impl` functions directly since they're easier to call from tests
//! (no need to mock tauri::State). The public tauri commands are thin wrappers.

use crate::features::embedding::commands::{
    generate_embedding_impl, generate_embeddings_batch_impl, get_embedding_model_info_impl,
    EmbeddingState,
};

/// Test that generate_embedding doesn't panic with minimal payload
///
/// Happy Path: Valid text provided, no embedding model loaded
/// Expected: Either Ok (if CPU fallback) or graceful Err (no panic)
#[tokio::test]
async fn smoke_test_generate_embedding_happy_path() {
    let state = EmbeddingState::default();

    let result = generate_embedding_impl("test text".to_string(), &state).await;

    match result {
        Ok(embedding) => {
            println!(
                "✅ generate_embedding returned {} dimensions",
                embedding.len()
            );
            assert!(
                !embedding.is_empty(),
                "Embedding should have non-zero dimensions"
            );
        }
        Err(e) => {
            println!("✅ generate_embedding returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that generate_embedding handles empty text gracefully
///
/// Error Path: Empty string
/// Expected: Graceful Err (validation failure)
#[tokio::test]
async fn smoke_test_generate_embedding_empty_text() {
    let state = EmbeddingState::default();

    let result = generate_embedding_impl(String::new(), &state).await;

    match result {
        Ok(embedding) => {
            println!(
                "✅ generate_embedding handled empty text, returned {} dimensions",
                embedding.len()
            );
        }
        Err(e) => {
            println!(
                "✅ generate_embedding rejected empty text gracefully: {:?}",
                e
            );
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that generate_embedding handles very long text gracefully
///
/// Error Path: Text exceeding max length (10k chars)
/// Expected: Graceful Err or truncation
#[tokio::test]
async fn smoke_test_generate_embedding_long_text() {
    let state = EmbeddingState::default();

    let long_text = "a".repeat(15_000);

    let result = generate_embedding_impl(long_text, &state).await;

    match result {
        Ok(embedding) => {
            println!(
                "✅ generate_embedding handled long text, returned {} dimensions",
                embedding.len()
            );
        }
        Err(e) => {
            println!(
                "✅ generate_embedding handled long text gracefully: {:?}",
                e
            );
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that generate_embeddings_batch doesn't panic with minimal payload
///
/// Happy Path: Valid batch of texts
/// Expected: Either Ok (if CPU fallback) or graceful Err
#[tokio::test]
async fn smoke_test_generate_embeddings_batch_happy_path() {
    let state = EmbeddingState::default();

    let texts = vec![
        "text 1".to_string(),
        "text 2".to_string(),
        "text 3".to_string(),
    ];

    let result = generate_embeddings_batch_impl(texts, &state).await;

    match result {
        Ok(embeddings) => {
            println!(
                "✅ generate_embeddings_batch returned {} embeddings",
                embeddings.len()
            );
            assert_eq!(
                embeddings.len(),
                3,
                "Should return 3 embeddings for 3 texts"
            );
        }
        Err(e) => {
            println!(
                "✅ generate_embeddings_batch returned graceful error: {:?}",
                e
            );
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that generate_embeddings_batch handles empty batch gracefully
///
/// Error Path: Empty texts array
/// Expected: Graceful Err or empty result
#[tokio::test]
async fn smoke_test_generate_embeddings_batch_empty() {
    let state = EmbeddingState::default();

    let result = generate_embeddings_batch_impl(vec![], &state).await;

    match result {
        Ok(embeddings) => {
            println!(
                "✅ generate_embeddings_batch handled empty batch, returned {} embeddings",
                embeddings.len()
            );
        }
        Err(e) => {
            println!(
                "✅ generate_embeddings_batch rejected empty batch gracefully: {:?}",
                e
            );
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that generate_embeddings_batch handles large batch gracefully
///
/// Error Path: Batch size exceeding limit (100 texts)
/// Expected: Graceful Err (DoS protection)
#[tokio::test]
async fn smoke_test_generate_embeddings_batch_too_large() {
    let state = EmbeddingState::default();

    let large_batch: Vec<String> = (0..150).map(|i| format!("text {}", i)).collect();

    let result = generate_embeddings_batch_impl(large_batch, &state).await;

    match result {
        Ok(embeddings) => {
            println!(
                "✅ generate_embeddings_batch handled large batch, returned {} embeddings",
                embeddings.len()
            );
        }
        Err(e) => {
            println!(
                "✅ generate_embeddings_batch rejected large batch gracefully: {:?}",
                e
            );
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that get_embedding_model_info doesn't panic
///
/// Happy Path: Request model information
/// Expected: Ok with model info (even if model not loaded)
#[tokio::test]
async fn smoke_test_get_embedding_model_info() {
    let state = EmbeddingState::default();

    let result = get_embedding_model_info_impl(&state).await;

    match result {
        Ok(info) => {
            println!(
                "✅ get_embedding_model_info returned: model={}, dimensions={}, loaded={}",
                info.model_name, info.dimensions, info.is_loaded
            );
            assert!(
                !info.model_name.is_empty(),
                "Model name should not be empty"
            );
            assert!(info.dimensions > 0, "Dimensions should be positive");
        }
        Err(e) => {
            println!(
                "✅ get_embedding_model_info returned graceful error: {:?}",
                e
            );
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}
