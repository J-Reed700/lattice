//! First Run Setup Use Case
//!
//! Detects if this is the first run and if a default embedding model needs to be downloaded.
//!
//! # Purpose
//!
//! Checks if any .onnx embedding models exist in the models directory.
//! If none exist, recommends downloading the default model (DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME).
//!
//! # Business Logic
//!
//! 1. Check if models directory exists
//! 2. Search for .onnx files in models directory
//! 3. If no models found, return needs_setup=true with recommendation
//! 4. If models exist, return needs_setup=false
//!
//! # Example
//!
//! ```rust
//! let use_case = CheckFirstRunStatusUseCase::new(models_path);
//! let response = use_case.execute().await?;
//!
//! if response.needs_setup {
//!     println!("First run detected. Recommended model: {}", response.recommended_model_id.unwrap());
//! }
//! ```

use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

// =============================================================================
// DTOs
// =============================================================================

/// Response DTO for first-run status check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRunStatusResponse {
    /// Whether first-run setup is needed
    pub needs_setup: bool,
    /// Recommended model ID if setup is needed
    pub recommended_model_id: Option<String>,
    /// Recommended model name for UI display
    pub recommended_model_name: Option<String>,
    /// Estimated download size in bytes
    pub estimated_size_bytes: Option<u64>,
}

// =============================================================================
// Use Case
// =============================================================================

/// Check first-run status use case
pub struct CheckFirstRunStatusUseCase {
    /// Path to models directory
    models_path: PathBuf,
}

impl CheckFirstRunStatusUseCase {
    /// Create a new instance
    ///
    /// # Arguments
    ///
    /// * `models_path` - Path to models directory (e.g., ~/.recall/models)
    pub fn new(models_path: PathBuf) -> Self {
        Self { models_path }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Response indicating if setup is needed and recommended model
    ///
    /// # Errors
    ///
    /// Returns error if filesystem operations fail
    pub async fn execute(&self) -> Result<FirstRunStatusResponse> {
        debug!(
            path = %self.models_path.display(),
            "Checking first-run status"
        );

        // Check if models directory exists
        if !self.models_path.exists() {
            info!("Models directory does not exist - first run detected");
            return Ok(Self::needs_setup_response());
        }

        // Search for .onnx files
        let has_models = self.has_onnx_models().await?;

        if !has_models {
            info!("No .onnx models found - first run detected");
            Ok(Self::needs_setup_response())
        } else {
            debug!("Existing models found - not first run");
            Ok(FirstRunStatusResponse {
                needs_setup: false,
                recommended_model_id: None,
                recommended_model_name: None,
                estimated_size_bytes: None,
            })
        }
    }

    /// Check if directory contains any .onnx files
    async fn has_onnx_models(&self) -> Result<bool> {
        let mut entries = tokio::fs::read_dir(&self.models_path)
            .await
            .map_err(|e| AppError::FileSystem(format!("Failed to read models directory: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::FileSystem(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "onnx" {
                        debug!(file = %path.display(), "Found .onnx model");
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// Create response for first-run setup needed
    fn needs_setup_response() -> FirstRunStatusResponse {
        FirstRunStatusResponse {
            needs_setup: true,
            // Use curated model ID so it matches the model catalog and download flow.
            recommended_model_id: Some(DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.to_string()),
            recommended_model_name: Some(format!(
                "{} (Embedding Model)",
                DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
            )),
            estimated_size_bytes: Some(420_000_000),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_first_run_no_directory() {
        let temp_dir = TempDir::new().unwrap();
        let models_path = temp_dir.path().join("nonexistent");

        let use_case = CheckFirstRunStatusUseCase::new(models_path);
        let result = use_case.execute().await.unwrap();

        assert!(result.needs_setup);
        assert!(result.recommended_model_id.is_some());
        assert_eq!(
            result.recommended_model_id.unwrap(),
            DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
        );
    }

    #[tokio::test]
    async fn test_first_run_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let models_path = temp_dir.path().join("models");
        fs::create_dir(&models_path).await.unwrap();

        let use_case = CheckFirstRunStatusUseCase::new(models_path);
        let result = use_case.execute().await.unwrap();

        assert!(result.needs_setup);
        assert!(result.recommended_model_id.is_some());
    }

    #[tokio::test]
    async fn test_not_first_run_has_onnx() {
        let temp_dir = TempDir::new().unwrap();
        let models_path = temp_dir.path().join("models");
        fs::create_dir(&models_path).await.unwrap();

        // Create a dummy .onnx file
        let model_file = models_path.join("model.onnx");
        fs::write(&model_file, b"dummy onnx content").await.unwrap();

        let use_case = CheckFirstRunStatusUseCase::new(models_path);
        let result = use_case.execute().await.unwrap();

        assert!(!result.needs_setup);
        assert!(result.recommended_model_id.is_none());
    }

    #[tokio::test]
    async fn test_first_run_only_non_onnx_files() {
        let temp_dir = TempDir::new().unwrap();
        let models_path = temp_dir.path().join("models");
        fs::create_dir(&models_path).await.unwrap();

        // Create non-.onnx files
        fs::write(models_path.join("config.json"), b"{}")
            .await
            .unwrap();
        fs::write(models_path.join("tokenizer.json"), b"{}")
            .await
            .unwrap();

        let use_case = CheckFirstRunStatusUseCase::new(models_path);
        let result = use_case.execute().await.unwrap();

        assert!(result.needs_setup);
        assert!(result.recommended_model_id.is_some());
    }
}
