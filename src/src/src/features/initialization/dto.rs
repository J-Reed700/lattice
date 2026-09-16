//! # Initialization DTOs
//!
//! Data Transfer Objects for system initialization use cases.
//!
//! These DTOs handle first-run setup operations including:
//! - Model downloads and initialization
//! - Database schema creation
//!
//! ## Design Philosophy (Bricks and Studs)
//!
//! - **Minimal**: Only essential fields
//! - **Clear**: Descriptive names and comprehensive documentation
//! - **Stable**: Versioned schemas for breaking changes
//! - **Validated**: Strong typing with serde

use serde::{Deserialize, Serialize};

/// Response from initialize_models use case.
///
/// Contains the outcome of model initialization including download and setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeModelsResponseDto {
    /// Whether initialization was successful
    pub success: bool,

    /// Path to the initialized model
    pub model_path: String,

    /// Model name (e.g., DEFAULT_EMBEDDING_MODEL_NAME)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,

    /// Model dimension (e.g., DEFAULT_EMBEDDING_DIM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_dimension: Option<usize>,

    /// Whether the model was already present (true) or downloaded (false)
    #[serde(default)]
    pub was_cached: bool,
}

/// Response from initialize_database use case.
///
/// Contains the outcome of database schema creation and migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeDatabaseResponseDto {
    /// Whether initialization was successful
    pub success: bool,

    /// Path to the database file
    pub database_path: String,

    /// Schema version after initialization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i64>,

    /// Whether this was a first-time initialization (true) or existing database (false)
    #[serde(default)]
    pub is_new_database: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_models_response_serialization() {
        use crate::domain::embedding_constants::{
            DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME,
        };
        let response = InitializeModelsResponseDto {
            success: true,
            model_path: "/path/to/model.onnx".to_string(),
            model_name: Some(DEFAULT_EMBEDDING_MODEL_NAME.to_string()),
            model_dimension: Some(DEFAULT_EMBEDDING_DIM),
            was_cached: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: InitializeModelsResponseDto = serde_json::from_str(&json).unwrap();

        assert!(deserialized.success);
        assert_eq!(deserialized.model_path, "/path/to/model.onnx");
        assert_eq!(deserialized.model_dimension, Some(DEFAULT_EMBEDDING_DIM));
    }

    #[test]
    fn test_initialize_database_response_serialization() {
        let response = InitializeDatabaseResponseDto {
            success: true,
            database_path: "/path/to/lattice.db".to_string(),
            schema_version: Some(1),
            is_new_database: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: InitializeDatabaseResponseDto = serde_json::from_str(&json).unwrap();

        assert!(deserialized.success);
        assert_eq!(deserialized.database_path, "/path/to/lattice.db");
        assert_eq!(deserialized.schema_version, Some(1));
        assert!(deserialized.is_new_database);
    }
}
