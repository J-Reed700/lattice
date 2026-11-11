//! The embedding port used while no embedding model is loaded.

use crate::application::ports::EmbeddingPort;
use crate::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;

/// Why no embedding model is available.
#[derive(Debug, Clone)]
pub enum EmbeddingUnavailable {
    /// No embedding model is installed or active.
    NotInstalled,
    /// An embedding model is active but loading it failed; carries the loader's error.
    FailedToLoad(AppError),
}

/// An [`EmbeddingPort`] that is never ready and fails every embedding with its reason.
#[derive(Debug, Clone)]
pub struct UnavailableEmbedding {
    reason: EmbeddingUnavailable,
}

impl UnavailableEmbedding {
    pub fn new(reason: EmbeddingUnavailable) -> Self {
        Self { reason }
    }

    pub fn not_installed() -> Self {
        Self::new(EmbeddingUnavailable::NotInstalled)
    }

    pub fn failed_to_load(error: AppError) -> Self {
        Self::new(EmbeddingUnavailable::FailedToLoad(error))
    }

    fn error(&self) -> AppError {
        match &self.reason {
            EmbeddingUnavailable::NotInstalled => {
                AppError::AiModelsNotInstalled("No embedding model is installed".to_string())
            }
            EmbeddingUnavailable::FailedToLoad(error) => error.clone(),
        }
    }
}

#[async_trait]
impl EmbeddingPort for UnavailableEmbedding {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Err(self.error())
    }

    async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Err(self.error())
    }

    fn load_failure(&self) -> Option<AppError> {
        match &self.reason {
            EmbeddingUnavailable::NotInstalled => None,
            EmbeddingUnavailable::FailedToLoad(error) => Some(error.clone()),
        }
    }

    fn model_identity(&self) -> String {
        "unavailable".to_owned()
    }

    /// No model exists to have a dimension; no caller sizes an index from an unavailable port.
    fn dimension(&self) -> usize {
        DEFAULT_EMBEDDING_DIM
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests;
