#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Integration tests for download progress tracking
//!
//! Tests the complete flow of download progress tracking including:
//! - Progress event emission
//! - Progress calculation accuracy
//! - Integration between DownloadSession and DownloadProgress
//! - Event conversion in DownloadEventBridge
use vault::domain::download::{DownloadProgress, DownloadSession, DownloadState};
use vault::features::download::events::infra_events::DownloadEvent as TauriDownloadEvent;

#[test]
fn test_download_progress_snapshot_creation() {
    let session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    let progress = session.progress();

    assert_eq!(progress.bytes_downloaded(), 0);
    assert_eq!(progress.total_bytes(), Some(1_000_000));
    assert_eq!(progress.bytes_per_second(), 0.0);
    assert_eq!(progress.percentage(), Some(0.0));
    assert_eq!(progress.estimated_time_remaining(), None);
}

#[test]
fn test_download_progress_updates() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(250_000, 50_000.0);
    let progress = session.progress();

    assert_eq!(progress.bytes_downloaded(), 250_000);
    assert_eq!(progress.bytes_per_second(), 50_000.0);
    assert_eq!(progress.percentage(), Some(25.0));
    assert_eq!(progress.estimated_time_remaining(), Some(15));
}

#[test]
fn test_download_progress_incremental_updates() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(100_000, 50_000.0);
    assert_eq!(session.progress().percentage(), Some(10.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(18));

    session.update_progress(500_000, 100_000.0);
    assert_eq!(session.progress().percentage(), Some(50.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(5));

    session.update_progress(900_000, 200_000.0);
    assert_eq!(session.progress().percentage(), Some(90.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(1));
}

#[test]
fn test_download_progress_unknown_size() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        None,
        None,
    )
    .unwrap();

    session.update_progress(250_000, 50_000.0);
    let progress = session.progress();

    assert_eq!(progress.bytes_downloaded(), 250_000);
    assert_eq!(progress.total_bytes(), None);
    assert_eq!(progress.percentage(), None);
    assert_eq!(progress.estimated_time_remaining(), None);
}

#[test]
fn test_download_progress_variable_speed() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(100_000, 10_000.0);
    assert_eq!(session.progress().estimated_time_remaining(), Some(90));

    session.update_progress(200_000, 50_000.0);
    assert_eq!(session.progress().estimated_time_remaining(), Some(16));

    session.update_progress(800_000, 100_000.0);
    assert_eq!(session.progress().estimated_time_remaining(), Some(2));
}

#[test]
fn test_download_progress_completion() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(1_000_000, 100_000.0);
    let progress = session.progress();

    assert_eq!(progress.percentage(), Some(100.0));
    assert_eq!(progress.estimated_time_remaining(), Some(0));
    assert!(progress.is_complete());
}

#[test]
fn test_download_progress_edge_cases() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(500_000, 0.0);
    assert_eq!(session.progress().estimated_time_remaining(), None);

    session.update_progress(500_000, -100.0);
    assert_eq!(session.progress().bytes_per_second(), 0.0);
    assert_eq!(session.progress().estimated_time_remaining(), None);

    session.update_progress(1_200_000, 100_000.0);
    assert_eq!(session.progress().percentage(), Some(100.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(0));
}

// DEPRECATED: Old event structure no longer exists
// TODO: Rewrite to test new snapshot-based event structure
#[test]
#[ignore = "needs rewrite for snapshot-based events"]
fn test_tauri_event_serialization() {
    // Old test removed - event structure changed to snapshot-based
}

// DEPRECATED: Old event structure no longer exists
// TODO: Rewrite to test new snapshot-based event structure
#[test]
#[ignore = "needs rewrite for snapshot-based events"]
fn test_tauri_event_deserialization() {
    // Old test removed - event structure changed to snapshot-based
}

#[test]
fn test_download_lifecycle_with_progress() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(10_000_000),
        None,
    )
    .unwrap();

    assert_eq!(session.state(), &DownloadState::Pending);

    session.start().unwrap();
    assert_eq!(session.state(), &DownloadState::Downloading);
    assert_eq!(session.progress().percentage(), Some(0.0));

    session.update_progress(2_500_000, 500_000.0);
    assert_eq!(session.progress().percentage(), Some(25.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(15));

    session.pause().unwrap();
    assert_eq!(session.state(), &DownloadState::Paused);

    session.resume().unwrap();
    assert_eq!(session.state(), &DownloadState::Downloading);

    session.update_progress(5_000_000, 750_000.0);
    assert_eq!(session.progress().percentage(), Some(50.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(7));

    session.update_progress(10_000_000, 1_000_000.0);
    assert_eq!(session.progress().percentage(), Some(100.0));
    assert!(session.progress().is_complete());

    session.complete().unwrap();
    assert_eq!(session.state(), &DownloadState::Completed);
}

#[test]
fn test_large_file_progress() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/large-model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/large-model.gguf"),
        Some(5_000_000_000),
        None,
    )
    .unwrap();

    session.update_progress(1_000_000_000, 10_000_000.0);
    assert_eq!(session.progress().percentage(), Some(20.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(400));

    session.update_progress(2_500_000_000, 25_000_000.0);
    assert_eq!(session.progress().percentage(), Some(50.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(100));
}

#[test]
fn test_slow_download_progress() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(10_000, 100.0);
    assert_eq!(session.progress().percentage(), Some(1.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(9900));
}

#[test]
fn test_very_fast_download_progress() {
    let mut session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap();

    session.update_progress(900_000, 10_000_000.0);
    assert_eq!(session.progress().percentage(), Some(90.0));
    assert_eq!(session.progress().estimated_time_remaining(), Some(0));
}

#[test]
fn test_progress_with_model_metadata() {
    let session = DownloadSession::new(
        "test-id".to_string(),
        "https://example.com/model.gguf".to_string(),
        std::path::PathBuf::from("/tmp/model.gguf"),
        Some(1_000_000),
        None,
    )
    .unwrap()
    .with_model_metadata("Test Model".to_string(), "test-model-v1".to_string());

    assert_eq!(session.model_name(), Some("Test Model"));
    assert_eq!(session.model_id(), Some("test-model-v1"));
    assert_eq!(session.progress().total_bytes(), Some(1_000_000));
}
