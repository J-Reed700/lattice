//! Application-wide constants.
//!
//! This module contains constants that are used across multiple layers
//! of the application. These are truly shared values that don't belong
//! to any specific domain or infrastructure layer.

use std::time::Duration;

/// Default timeout for operations in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Database query timeouts
pub const DB_QUERY_TIMEOUT_QUICK: Duration = Duration::from_secs(5);
pub const DB_QUERY_TIMEOUT_NORMAL: Duration = Duration::from_secs(30);
pub const DB_QUERY_TIMEOUT_HEAVY: Duration = Duration::from_secs(60);

/// HTTP request timeout for web integrations
pub const WEB_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// LLM inference timeout
pub const LLM_INFERENCE_TIMEOUT: Duration = Duration::from_secs(60);

/// ONNX inference timeout
pub const ONNX_INFERENCE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum file size for processing (100 MB in bytes)
pub const MAX_FILE_SIZE_BYTES: u64 = 100 * 1024 * 1024;

/// Default batch size for processing operations
pub const DEFAULT_BATCH_SIZE: usize = 100;

/// Maximum batch size for processing operations
pub const MAX_BATCH_SIZE: usize = 1000;

// Re-export embedding defaults from domain layer.
pub use crate::domain::embedding_constants::{
    DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, DEFAULT_EMBEDDING_MODEL_NAME,
};

/// Application version (from Cargo.toml)
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

/// Minimum similarity score for search results (0.0 to 1.0)
pub const MIN_SIMILARITY_SCORE: f32 = 0.3;

/// Default number of search results to return
pub const DEFAULT_SEARCH_LIMIT: usize = 10;

/// Maximum number of search results to return
pub const MAX_SEARCH_LIMIT: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_validity() {
        assert!(MAX_FILE_SIZE_BYTES > 0);
        assert!(DEFAULT_BATCH_SIZE > 0);
        assert!(DEFAULT_BATCH_SIZE <= MAX_BATCH_SIZE);
        assert!(MIN_SIMILARITY_SCORE >= 0.0 && MIN_SIMILARITY_SCORE <= 1.0);
        assert!(DEFAULT_SEARCH_LIMIT <= MAX_SEARCH_LIMIT);
    }
}
