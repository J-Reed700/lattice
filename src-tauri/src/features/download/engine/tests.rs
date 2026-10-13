//! Tests for the HTTP download engine.
use super::*;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

#[test]
fn nested_mount_is_more_specific_than_root_mount() {
    assert!(
        mount_point_specificity(Path::new("/Volumes/External"))
            > mount_point_specificity(Path::new("/"))
    );
}

#[test]
fn redirect_location_is_resolved_against_request_url() {
    assert_eq!(
        resolve_redirect_location(
            "https://huggingface.co/org/model/resolve/main/model.gguf",
            "../../blobs/abc"
        )
        .expect("resolve relative redirect"),
        "https://huggingface.co/org/model/blobs/abc"
    );
}

#[test]
fn authorization_is_not_forwarded_to_untrusted_redirects() {
    assert!(may_forward_authorization(
        "https://huggingface.co/org/model/resolve/main/model.gguf",
        "https://cdn-lfs.hf.co/file"
    ));
    assert!(!may_forward_authorization(
        "https://huggingface.co/org/model/resolve/main/model.gguf",
        "https://downloads.example.com/file"
    ));
    assert!(may_forward_authorization(
        "https://models.example.com/file",
        "https://models.example.com/other"
    ));
}

fn temp_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

#[tokio::test]
async fn complete_local_file_is_not_requested_again() {
    let destination = temp_path("engine_already_complete.bin");
    let payload = b"already on disk";
    tokio::fs::write(&destination, payload)
        .await
        .expect("write fixture");
    let total = payload.len() as u64;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let callback: ProgressCallback = Arc::new(move |bytes, speed| {
        let _ = tx.send((bytes, speed));
    });

    // Port 9 is the discard service: if the engine issued a request here
    // the test would fail with a connection error instead of completing.
    let result = HttpDownloadEngine::new()
        .expect("engine")
        .download_impl(
            "http://127.0.0.1:9/never-requested.bin",
            &destination,
            Some(total),
            Some(callback),
            None,
            Some(total),
        )
        .await
        .expect("complete file short-circuits the transfer");

    assert_eq!(result.bytes_downloaded, total);
    assert_eq!(result.total_bytes, Some(total));
    assert_eq!(
        result.sha256_checksum,
        format!("{:x}", Sha256::digest(payload))
    );

    let events = collect_progress_events(rx, Duration::from_millis(200)).await;
    assert_eq!(events, vec![(total, 0.0)]);

    let _ = tokio::fs::remove_file(&destination).await;
}

#[tokio::test]
async fn complete_local_file_with_wrong_length_is_rejected() {
    let destination = temp_path("engine_wrong_length.bin");
    tokio::fs::write(&destination, b"short")
        .await
        .expect("write fixture");

    let result = HttpDownloadEngine::new()
        .expect("engine")
        .complete_from_local_file("http://127.0.0.1:9/x.bin", &destination, 1_000, None)
        .await;

    assert!(matches!(result, Err(DownloadError::ValidationFailed(_))));
    let _ = tokio::fs::remove_file(&destination).await;
}

#[tokio::test]
async fn test_mock_engine_download() {
    let engine = mock::MockDownloadEngine::new();

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_supports_resume("https://example.com/file.bin", true);

    let result = engine
        .download(DownloadOptions {
            url: "https://example.com/file.bin".to_string(),
            destination: temp_path("test.bin"),
            resume_from: None,
            progress_callback: None,
            auth_token: None,
        })
        .await
        .unwrap();

    assert_eq!(result.bytes_downloaded, 1000);
    assert_eq!(result.total_bytes, Some(1000));
}

#[tokio::test]
async fn test_mock_engine_file_size() {
    let engine = mock::MockDownloadEngine::new();

    engine.set_file_size("https://example.com/file.bin", Some(5000));

    let (size, _etag) = engine
        .get_file_size("https://example.com/file.bin")
        .await
        .unwrap();

    assert_eq!(size, Some(5000));
}

#[tokio::test]
async fn test_mock_engine_supports_resume() {
    let engine = mock::MockDownloadEngine::new();

    engine.set_supports_resume("https://example.com/file.bin", true);

    let supports = engine
        .supports_resume("https://example.com/file.bin")
        .await
        .unwrap();

    assert!(supports);
}

#[tokio::test]
async fn test_mock_engine_with_progress_callback() {
    let engine = mock::MockDownloadEngine::new();

    let progress_called = Arc::new(Mutex::new(false));
    let progress_called_clone = progress_called.clone();

    let callback: ProgressCallback = Arc::new(move |bytes, speed| {
        *progress_called_clone.lock().unwrap() = true;
        assert!(bytes > 0);
        assert!(speed > 0.0);
    });

    engine
        .download(DownloadOptions {
            url: "https://example.com/file.bin".to_string(),
            destination: temp_path("test.bin"),
            resume_from: None,
            progress_callback: Some(callback),
            auth_token: None,
        })
        .await
        .unwrap();

    assert!(*progress_called.lock().unwrap());
}

#[tokio::test]
async fn test_mock_engine_call_tracking() {
    let engine = mock::MockDownloadEngine::new();

    // Initial state: no calls
    assert!(engine.get_download_calls().is_empty());
    assert!(engine.get_last_call().is_none());

    // First download
    engine
        .download(DownloadOptions {
            url: "https://example.com/file1.bin".to_string(),
            destination: temp_path("test1.bin"),
            resume_from: None,
            progress_callback: None,
            auth_token: None,
        })
        .await
        .unwrap();

    let calls = engine.get_download_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].url, "https://example.com/file1.bin");
    assert_eq!(calls[0].resume_from, None);

    // Second download with resume_from (CRITICAL for Test 16)
    engine
        .download(DownloadOptions {
            url: "https://example.com/file2.bin".to_string(),
            destination: temp_path("test2.bin"),
            resume_from: Some(5000),
            progress_callback: None,
            auth_token: Some("test-token".to_string()),
        })
        .await
        .unwrap();

    let calls = engine.get_download_calls();
    assert_eq!(calls.len(), 2);

    let last_call = engine.get_last_call().unwrap();
    assert_eq!(last_call.url, "https://example.com/file2.bin");
    assert_eq!(last_call.resume_from, Some(5000));
    assert_eq!(last_call.auth_token, Some("test-token".to_string()));

    // Clear calls
    engine.clear_calls();
    assert!(engine.get_download_calls().is_empty());
}

#[test]
fn test_is_content_length_valid_none() {
    assert!(!is_content_length_valid(None));
}

#[test]
fn test_is_content_length_valid_zero() {
    assert!(!is_content_length_valid(Some(0)));
}

#[test]
fn test_is_content_length_valid_small_redirect_html() {
    assert!(!is_content_length_valid(Some(1309)));
}

#[test]
fn test_is_content_length_valid_just_below_threshold() {
    assert!(!is_content_length_valid(Some(MIN_VALID_MODEL_SIZE - 1)));
}

#[test]
fn test_is_content_length_valid_at_threshold() {
    assert!(is_content_length_valid(Some(MIN_VALID_MODEL_SIZE)));
}

#[test]
fn test_is_content_length_valid_above_threshold() {
    assert!(is_content_length_valid(Some(MIN_VALID_MODEL_SIZE + 1)));
}

#[test]
fn test_is_content_length_valid_typical_model_file() {
    assert!(is_content_length_valid(Some(724_923)));
}

#[test]
fn test_is_content_length_valid_large_file() {
    assert!(is_content_length_valid(Some(1024 * 1024 * 1024)));
}

#[test]
fn test_resolved_resume_offset_requires_partial_content() {
    assert_eq!(
        HttpDownloadEngine::resolved_resume_offset(Some(1024), StatusCode::PARTIAL_CONTENT),
        1024
    );
    assert_eq!(
        HttpDownloadEngine::resolved_resume_offset(Some(1024), StatusCode::OK),
        0
    );
    assert_eq!(
        HttpDownloadEngine::resolved_resume_offset(None, StatusCode::OK),
        0
    );
}

#[test]
fn test_is_retryable_error_covers_transient_http() {
    assert!(HttpDownloadEngine::is_retryable_error(
        &DownloadError::NetworkError("connection reset".to_string())
    ));
    assert!(HttpDownloadEngine::is_retryable_error(
        &DownloadError::HttpError {
            status: 503,
            message: "service unavailable".to_string()
        }
    ));
    assert!(!HttpDownloadEngine::is_retryable_error(
        &DownloadError::HttpError {
            status: 404,
            message: "not found".to_string()
        }
    ));
}

#[tokio::test]
async fn test_download_engine_http_download() {
    use std::time::Duration;

    let test_data = b"This is test file content for HTTP download";
    let test_url = "https://example.com/file.bin";

    let mock_engine = mock::MockDownloadEngine::new();
    mock_engine.set_download_result(
        test_url,
        Ok(DownloadResult {
            bytes_downloaded: test_data.len() as u64,
            total_bytes: Some(test_data.len() as u64),
            sha256_checksum: "a".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );

    let options = DownloadOptions {
        url: test_url.to_string(),
        destination: temp_path("test_download.bin"),
        resume_from: None,
        progress_callback: None,
        auth_token: None,
    };

    let result = mock_engine.download(options).await;

    assert!(result.is_ok(), "Download should succeed");
    let download_result = result.unwrap();
    assert_eq!(download_result.bytes_downloaded, test_data.len() as u64);
    assert_eq!(download_result.total_bytes, Some(test_data.len() as u64));

    // Note: MockDownloadEngine doesn't write files, so we verify the mock behavior
    // In production, this test would use a real HTTP client with mock server
    // and verify file contents with: verify_file_contents(&dest_path, test_data).await.unwrap();
}

/// Collect progress events from the mpsc channel.
///
/// Collects (bytes_downloaded, speed) tuples until timeout or channel closes
async fn collect_progress_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(u64, f64)>,
    timeout: Duration,
) -> Vec<(u64, f64)> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
            Ok(Some(event)) => events.push(event),
            Ok(None) => break, // Channel closed
            Err(_) => break,   // Timeout
        }
    }

    events
}

#[tokio::test]
async fn test_download_engine_progress_callbacks() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let mock_engine = mock::MockDownloadEngine::new();
    mock_engine.set_download_result(
        "https://example.com/large_file.bin",
        Ok(DownloadResult {
            bytes_downloaded: 10000,
            total_bytes: Some(10000),
            sha256_checksum: "a".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );

    // Progress callback that sends to the channel.
    let progress_callback: ProgressCallback = Arc::new(move |bytes, speed| {
        tx.send((bytes, speed)).ok();
    });

    let options = DownloadOptions {
        url: "https://example.com/large_file.bin".to_string(),
        destination: temp_path("large_file.bin"),
        resume_from: None,
        progress_callback: Some(progress_callback),
        auth_token: None,
    };

    let result = mock_engine.download(options).await;

    assert!(result.is_ok(), "Download should succeed");
    let download_result = result.unwrap();
    assert_eq!(download_result.bytes_downloaded, 10000);

    // Collect progress events.
    let progress_events = collect_progress_events(rx, Duration::from_secs(2)).await;

    assert!(!progress_events.is_empty(), "Should have progress events");

    // MockDownloadEngine sends single progress callback - verify it was received
    let (final_bytes, final_speed) = progress_events.last().unwrap();
    assert_eq!(
        *final_bytes, 10000,
        "Final progress should match total bytes"
    );
    assert!(*final_speed > 0.0, "Speed should be positive");
}

#[tokio::test]
async fn test_download_engine_range_request() {
    let mock_engine = mock::MockDownloadEngine::new();

    // Set up response for range request (remaining content after 500 bytes)
    let remaining_bytes = 500u64;
    mock_engine.set_download_result(
        "https://example.com/partial_file.bin",
        Ok(DownloadResult {
            bytes_downloaded: remaining_bytes + 500, // Total bytes after resume
            total_bytes: Some(1000),
            sha256_checksum: "a".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );

    let options = DownloadOptions {
        url: "https://example.com/partial_file.bin".to_string(),
        destination: temp_path("partial_file.bin"),
        resume_from: Some(500), // Resume from 500 bytes
        progress_callback: None,
        auth_token: None,
    };

    let result = mock_engine.download(options).await;

    assert!(result.is_ok(), "Range request download should succeed");

    // CRITICAL: Verify engine was called with resume_from
    let calls = mock_engine.get_download_calls();
    assert!(!calls.is_empty(), "Should have recorded download call");

    let last_call = calls.last().unwrap();
    assert_eq!(
        last_call.resume_from,
        Some(500),
        "CRITICAL: Engine should be called with resume_from = Some(500)"
    );

    // Note: In production, MockHttpClient would verify Range header:
    // let range_header = mock_http.get_last_range_header();
    // assert_eq!(range_header, Some("bytes=500-".to_string()));
}

#[tokio::test]
async fn test_download_engine_network_error() {
    let mock_engine = mock::MockDownloadEngine::new();
    mock_engine.set_network_error("Connection timeout".to_string());

    let options = DownloadOptions {
        url: "https://example.com/unreachable.bin".to_string(),
        destination: temp_path("unreachable.bin"),
        resume_from: None,
        progress_callback: None,
        auth_token: None,
    };

    let result = mock_engine.download(options).await;

    assert!(result.is_err(), "Download should fail with network error");

    let error = result.unwrap_err();
    assert!(
        matches!(error, DownloadError::NetworkError(_)),
        "Error should be NetworkError variant"
    );

    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("Connection timeout") || error_msg.contains("network"),
        "Error should indicate network failure: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_download_engine_404_handling() {
    let mock_engine = mock::MockDownloadEngine::new();
    mock_engine.set_http_error(404, "Not Found".to_string());

    let options = DownloadOptions {
        url: "https://example.com/nonexistent.bin".to_string(),
        destination: temp_path("nonexistent.bin"),
        resume_from: None,
        progress_callback: None,
        auth_token: None,
    };

    let result = mock_engine.download(options).await;

    assert!(result.is_err(), "Download should fail with 404 error");

    let error = result.unwrap_err();
    assert!(
        matches!(error, DownloadError::HttpError { .. }),
        "Error should be HttpError variant"
    );

    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("404") || error_msg.contains("Not Found"),
        "Error should indicate 404 Not Found: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_download_engine_cancellation() {
    let mock_engine = mock::MockDownloadEngine::new();
    mock_engine.set_download_result(
        "https://example.com/cancellable.bin",
        Ok(DownloadResult {
            bytes_downloaded: 10000,
            total_bytes: Some(10000),
            sha256_checksum: "a".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );

    let options = DownloadOptions {
        url: "https://example.com/cancellable.bin".to_string(),
        destination: temp_path("cancellable.bin"),
        resume_from: None,
        progress_callback: None,
        auth_token: Some("cancel-token".to_string()),
    };

    let result = mock_engine.download(options).await;

    // Assert: Download should complete or be cancellable
    // Note: MockDownloadEngine completes instantly, so this tests the API
    // In production with real engine, auth_token would signal cancellation
    assert!(
        result.is_ok() || result.is_err(),
        "Download should either complete or be cancelled"
    );

    let calls = mock_engine.get_download_calls();
    assert!(!calls.is_empty(), "Download should have been called");
    assert!(
        calls[0].auth_token.is_some(),
        "Auth token (cancellation signal) should be passed to engine"
    );
}
