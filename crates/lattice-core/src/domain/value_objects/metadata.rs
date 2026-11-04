//! # Metadata Value Object
//!
//! Validated metadata JSON.

use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};

/// Validated metadata JSON
///
/// This value object ensures that metadata JSON:
/// 1. Is valid JSON (can be deserialized)
/// 2. Is immutable after validation
///
/// ## Invariants
///
/// - Valid JSON structure
/// - Non-empty string
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedMetadata {
    json: String,
}

impl ValidatedMetadata {
    /// Create validated metadata from JSON string
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if:
    /// - JSON is malformed (invalid syntax)
    /// - JSON is empty string
    pub fn new(json: String) -> Result<Self, AppError> {
        if json.is_empty() {
            return Err(AppError::InvalidInput(
                "Metadata JSON cannot be empty".to_string(),
            ));
        }

        serde_json::from_str::<serde_json::Value>(&json)
            .map_err(|e| AppError::InvalidInput(format!("Invalid JSON in metadata: {}", e)))?;

        Ok(Self { json })
    }

    /// Get the validated JSON string
    pub fn as_str(&self) -> &str {
        &self.json
    }

    /// Get the size in bytes
    pub fn size_bytes(&self) -> usize {
        self.json.len()
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
        assert!(validated.size_bytes() > 0);
    }

    #[test]
    fn metadata_of_any_size_is_valid() {
        let large_json = format!(r#"{{"data": "{}"}}"#, "x".repeat(200_000));

        let validated = ValidatedMetadata::new(large_json.clone()).unwrap();

        assert_eq!(validated.as_str(), large_json);
        assert_eq!(validated.size_bytes(), large_json.len());
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
    fn test_realistic_search_results() {
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
