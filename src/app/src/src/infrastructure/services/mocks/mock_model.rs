//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::infrastructure::services::traits::*;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock model manager for testing
///
/// Allows configuring model readiness state and download progress.
/// All operations are deterministic and fast (no I/O).
pub struct MockModelManager {
    model_ready: Arc<RwLock<bool>>,
    reranker_ready: Arc<RwLock<bool>>,
    download_progress: Arc<RwLock<Option<f32>>>,
    model_dir: std::path::PathBuf,
}

#[cfg(test)]
impl MockModelManager {
    /// Create new mock with default state (models ready)
    pub fn new() -> Self {
        Self {
            model_ready: Arc::new(RwLock::new(true)),
            reranker_ready: Arc::new(RwLock::new(true)),
            download_progress: Arc::new(RwLock::new(Some(1.0))),
            model_dir: std::path::PathBuf::from("/mock/models"),
        }
    }

    /// Create mock with custom model directory
    pub fn with_model_dir(model_dir: std::path::PathBuf) -> Self {
        Self {
            model_ready: Arc::new(RwLock::new(true)),
            reranker_ready: Arc::new(RwLock::new(true)),
            download_progress: Arc::new(RwLock::new(Some(1.0))),
            model_dir,
        }
    }

    /// Set main model readiness state
    pub fn set_model_ready(&self, ready: bool) {
        *self.model_ready.write().unwrap() = ready;
    }

    /// Set reranker model readiness state
    pub fn set_reranker_ready(&self, ready: bool) {
        *self.reranker_ready.write().unwrap() = ready;
    }

    /// Set download progress (0.0 to 1.0, or None if not downloading)
    pub fn set_download_progress(&self, progress: Option<f32>) {
        *self.download_progress.write().unwrap() = progress;
    }
}

#[cfg(test)]
impl Default for MockModelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl ModelManagerTrait for MockModelManager {
    async fn ensure_model_available(&self) -> Result<std::path::PathBuf> {
        // Automatically mark as ready when ensured
        self.set_model_ready(true);
        Ok(self.get_model_path())
    }

    async fn is_model_ready(&self) -> bool {
        *self.model_ready.read().unwrap()
    }

    fn get_model_path(&self) -> std::path::PathBuf {
        self.model_dir.join("model.onnx")
    }

    fn get_tokenizer_path(&self) -> std::path::PathBuf {
        self.model_dir.join("tokenizer.json")
    }

    async fn ensure_reranker_available(&self) -> Result<std::path::PathBuf> {
        // Automatically mark as ready when ensured
        self.set_reranker_ready(true);
        Ok(self.get_reranker_path())
    }

    async fn is_reranker_ready(&self) -> bool {
        *self.reranker_ready.read().unwrap()
    }

    fn get_reranker_path(&self) -> std::path::PathBuf {
        self.model_dir.join("reranker/model.onnx")
    }

    fn get_reranker_tokenizer_path(&self) -> std::path::PathBuf {
        self.model_dir.join("reranker/tokenizer.json")
    }

    fn cancel_download(&self) {
        // Reset progress to indicate cancellation
        self.set_download_progress(None);
    }

    fn get_download_progress(&self) -> Option<f32> {
        *self.download_progress.read().unwrap()
    }

    fn get_model_info(&self) -> ModelInfo {
        ModelInfo {
            name: "mock-model".to_string(),
            version: "1.0.0".to_string(),
            size_bytes: Some(1024),
            is_downloaded: *self.model_ready.read().unwrap(),
        }
    }
}
