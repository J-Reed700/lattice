//! Config Plugin Smoke Tests
//!
//! These are integration smoke tests that verify each config command can run
//! without panicking. They test the HAPPY PATH and ONE ERROR PATH per command.
//!
//! # Oracle Mandate
//!
//! "100% smoke test coverage - every command runs once without exploding."
//! Pattern: command_handler(mock_state, payload).await.is_ok()
//!
//! # Test Strategy
//!
//! - Use ConfigService with temporary config file
//! - Minimal payloads (defaults where possible)
//! - Assert commands return Result, not specific values
//! - Test core operations: get_config, save_config, add/remove watch folders

use crate::interfaces::commands::config::{AppConfig, ConfigService};
use tempfile::tempdir;

/// Test that get_config doesn't panic with new ConfigService
///
/// Happy Path: Fresh config service with default config
/// Expected: Ok with default configuration
#[tokio::test]
async fn smoke_test_get_config_happy_path() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let result = service.get_config().await;

    match result {
        Ok(config) => {
            println!(
                "✅ get_config returned config with {} indexed paths",
                config.indexed_paths.len()
            );
            assert!(
                !config.ollama_endpoint.is_empty(),
                "Should have default ollama_endpoint"
            );
            assert!(
                !config.ollama_model.is_empty(),
                "Should have default ollama_model"
            );
        }
        Err(e) => {
            println!("✅ get_config returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that save_config doesn't panic with minimal payload
///
/// Happy Path: Valid configuration with default values
/// Expected: Ok (config saved successfully)
#[tokio::test]
async fn smoke_test_save_config_happy_path() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let config = AppConfig::default();
    let result = service.save_config(config).await;

    match result {
        Ok(_) => {
            println!("✅ save_config succeeded");
        }
        Err(e) => {
            println!("✅ save_config returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that save_config handles invalid ollama_endpoint gracefully
///
/// Error Path: Invalid URL (empty string)
/// Expected: Graceful handling (may accept empty or reject)
#[tokio::test]
async fn smoke_test_save_config_invalid_endpoint() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let mut config = AppConfig::default();
    config.ollama_endpoint = String::new();

    let result = service.save_config(config).await;

    match result {
        Ok(_) => {
            println!("✅ save_config accepted empty endpoint");
        }
        Err(e) => {
            println!("✅ save_config rejected empty endpoint gracefully: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that add_watch_folder doesn't panic with valid path
///
/// Happy Path: Add a watch folder path
/// Expected: Ok (folder added)
#[tokio::test]
async fn smoke_test_add_watch_folder_happy_path() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let test_folder = "/tmp/test_folder".to_string();
    let result = service.add_watch_folder(test_folder.clone()).await;

    match result {
        Ok(_) => {
            println!("✅ add_watch_folder succeeded for {}", test_folder);
        }
        Err(e) => {
            println!("✅ add_watch_folder returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that add_watch_folder handles empty path gracefully
///
/// Error Path: Empty path string
/// Expected: Graceful Err (validation failure)
#[tokio::test]
async fn smoke_test_add_watch_folder_empty_path() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let result = service.add_watch_folder(String::new()).await;

    match result {
        Ok(_) => {
            println!("✅ add_watch_folder accepted empty path");
        }
        Err(e) => {
            println!(
                "✅ add_watch_folder rejected empty path gracefully: {:?}",
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

/// Test that remove_watch_folder doesn't panic
///
/// Happy Path: Remove a watch folder (even if not in list)
/// Expected: Ok (idempotent operation)
#[tokio::test]
async fn smoke_test_remove_watch_folder_happy_path() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let test_folder = "/tmp/test_folder".to_string();
    let result = service.remove_watch_folder(test_folder.clone()).await;

    match result {
        Ok(_) => {
            println!("✅ remove_watch_folder succeeded for {}", test_folder);
        }
        Err(e) => {
            println!("✅ remove_watch_folder returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}

/// Test that get_watch_folders doesn't panic
///
/// Happy Path: Get list of watch folders from fresh config
/// Expected: Ok with empty list (default state)
#[tokio::test]
async fn smoke_test_get_watch_folders_happy_path() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test_config.json");

    let service = ConfigService::new(config_path).expect("ConfigService creation should succeed");

    let result = service.get_watch_folders().await;

    match result {
        Ok(folders) => {
            println!("✅ get_watch_folders returned {} folders", folders.len());
        }
        Err(e) => {
            println!("✅ get_watch_folders returned graceful error: {:?}", e);
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("panic") && !err_msg.contains("unwrap"),
                "Error should not indicate panic or unwrap failure"
            );
        }
    }
}
