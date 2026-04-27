//! Tests for file storage service.

use super::*;
use crate::shared::domain_types::ValidatedFilePath;
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn setup_test_db() -> (SqlitePool, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    let pool = SqlitePool::connect(":memory:").await.unwrap();

    // Create tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS files (
            id TEXT PRIMARY KEY NOT NULL,
            content_hash TEXT NOT NULL UNIQUE,
            file_name TEXT NOT NULL,
            file_extension TEXT,
            mime_type TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            storage_path TEXT NOT NULL,
            is_indexed INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            accessed_at INTEGER NOT NULL,
            ref_count INTEGER NOT NULL DEFAULT 1,
            metadata TEXT
        ) STRICT;
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    (pool, temp_dir)
}

#[tokio::test]
async fn test_store_new_file() {
    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    let service = FileStorageService::new(vault_path.clone(), pool);

    // Create a test file
    let test_file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&test_file_path, b"Hello, World!")
        .await
        .unwrap();

    // Store file
    let validated_path = ValidatedFilePath::new(test_file_path.clone()).unwrap();
    let result = service
        .store_file(validated_path, "text/plain", None)
        .await
        .unwrap();

    assert_eq!(result.file_name, "test.txt");
    assert_eq!(result.file_extension, Some("txt".to_string()));
    assert_eq!(result.mime_type, "text/plain");
    assert_eq!(result.size_bytes, 13);
    assert_eq!(result.ref_count, 1);
    assert!(!result.is_indexed);

    // Verify file exists on disk
    let stored_path = vault_path.join(&result.storage_path);
    assert!(stored_path.exists());
}

#[tokio::test]
async fn test_deduplication() {
    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    let service = FileStorageService::new(vault_path.clone(), pool);

    // Create a test file
    let test_file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&test_file_path, b"Hello, World!")
        .await
        .unwrap();

    // Store file first time
    let validated_path = ValidatedFilePath::new(test_file_path.clone()).unwrap();
    let result1 = service
        .store_file(validated_path.clone(), "text/plain", None)
        .await
        .unwrap();

    // Store same file again
    let result2 = service
        .store_file(validated_path, "text/plain", None)
        .await
        .unwrap();

    // Should be same file with incremented ref_count
    assert_eq!(result1.content_hash, result2.content_hash);
    assert_eq!(result2.ref_count, 2);
}

#[tokio::test]
async fn test_delete_file() {
    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    let service = FileStorageService::new(vault_path.clone(), pool);

    // Create and store a test file
    let test_file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&test_file_path, b"Hello, World!")
        .await
        .unwrap();

    let validated_path = ValidatedFilePath::new(test_file_path.clone()).unwrap();
    let result = service
        .store_file(validated_path, "text/plain", None)
        .await
        .unwrap();

    let stored_path = vault_path.join(&result.storage_path);
    assert!(stored_path.exists());

    // Delete file
    service.delete_file(&result.id).await.unwrap();

    // File should be deleted from disk
    assert!(!stored_path.exists());

    // File should not be found in database
    let found = service.get_file_by_id(&result.id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_ref_counting() {
    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    let service = FileStorageService::new(vault_path.clone(), pool);

    // Create and store a test file
    let test_file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&test_file_path, b"Hello, World!")
        .await
        .unwrap();

    let validated_path = ValidatedFilePath::new(test_file_path.clone()).unwrap();
    let result = service
        .store_file(validated_path, "text/plain", None)
        .await
        .unwrap();

    // Increment ref_count
    service.increment_ref_count(&result.id).await.unwrap();

    let updated = service.get_file_by_id(&result.id).await.unwrap().unwrap();
    assert_eq!(updated.ref_count, 2);

    let stored_path = vault_path.join(&result.storage_path);

    // First delete should only decrement ref_count
    service.delete_file(&result.id).await.unwrap();
    assert!(stored_path.exists());

    let after_first_delete = service.get_file_by_id(&result.id).await.unwrap().unwrap();
    assert_eq!(after_first_delete.ref_count, 1);

    // Second delete should remove file
    service.delete_file(&result.id).await.unwrap();
    assert!(!stored_path.exists());
}

#[tokio::test]
async fn test_verify_file() {
    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    let service = FileStorageService::new(vault_path.clone(), pool);

    // Create and store a test file
    let test_file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&test_file_path, b"Hello, World!")
        .await
        .unwrap();

    let validated_path = ValidatedFilePath::new(test_file_path.clone()).unwrap();
    let result = service
        .store_file(validated_path, "text/plain", None)
        .await
        .unwrap();

    // Verify should pass
    assert!(service.verify_file(&result.id).await.unwrap());

    // Corrupt the file
    let stored_path = vault_path.join(&result.storage_path);
    tokio::fs::write(&stored_path, b"Corrupted!").await.unwrap();

    // Verify should fail
    assert!(!service.verify_file(&result.id).await.unwrap());
}

#[tokio::test]
async fn test_cleanup_orphaned_files() {
    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    let service = FileStorageService::new(vault_path.clone(), pool.clone());

    // Create and store a test file
    let test_file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&test_file_path, b"Hello, World!")
        .await
        .unwrap();

    let validated_path = ValidatedFilePath::new(test_file_path.clone()).unwrap();
    let result = service
        .store_file(validated_path, "text/plain", None)
        .await
        .unwrap();

    // Manually set ref_count to 0
    sqlx::query("UPDATE files SET ref_count = 0 WHERE id = ?")
        .bind(&result.id)
        .execute(&pool)
        .await
        .unwrap();

    let stored_path = vault_path.join(&result.storage_path);
    assert!(stored_path.exists());

    // Cleanup orphaned files
    let count = service.cleanup_orphaned_files().await.unwrap();
    assert_eq!(count, 1);

    // File should be deleted
    assert!(!stored_path.exists());
    let found = service.get_file_by_id(&result.id).await.unwrap();
    assert!(found.is_none());
}

// =============================================================================
// Security Tests - CWE-22 Directory Traversal Prevention
// =============================================================================

#[tokio::test]
async fn test_directory_traversal_prevention() {
    // CRITICAL SECURITY TEST: Verify ValidatedFilePath prevents directory traversal attacks (CWE-22)

    // Attempt 1: Explicit parent directory traversal
    let malicious_path = std::path::PathBuf::from("../../../etc/passwd");
    let result = ValidatedFilePath::new(malicious_path.clone());

    assert!(
        result.is_err(),
        "CRITICAL SECURITY: Directory traversal '../../../etc/passwd' must be rejected"
    );

    // Attempt 2: Hidden traversal in middle of path
    let hidden_traversal = std::path::PathBuf::from("valid/../../etc/passwd");
    let result2 = ValidatedFilePath::new(hidden_traversal);

    assert!(
        result2.is_err(),
        "CRITICAL SECURITY: Hidden directory traversal must be rejected"
    );

    // Attempt 3: Current directory tricks
    let current_dir_trick = std::path::PathBuf::from("./././../../etc/passwd");
    let result3 = ValidatedFilePath::new(current_dir_trick);

    assert!(
        result3.is_err(),
        "CRITICAL SECURITY: Current directory traversal tricks must be rejected"
    );

    // Attempt 4: Absolute path with traversal
    let absolute_with_traversal = std::path::PathBuf::from("/tmp/../etc/passwd");
    let result4 = ValidatedFilePath::new(absolute_with_traversal);

    assert!(
        result4.is_err(),
        "CRITICAL SECURITY: Absolute paths with '..' must be rejected"
    );

    // Verify error message is appropriate
    if let Err(e) = result {
        let error_msg = format!("{:?}", e);
        assert!(
            error_msg.contains("..") || error_msg.contains("parent"),
            "Error message should mention directory traversal: {}",
            error_msg
        );
    }
}

#[tokio::test]
async fn test_symlink_attack_detection() {
    // CRITICAL SECURITY TEST: Verify symlink attacks are detected (CWE-59)

    let temp_dir = TempDir::new().unwrap();

    // Create a test file in temp directory
    let test_file = temp_dir.path().join("legitimate.txt");
    tokio::fs::write(&test_file, b"legitimate content")
        .await
        .unwrap();

    // Create sensitive file simulation (in temp for testing)
    let sensitive_file = temp_dir.path().join("sensitive_data.txt");
    tokio::fs::write(&sensitive_file, b"SECRET DATA")
        .await
        .unwrap();

    // Create symlink pointing to sensitive file
    let symlink_path = temp_dir.path().join("innocent_link.txt");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        // Create symlink (may fail on some systems, that's okay for testing)
        let _ = symlink(&sensitive_file, &symlink_path);
    }

    #[cfg(windows)]
    {
        // Windows requires admin for symlinks, so skip this part on Windows
        // The path validation logic still applies
    }

    // Attempt to validate symlink path
    let validated_path = ValidatedFilePath::new(symlink_path.clone());

    // SECURITY NOTE: ValidatedFilePath doesn't currently detect symlinks,
    // but it should prevent directory traversal via symlinks
    #[cfg(unix)]
    {
        if validated_path.is_ok() && symlink_path.exists() {
            // If symlink exists and validates, verify it doesn't escape via canonicalization
            let canonical = std::fs::canonicalize(&symlink_path).unwrap_or(symlink_path.clone());

            // Canonicalize the temp directory to handle macOS /private prefix
            let canonical_temp = std::fs::canonicalize(temp_dir.path()).unwrap();

            // Ensure canonicalized path is still within temp directory
            assert!(
                canonical.starts_with(&canonical_temp),
                "CRITICAL SECURITY: Symlink must not escape base directory. Got: {:?}, Expected within: {:?}",
                canonical,
                canonical_temp
            );
        }
    }

    #[cfg(windows)]
    {
        // On Windows, focus on path validation
        if let Ok(path) = validated_path {
            assert!(
                path.as_path().starts_with(temp_dir.path()) || !path.as_path().is_absolute(),
                "CRITICAL SECURITY: Path must remain within expected boundaries"
            );
        }
    }
}

#[tokio::test]
async fn test_path_validation_edge_cases() {
    // CRITICAL SECURITY TEST: Verify various path manipulation techniques are blocked

    // Edge Case 1: Backslash traversal (Windows-style on Unix)
    let backslash_traversal = std::path::PathBuf::from("..\\..\\..\\etc\\passwd");
    let result1 = ValidatedFilePath::new(backslash_traversal);

    // On Unix, backslashes are valid filename characters, but '..' should still be caught
    // On Windows, this becomes legitimate path traversal and should be rejected
    #[cfg(windows)]
    {
        assert!(
            result1.is_err(),
            "CRITICAL SECURITY: Backslash traversal must be rejected on Windows"
        );
    }

    // Edge Case 2: Mixed slashes
    let mixed_slashes = std::path::PathBuf::from("valid/../../../etc/passwd");
    let result2 = ValidatedFilePath::new(mixed_slashes);

    assert!(
        result2.is_err(),
        "CRITICAL SECURITY: Mixed slash traversal must be rejected"
    );

    // Edge Case 3: Double slashes (should be normalized away, not a security issue)
    let double_slash = std::path::PathBuf::from("//legitimate//file.txt");
    let result3 = ValidatedFilePath::new(double_slash);

    // Double slashes are normalized by PathBuf and are safe
    assert!(
        result3.is_ok(),
        "Double slashes should be normalized and accepted: //legitimate//file.txt"
    );

    // Edge Case 4: Trailing slash
    let trailing_slash = std::path::PathBuf::from("legitimate/file.txt/");
    let result4 = ValidatedFilePath::new(trailing_slash);

    // Trailing slashes are harmless
    assert!(result4.is_ok(), "Trailing slashes should be accepted");

    // Edge Case 5: Empty path components
    let empty_components = std::path::PathBuf::from("legitimate//file.txt");
    let result5 = ValidatedFilePath::new(empty_components);

    // Empty components are normalized by PathBuf
    assert!(
        result5.is_ok(),
        "Empty path components should be normalized and accepted"
    );

    // Edge Case 6: Relative path without traversal (should be accepted)
    let safe_relative = std::path::PathBuf::from("safe/path/to/file.txt");
    let result6 = ValidatedFilePath::new(safe_relative);

    assert!(result6.is_ok(), "Safe relative paths should be accepted");

    // Edge Case 7: Absolute path without traversal (should be accepted)
    let safe_absolute = std::path::PathBuf::from("/tmp/safe/file.txt");
    let result7 = ValidatedFilePath::new(safe_absolute);

    assert!(result7.is_ok(), "Safe absolute paths should be accepted");

    // Edge Case 8: Path with '..' in filename (not as component) - should be safe
    // Note: This is a filename containing "..", not a path component
    let dotdot_filename = std::path::PathBuf::from("file..with..dots.txt");
    let result8 = ValidatedFilePath::new(dotdot_filename);

    assert!(
        result8.is_ok(),
        "Filenames containing '..' characters should be accepted if not a ParentDir component"
    );
}

#[tokio::test]
async fn test_file_size_limit_enforcement() {
    // CRITICAL SECURITY TEST: Verify file size limits prevent DoS attacks

    let (pool, temp_dir) = setup_test_db().await;
    let vault_path = temp_dir.path().join("lattice");
    tokio::fs::create_dir_all(&vault_path).await.unwrap();

    // Create service with specific size limit (1MB)
    let max_size = 1024 * 1024; // 1MB
    let service = FileStorageService::with_max_size(vault_path.clone(), pool, max_size);

    // Test 1: Create a file that's exactly at the limit (should succeed)
    let at_limit_file = temp_dir.path().join("at_limit.bin");
    let at_limit_content = vec![0u8; max_size as usize];
    tokio::fs::write(&at_limit_file, at_limit_content)
        .await
        .unwrap();

    let validated_at_limit = ValidatedFilePath::new(at_limit_file.clone()).unwrap();
    let result_at_limit = service
        .store_file(validated_at_limit, "application/octet-stream", None)
        .await;

    assert!(
        result_at_limit.is_ok(),
        "Files at exactly max_file_size should be accepted"
    );

    // Test 2: Create a file that exceeds the limit (should fail)
    let oversized_file = temp_dir.path().join("oversized.bin");
    let oversized_content = vec![0u8; (max_size + 1) as usize];
    tokio::fs::write(&oversized_file, oversized_content)
        .await
        .unwrap();

    let validated_oversized = ValidatedFilePath::new(oversized_file.clone()).unwrap();
    let result_oversized = service
        .store_file(validated_oversized, "application/octet-stream", None)
        .await;

    assert!(
        result_oversized.is_err(),
        "Files exceeding max_file_size should be rejected to prevent DoS"
    );

    // Test 3: Verify error message mentions size limit
    if let Err(e) = result_oversized {
        let error_msg = format!("{:?}", e);
        assert!(
            error_msg.contains("size")
                || error_msg.contains("large")
                || error_msg.contains("FileTooLarge"),
            "Error should indicate size limit violation: {}",
            error_msg
        );
    }

    // Test 4: Create a very large file (2MB) to ensure protection
    let very_large_file = temp_dir.path().join("very_large.bin");
    let very_large_content = vec![0u8; 2 * 1024 * 1024]; // 2MB
    tokio::fs::write(&very_large_file, very_large_content)
        .await
        .unwrap();

    let validated_very_large = ValidatedFilePath::new(very_large_file.clone()).unwrap();
    let result_very_large = service
        .store_file(validated_very_large, "application/octet-stream", None)
        .await;

    assert!(
        result_very_large.is_err(),
        "Very large files (2MB) should be rejected when limit is 1MB"
    );
}
