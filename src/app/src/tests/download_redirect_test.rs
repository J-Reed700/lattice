#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

/// Test that download engine properly handles redirects
///
/// This test verifies the fix for the BGE-M3 download bug where:
/// - HEAD request follows redirects and finds the file
/// - GET request must use the FINAL redirect URL, not the original URL
///
/// Without the fix, GET would use original URL and return 0 bytes.
use lattice::features::download::engine::{DownloadEngine, DownloadOptions, HttpDownloadEngine};
use std::path::PathBuf;
use tempfile::TempDir;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};
#[tokio::test]
async fn test_download_follows_redirects() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let dest_path = temp_dir.path().join("test_file.bin");

    let test_data = b"Hello from redirected URL!";

    // HEAD request to original URL redirects to final URL
    Mock::given(method("HEAD"))
        .and(path("/original"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/final", mock_server.uri()))
                .insert_header("content-length", "0"),
        )
        .mount(&mock_server)
        .await;

    // HEAD request to final URL returns actual size
    Mock::given(method("HEAD"))
        .and(path("/final"))
        .respond_with(
            ResponseTemplate::new(200).insert_header("content-length", test_data.len().to_string()),
        )
        .mount(&mock_server)
        .await;

    // Small responses intentionally trigger the engine's Range probe before
    // the real download. Keep the probe distinct from the full-body GET.
    Mock::given(method("GET"))
        .and(path("/final"))
        .and(header("range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_bytes(&test_data[..1])
                .insert_header("content-range", format!("bytes 0-0/{}", test_data.len())),
        )
        .with_priority(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    // GET request to final URL returns data
    Mock::given(method("GET"))
        .and(path("/final"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(test_data)
                .insert_header("content-length", test_data.len().to_string()),
        )
        .with_priority(2)
        .expect(1) // Should be called exactly once
        .mount(&mock_server)
        .await;

    // GET request to original URL should NOT be called if fix works
    Mock::given(method("GET"))
        .and(path("/original"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes([]) // Empty response
                .insert_header("content-length", "0"),
        )
        .expect(0) // Should NOT be called
        .mount(&mock_server)
        .await;

    let engine = HttpDownloadEngine::new().unwrap();

    // Download using original URL (which redirects)
    let result = engine
        .download(DownloadOptions {
            url: format!("{}/original", mock_server.uri()),
            destination: dest_path.clone(),
            resume_from: None,
            progress_callback: None,
            auth_token: None,
        })
        .await
        .unwrap();

    // Verify download succeeded with correct size
    assert_eq!(result.bytes_downloaded, test_data.len() as u64);
    assert_eq!(result.total_bytes, Some(test_data.len() as u64));
    assert!(dest_path.exists());

    // Verify file contains correct data (proving GET used final URL)
    let downloaded_data = std::fs::read(&dest_path).unwrap();
    assert_eq!(downloaded_data, test_data);
}

#[tokio::test]
async fn test_get_file_size_returns_final_url() {
    let mock_server = MockServer::start().await;

    // HEAD request redirects
    Mock::given(method("HEAD"))
        .and(path("/original"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/final", mock_server.uri()))
                .insert_header("content-length", "0"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("HEAD"))
        .and(path("/final"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-length", "12345"))
        .mount(&mock_server)
        .await;

    // Wiremock may normalize an empty HEAD response to Content-Length: 0.
    // Model the production fallback so the test verifies the same size via
    // Content-Range when the HEAD length is unavailable or suspicious.
    Mock::given(method("GET"))
        .and(path("/final"))
        .and(header("range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_bytes([0])
                .insert_header("content-range", "bytes 0-0/12345"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let engine = HttpDownloadEngine::new().unwrap();

    let (size, final_url) = engine
        .get_file_size(&format!("{}/original", mock_server.uri()))
        .await
        .unwrap();

    // Verify size is correct
    assert_eq!(size, Some(12345));

    // Verify final URL is returned (not original)
    assert!(
        final_url.contains("/final"),
        "Expected final URL, got: {}",
        final_url
    );
    assert!(
        !final_url.contains("/original") || final_url.ends_with("/final"),
        "Final URL should not be the original: {}",
        final_url
    );
}
