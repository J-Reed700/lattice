//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::shared::error::AppError;

#[async_trait]
pub trait ModelManagerTrait: Send + Sync {
    /// Ensure embedding model is available (download if needed)
    ///
    /// # Returns
    /// Path to model file
    ///
    /// # Errors
    /// - `AppError::Network` if download fails
    /// - `AppError::Other` if disk operations fail
    async fn ensure_model_available(&self) -> Result<std::path::PathBuf, AppError>;

    /// Check if embedding model is ready to use
    ///
    /// # Returns
    /// `true` if model and tokenizer files exist
    async fn is_model_ready(&self) -> bool;

    /// Get path to embedding model file
    ///
    /// # Returns
    /// Path to model.onnx file (may not exist yet)
    fn get_model_path(&self) -> std::path::PathBuf;

    /// Get path to embedding model tokenizer file
    ///
    /// # Returns
    /// Path to tokenizer.json file (may not exist yet)
    fn get_tokenizer_path(&self) -> std::path::PathBuf;

    /// Ensure reranker model is available (download if needed)
    ///
    /// # Returns
    /// Path to reranker model file
    ///
    /// # Errors
    /// - `AppError::Network` if download fails
    /// - `AppError::Other` if disk operations fail
    async fn ensure_reranker_available(&self) -> Result<std::path::PathBuf, AppError>;

    /// Check if reranker model is ready to use
    ///
    /// # Returns
    /// `true` if reranker model and tokenizer files exist
    async fn is_reranker_ready(&self) -> bool;

    /// Get path to reranker model file
    ///
    /// # Returns
    /// Path to reranker/model.onnx file (may not exist yet)
    fn get_reranker_path(&self) -> std::path::PathBuf;

    /// Get path to reranker tokenizer file
    ///
    /// # Returns
    /// Path to reranker/tokenizer.json file (may not exist yet)
    fn get_reranker_tokenizer_path(&self) -> std::path::PathBuf;

    /// Cancel ongoing download
    ///
    /// Safe to call even if no download is in progress.
    fn cancel_download(&self);

    /// Get download progress (0.0 to 1.0)
    ///
    /// # Returns
    /// Progress fraction, or `None` if not downloading
    fn get_download_progress(&self) -> Option<f32>;

    /// Get model info
    ///
    /// # Returns
    /// Model name, version, and download status
    fn get_model_info(&self) -> ModelInfo;
}

/// Model information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub version: String,
    pub size_bytes: Option<i64>,
    pub is_downloaded: bool,
}
