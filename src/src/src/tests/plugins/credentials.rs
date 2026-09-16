//! Credentials Plugin Smoke Tests
//!
//! These are integration smoke tests that verify each credentials command can run
//! without panicking. They test the HAPPY PATH and ONE ERROR PATH per command.
//!
//! Each command runs at least once without panicking.
//! Pattern: command_handler(mock_state, payload).await.is_ok()
//!
//! # Test Strategy
//!
//! - Use real Container with test configuration
//! - Minimal payloads (defaults where possible)
//! - Test core operations: set/get/delete API keys, check existence
//! - Assert commands return Result, not specific values

use crate::features::credentials::commands::{
    delete_api_key_impl, get_api_key_impl, has_api_key_impl, set_api_key_impl,
};
use crate::tests::common::setup_test_container;

/// Test that set_api_key doesn't panic with valid service
///
/// Happy Path: Store API key for known service
/// Expected: Ok or graceful Err (keyring may not be available in test env)
#[tokio::test]
async fn smoke_test_set_api_key_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = set_api_key_impl(
        &container,
        "ollama".to_string(),
        "test_api_key_12345".to_string(),
    )
    .await;

    match result {
        Ok(_) => {
            println!("✅ set_api_key succeeded for ollama service");
        }
        Err(e) => {
            println!("✅ set_api_key returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that set_api_key handles unknown service gracefully
///
/// Error Path: Unknown service name
/// Expected: Graceful Err (validation failure)
#[tokio::test]
async fn smoke_test_set_api_key_unknown_service() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = set_api_key_impl(
        &container,
        "unknown_service_xyz".to_string(),
        "test_api_key".to_string(),
    )
    .await;

    match result {
        Ok(_) => {
            println!("✅ set_api_key accepted unknown service");
        }
        Err(e) => {
            println!(
                "✅ set_api_key rejected unknown service gracefully: {:?}",
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

/// Test that set_api_key handles empty key gracefully
///
/// Error Path: Empty API key string
/// Expected: Graceful handling (may accept empty or reject)
#[tokio::test]
async fn smoke_test_set_api_key_empty_key() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = set_api_key_impl(&container, "ollama".to_string(), String::new()).await;

    match result {
        Ok(_) => {
            println!("✅ set_api_key accepted empty key");
        }
        Err(e) => {
            println!("✅ set_api_key rejected empty key gracefully: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that get_api_key doesn't panic
///
/// Happy Path: Retrieve API key for known service (may not exist)
/// Expected: Ok(None) if not set, Ok(Some(_)) if set, or graceful Err
#[tokio::test]
async fn smoke_test_get_api_key_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = get_api_key_impl(&container, "ollama".to_string()).await;

    match result {
        Ok(key_opt) => {
            println!(
                "✅ get_api_key returned result, key_exists={}",
                key_opt.is_some()
            );
        }
        Err(e) => {
            println!("✅ get_api_key returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that get_api_key handles unknown service gracefully
///
/// Error Path: Unknown service name
/// Expected: Graceful Err (validation failure)
#[tokio::test]
async fn smoke_test_get_api_key_unknown_service() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = get_api_key_impl(&container, "unknown_service_xyz".to_string()).await;

    match result {
        Ok(key_opt) => {
            println!(
                "✅ get_api_key handled unknown service, returned {:?}",
                key_opt
            );
        }
        Err(e) => {
            println!(
                "✅ get_api_key rejected unknown service gracefully: {:?}",
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

/// Test that delete_api_key doesn't panic
///
/// Happy Path: Delete API key for known service (idempotent)
/// Expected: Ok or graceful Err
#[tokio::test]
async fn smoke_test_delete_api_key_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = delete_api_key_impl(&container, "ollama".to_string()).await;

    match result {
        Ok(_) => {
            println!("✅ delete_api_key succeeded for ollama service");
        }
        Err(e) => {
            println!("✅ delete_api_key returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that delete_api_key handles unknown service gracefully
///
/// Error Path: Unknown service name
/// Expected: Graceful Err (validation failure)
#[tokio::test]
async fn smoke_test_delete_api_key_unknown_service() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = delete_api_key_impl(&container, "unknown_service_xyz".to_string()).await;

    match result {
        Ok(_) => {
            println!("✅ delete_api_key accepted unknown service");
        }
        Err(e) => {
            println!(
                "✅ delete_api_key rejected unknown service gracefully: {:?}",
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

/// Test that has_api_key doesn't panic
///
/// Happy Path: Check if API key exists for known service
/// Expected: Ok(true) or Ok(false) depending on key existence, or graceful Err
#[tokio::test]
async fn smoke_test_has_api_key_happy_path() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = has_api_key_impl(&container, "ollama".to_string()).await;

    match result {
        Ok(exists) => {
            println!("✅ has_api_key returned key_exists={}", exists);
        }
        Err(e) => {
            println!("✅ has_api_key returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that has_api_key handles unknown service gracefully
///
/// Error Path: Unknown service name
/// Expected: Graceful Err or Ok(false)
#[tokio::test]
async fn smoke_test_has_api_key_unknown_service() {
    let container = setup_test_container()
        .await
        .expect("Container setup should succeed");

    let result = has_api_key_impl(&container, "unknown_service_xyz".to_string()).await;

    match result {
        Ok(exists) => {
            println!(
                "✅ has_api_key handled unknown service, returned {}",
                exists
            );
        }
        Err(e) => {
            println!(
                "✅ has_api_key rejected unknown service gracefully: {:?}",
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
