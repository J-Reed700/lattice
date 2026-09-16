//! # Mock HTTP Client for Download Testing
//!
//! Mock HTTP client for testing download operations,
//! with CRITICAL support for Range header verification (Test 26).
//!
//! ## Key Features
//!
//! - Thread-safe request/response mocking
//! - Request capture for verification
//! - Range header extraction (CRITICAL for resumption testing)
//! - Response configuration per URL
//!
//! ## Usage
//!
//! ```rust
//! let mock = MockHttpClient::new();
//!
//! // Configure response
//! mock.set_response("http://example.com/file", MockHttpResponse {
//!     status: 200,
//!     body: vec![1, 2, 3],
//!     content_length: Some(3),
//!     headers: HashMap::new(),
//! });
//!
//! // Verify Range header (CRITICAL for Test 26)
//! let range = mock.get_last_range_header();
//! assert_eq!(range, Some("bytes=5000-".to_string()));
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock HTTP client for testing download operations
pub struct MockHttpClient {
    responses: Arc<Mutex<HashMap<String, MockHttpResponse>>>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

/// Mock HTTP response configuration
#[allow(dead_code)] // Fields will be used when HTTP client integration is implemented
pub struct MockHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_length: Option<u64>,
    pub headers: HashMap<String, String>,
}

/// Captured HTTP request
#[derive(Clone)]
pub struct HttpRequest {
    pub url: String,
    pub headers: HashMap<String, String>,
    pub method: String,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Configure mock response for a URL
    pub fn set_response(&self, url: &str, response: MockHttpResponse) {
        self.responses
            .lock()
            .unwrap()
            .insert(url.to_string(), response);
    }

    /// Get all captured requests
    pub fn get_requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Get most recent request
    pub fn get_last_request(&self) -> Option<HttpRequest> {
        self.requests.lock().unwrap().last().cloned()
    }

    /// Get Range header from last request (CRITICAL for Test 26)
    pub fn get_last_range_header(&self) -> Option<String> {
        self.get_last_request()
            .and_then(|req| req.headers.get("Range").cloned())
    }

    /// Clear all captured requests and responses
    pub fn clear(&self) {
        self.requests.lock().unwrap().clear();
        self.responses.lock().unwrap().clear();
    }

    /// Record an HTTP request (used by test framework)
    pub fn record_request(&self, url: String, headers: HashMap<String, String>, method: String) {
        self.requests.lock().unwrap().push(HttpRequest {
            url,
            headers,
            method,
        });
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

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

        let responses = client.responses.lock().unwrap();
        assert!(responses.contains_key("http://example.com/file"));
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
    fn test_get_last_range_header() {
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
        assert_eq!(range, Some("bytes=5000-".to_string()));
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
}
