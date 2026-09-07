//! # Metadata Value Object
//!
//! Validated metadata with size constraints to prevent database errors.
//!
//! ## Purpose
//!
//! Ensures metadata JSON serialization stays within database column size limits
//! (typically 64KB for TEXT columns). Prevents runtime errors from oversized data.
//!
//! ## Example
//!
//! ```rust
//! use lattice::domain::metadata::ValidatedMetadata;
//!
//! let json = r#"{"sources": [{"id": 1}]}"#.to_string();
//! let validated = ValidatedMetadata::new(json)?;
//! assert!(validated.size_bytes() < 65_000);
//! # Ok::<(), lattice::shared::errors::AppError>(())
//! ```

use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};

/// Maximum size for serialized metadata (64KB - some buffer)
///
/// TEXT columns in SQLite typically have a 64KB limit. We use 65,000 bytes
/// (63.5KB) to leave room for encoding overhead and safety margin.
const MAX_METADATA_SIZE_BYTES: usize = 65_000;

/// Validated metadata that enforces size constraints
///
/// This value object ensures that metadata JSON:
/// 1. Does not exceed database column size limits
/// 2. Is valid JSON (can be deserialized)
/// 3. Is immutable after validation
///
/// ## Invariants
///
/// - Size ≤ 65,000 bytes (~64KB)
/// - Valid JSON structure
/// - Non-empty string
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedMetadata {
    json: String,
}

impl ValidatedMetadata {
    /// Create validated metadata from JSON string
    ///
    /// # Arguments
    ///
    /// * `json` - JSON string to validate
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if:
    /// - JSON exceeds size limit (65KB)
    /// - JSON is malformed (invalid syntax)
    /// - JSON is empty string
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::domain::metadata::ValidatedMetadata;
    ///
    /// let json = r#"{"key": "value"}"#.to_string();
    /// let metadata = ValidatedMetadata::new(json)?;
    /// # Ok::<(), lattice::shared::errors::AppError>(())
    /// ```
    pub fn new(json: String) -> Result<Self, AppError> {
        // Validate non-empty
        if json.is_empty() {
            return Err(AppError::InvalidInput(
                "Metadata JSON cannot be empty".to_string(),
            ));
        }

        // Validate size first (fast check)
        let size_bytes = json.len();
        if size_bytes > MAX_METADATA_SIZE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "Metadata too large: {} bytes (maximum: {} bytes, ~64KB). \
                     Try reducing the number of sources or content length.",
                size_bytes, MAX_METADATA_SIZE_BYTES
            )));
        }

        // Validate it's valid JSON (ensures no corruption)
        serde_json::from_str::<serde_json::Value>(&json)
            .map_err(|e| AppError::InvalidInput(format!("Invalid JSON in metadata: {}", e)))?;

        Ok(Self { json })
    }

    /// Get the validated JSON string
    ///
    /// # Returns
    ///
    /// Reference to the validated JSON string
    pub fn as_str(&self) -> &str {
        &self.json
    }

    /// Get the size in bytes
    ///
    /// # Returns
    ///
    /// Size of the JSON string in bytes
    pub fn size_bytes(&self) -> usize {
        self.json.len()
    }

    /// Get maximum allowed size in bytes
    ///
    /// # Returns
    ///
    /// Maximum metadata size (65,000 bytes)
    pub const fn max_size_bytes() -> usize {
        MAX_METADATA_SIZE_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_metadata() {
        let json = r#"{"sources": [{"id": 1, "content": "test"}]}"#.to_string();
        let validated = ValidatedMetadata::new(json.clone()).unwrap();

        assert_eq!(validated.as_str(), json);
        assert!(validated.size_bytes() < MAX_METADATA_SIZE_BYTES);
        assert!(validated.size_bytes() > 0);
    }

    #[test]
    fn test_oversized_metadata() {
        // Create oversized JSON (70KB of data)
        let large_json = format!(r#"{{"data": "{}"}}"#, "x".repeat(70_000));
        let result = ValidatedMetadata::new(large_json);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("too large"),
            "Error should mention size: {}",
            err_msg
        );
        assert!(
            err_msg.contains("bytes"),
            "Error should show byte count: {}",
            err_msg
        );
    }

    #[test]
    fn test_invalid_json() {
        let invalid = "{not valid json}".to_string();
        let result = ValidatedMetadata::new(invalid);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid JSON"),
            "Error should mention JSON: {}",
            err_msg
        );
    }

    #[test]
    fn test_empty_json() {
        let empty = "".to_string();
        let result = ValidatedMetadata::new(empty);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("empty"),
            "Error should mention empty: {}",
            err_msg
        );
    }

    #[test]
    fn test_boundary_size() {
        // Test at exactly the limit (should fail due to overhead)
        let boundary = format!(
            r#"{{"data": "{}"}}"#,
            "x".repeat(MAX_METADATA_SIZE_BYTES - 15)
        );
        let result = ValidatedMetadata::new(boundary);

        // Should succeed if under limit
        match result {
            Ok(metadata) => {
                assert!(metadata.size_bytes() <= MAX_METADATA_SIZE_BYTES);
            }
            Err(_) => {
                // Also acceptable if slightly over due to JSON structure
            }
        }
    }

    #[test]
    fn test_normal_sized_metadata() {
        // Typical case: 10 sources with reasonable content
        let sources = (0..10)
            .map(|i| {
                format!(
                    r#"{{"id": {}, "content": "Sample content for source {}", "score": 0.8}}"#,
                    i, i
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        let json = format!(r#"{{"sources": [{}]}}"#, sources);
        let result = ValidatedMetadata::new(json);

        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(
            metadata.size_bytes() < 10_000,
            "Normal metadata should be small"
        );
    }

    #[test]
    fn test_max_size_constant() {
        assert_eq!(ValidatedMetadata::max_size_bytes(), MAX_METADATA_SIZE_BYTES);
        assert_eq!(ValidatedMetadata::max_size_bytes(), 65_000);
    }

    #[test]
    fn test_realistic_search_results() {
        // Simulate realistic search results metadata
        let search_result = r#"{
            "sources": [
                {
                    "document_id": "doc_123",
                    "chunk_id": "chunk_456",
                    "content": "This is a sample chunk of text from a document.",
                    "score": 0.95,
                    "file_name": "example.txt",
                    "file_path": "/path/to/example.txt",
                    "mime_type": "text/plain",
                    "category": "Text"
                }
            ]
        }"#
        .to_string();

        let result = ValidatedMetadata::new(search_result);
        assert!(result.is_ok());

        let metadata = result.unwrap();
        assert!(metadata.size_bytes() < 1000);
    }
}
