use async_trait::async_trait;
use lattice_core::shared::error::AppError;

#[async_trait]
pub trait ModelManagerTrait: Send + Sync {
    /// Ensure embedding model is available, downloading if needed.
    async fn ensure_model_available(&self) -> Result<std::path::PathBuf, AppError>;

    /// Check if embedding model is ready to use.
    async fn is_model_ready(&self) -> bool;

    /// Get path to embedding model file.
    fn get_model_path(&self) -> std::path::PathBuf;

    /// Get path to embedding model tokenizer file.
    fn get_tokenizer_path(&self) -> std::path::PathBuf;

    /// Ensure reranker model is available, downloading if needed.
    async fn ensure_reranker_available(&self) -> Result<std::path::PathBuf, AppError>;

    /// Check if reranker model is ready to use.
    async fn is_reranker_ready(&self) -> bool;

    /// Get path to reranker model file.
    fn get_reranker_path(&self) -> std::path::PathBuf;

    /// Get path to reranker tokenizer file.
    fn get_reranker_tokenizer_path(&self) -> std::path::PathBuf;

    /// Cancel ongoing download.
    fn cancel_download(&self);

    /// Get download progress fraction (0.0 to 1.0), or `None` if not downloading.
    fn get_download_progress(&self) -> Option<f32>;

    /// Get model information and status.
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
