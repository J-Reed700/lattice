//! Degraded Mock for EmbeddingPort
//!
//! Used when AI models are not installed to provide helpful error messages
//! instead of panicking or failing silently.

use crate::application::ports::EmbeddingPort;
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use async_trait::async_trait;

/// Mock EmbeddingPort that returns AiModelsNotInstalled errors
///
/// This mock is used when embedding models are not available, providing
/// a graceful degradation path for AI-dependent features.
pub struct MockEmbeddingPort {
    error_message: String,
}

impl MockEmbeddingPort {
    /// Create a degraded mock that returns helpful error messages
    pub fn new_degraded() -> Self {
        Self {
            error_message: "AI embedding models not installed".to_string(),
        }
    }
}

#[async_trait]
impl EmbeddingPort for MockEmbeddingPort {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Err(AppError::AiModelsNotInstalled(self.error_message.clone()))
    }

    async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Err(AppError::AiModelsNotInstalled(self.error_message.clone()))
    }

    fn dimension(&self) -> usize {
        DEFAULT_EMBEDDING_DIM // Return expected dimension even in degraded mode
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(false) // Degraded mock is never ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_degraded_mock_returns_error() {
        let mock = MockEmbeddingPort::new_degraded();

        let result = mock.embed_single("test").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AppError::AiModelsNotInstalled(_)
        ));
    }

    #[tokio::test]
    async fn test_degraded_mock_batch_returns_error() {
        let mock = MockEmbeddingPort::new_degraded();

        let result = mock
            .embed_batch(&["test1".to_string(), "test2".to_string()])
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AppError::AiModelsNotInstalled(_)
        ));
    }

    #[test]
    fn test_embedding_dim_returns_expected_value() {
        let mock = MockEmbeddingPort::new_degraded();
        assert_eq!(mock.dimension(), DEFAULT_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn test_is_ready_returns_false() {
        let mock = MockEmbeddingPort::new_degraded();
        let ready = mock.is_ready().await.unwrap();
        assert!(!ready, "Degraded mock should never be ready");
    }
}
