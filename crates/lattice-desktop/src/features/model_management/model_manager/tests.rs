// Tests for ModelManager: reranker readiness and paths, cancellation, progress.

use super::ModelManager;
use crate::infrastructure::services::traits::ModelManagerTrait;
use lattice_core::domain::models::curated::CURATED_RERANKER;
use lattice_core::shared::constants::RERANKER_MODEL_DIR;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

/// Create ModelManager with temp directory
fn create_test_manager(temp_dir: &TempDir) -> ModelManager {
    ModelManager::new(temp_dir.path().to_path_buf()).expect("Failed to create ModelManager")
}

#[tokio::test]
async fn test_download_cancellation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manager = Arc::new(create_test_manager(&temp_dir));

    manager.cancel_download();

    // Assert: Cancellation mechanism is tested
    // Note: We cannot access is_cancelled directly (private field)
    // The test verifies that cancel_download() can be called successfully
    // Full cancellation test would require HTTP mocking
}

#[tokio::test]
async fn test_is_reranker_ready() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manager = create_test_manager(&temp_dir);
    let gguf = manager.get_reranker_path();
    fs::create_dir_all(gguf.parent().unwrap())
        .await
        .expect("Failed to create reranker dir");

    std::fs::write(&gguf, b"partial").expect("Failed to write model");
    assert!(
        !manager.is_reranker_ready().await,
        "a file of another size is not the reranker"
    );

    std::fs::File::create(&gguf)
        .and_then(|file| file.set_len(CURATED_RERANKER.bytes))
        .expect("Failed to size model");
    assert!(manager.is_reranker_ready().await);
}

#[tokio::test]
async fn test_get_reranker_paths() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manager = create_test_manager(&temp_dir);

    assert_eq!(
        manager.get_reranker_path(),
        temp_dir
            .path()
            .join(RERANKER_MODEL_DIR)
            .join("qwen3-reranker-0.6b-q8_0.gguf")
    );
}

#[tokio::test]
async fn no_download_progress_is_reported_before_a_download_starts() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manager = create_test_manager(&temp_dir);

    let progress = manager.get_download_progress();

    // Assert: Should return None (TODO in implementation)
    assert!(progress.is_none());
}

#[tokio::test]
async fn test_trait_cancel_download() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manager = create_test_manager(&temp_dir);

    // Note: Cannot access private is_cancelled field directly

    manager.cancel_download();

    // Assert: Cancellation is successful (method doesn't panic)
    // Full verification would require accessing internal state or HTTP mocking
}
