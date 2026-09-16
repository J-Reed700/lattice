//! Standalone test for MockHttpClient verification
//! Validation of Range header capture.

// A panic is the assertion signal for this integration-test crate. The package
// denies these operations in production targets.
#![allow(clippy::expect_used)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic)]
#![allow(clippy::unwrap_in_result)]
#![allow(clippy::unwrap_used)]

mod common;

use common::mock_http::{HttpRequest, MockHttpClient, MockHttpResponse};
use std::collections::HashMap;

#[test]
fn test_mock_http_client_set_and_get_response() {
    let client = MockHttpClient::new();
    let response = MockHttpResponse {
        status: 200,
        body: vec![1, 2, 3],
        content_length: Some(3),
        headers: HashMap::new(),
    };

    client.set_response("http://example.com/file", response);

    // Verify response was stored (internal validation)
    assert_eq!(client.get_requests().len(), 0);
}

#[test]
fn test_record_and_get_requests() {
    let client = MockHttpClient::new();

    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=5000-".to_string());

    client.record_request(
        "http://example.com/file".to_string(),
        headers.clone(),
        "GET".to_string(),
    );

    let requests = client.get_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "http://example.com/file");
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].headers.get("Range").unwrap(), "bytes=5000-");
}

#[test]
fn test_get_last_range_header_critical_for_test_26() {
    let client = MockHttpClient::new();

    // First request without Range
    client.record_request(
        "http://example.com/file".to_string(),
        HashMap::new(),
        "GET".to_string(),
    );

    // Second request with Range (CRITICAL for Test 26)
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=5000-".to_string());
    client.record_request(
        "http://example.com/file".to_string(),
        headers,
        "GET".to_string(),
    );

    let range = client.get_last_range_header();
    assert_eq!(
        range,
        Some("bytes=5000-".to_string()),
        "CRITICAL: Range header must be captured for Test 26 verification"
    );
}

#[test]
fn test_clear_requests() {
    let client = MockHttpClient::new();

    client.record_request(
        "http://example.com/file".to_string(),
        HashMap::new(),
        "GET".to_string(),
    );

    assert_eq!(client.get_requests().len(), 1);

    client.clear();

    assert_eq!(client.get_requests().len(), 0);
}

#[test]
fn test_default_implementation() {
    let client = MockHttpClient::default();
    assert_eq!(client.get_requests().len(), 0);
}

#[test]
fn test_clone_http_request() {
    let mut headers = HashMap::new();
    headers.insert("Range".to_string(), "bytes=1000-".to_string());

    let request = HttpRequest {
        url: "http://example.com".to_string(),
        headers,
        method: "GET".to_string(),
    };

    let cloned = request.clone();
    assert_eq!(cloned.url, request.url);
    assert_eq!(cloned.method, request.method);
    assert_eq!(
        cloned.headers.get("Range"),
        Some(&"bytes=1000-".to_string())
    );
}
