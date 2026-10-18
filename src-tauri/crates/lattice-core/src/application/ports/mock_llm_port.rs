//! Degraded LLMPort
//!
//! Stands in where no language model is wired: it answers every completion
//! with `AiModelsNotInstalled` and reports itself not ready, so a caller can
//! tell "no model here" from a model that failed.

use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::application::ports::LLMPort;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;

/// An LLMPort with no model behind it, beside
/// [`MockEmbeddingPort::new_degraded`](crate::application::ports::MockEmbeddingPort::new_degraded).
#[derive(Debug, Default, Clone, Copy)]
pub struct DegradedLLMPort;

#[async_trait]
impl LLMPort for DegradedLLMPort {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Err(AppError::AiModelsNotInstalled(
            "No language model is available here".to_string(),
        ))
    }

    fn model_name(&self) -> &str {
        "unavailable"
    }

    fn max_context_tokens(&self) -> usize {
        4096
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn degraded_llm_is_not_ready_and_refuses_completions() {
        let llm = DegradedLLMPort;
        assert!(!llm.is_ready().await.unwrap());
        let result = llm.complete(&CompletionRequest::default()).await;
        assert!(matches!(result, Err(AppError::AiModelsNotInstalled(_))));
    }
}
