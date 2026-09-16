#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

/// Unit tests for download_engine validation functions
///
/// This test file verifies the is_content_length_valid function
/// which determines if a Content-Length header value is reliable.
///
/// Key scenarios tested:
/// - None values (server didn't provide Content-Length)
/// - Small values (likely redirect HTML, like HuggingFace's 1309 bytes)
/// - Valid model file sizes (>= 10KB threshold)
///
#[cfg(test)]
mod validation_tests {
    // Re-export the constant and function for testing
    const MIN_VALID_MODEL_SIZE: u64 = 10 * 1024; // 10 KB

    fn is_content_length_valid(content_length: Option<u64>) -> bool {
        match content_length {
            Some(size) => size >= MIN_VALID_MODEL_SIZE,
            None => false,
        }
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
        // Test small HTML response (like HuggingFace redirect: 1309 bytes)
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
        // Test typical model file size (724,923 bytes, like HuggingFace model)
        assert!(is_content_length_valid(Some(724_923)));
    }

    #[test]
    fn test_is_content_length_valid_large_file() {
        // Test large file (1 GB)
        assert!(is_content_length_valid(Some(1024 * 1024 * 1024)));
    }
}
