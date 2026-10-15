//! The judge's model, teaching the judge's pace from every call it answers.
//!
//! The judge's calls are made inside the shared claim checker, which knows
//! nothing of paces; wrapping the port is how each answered call's prefill
//! timing reaches the pace without the checker carrying it.

use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::prompt_pace::PromptPace;
use lattice_core::shared::error::Result;

pub(super) struct PacedLlm {
    inner: Arc<dyn LLMPort>,
    pace: Arc<PromptPace>,
}

impl PacedLlm {
    pub(super) fn new(inner: Arc<dyn LLMPort>, pace: Arc<PromptPace>) -> Self {
        Self { inner, pace }
    }

    fn sent_tokens(&self, request: &CompletionRequest) -> usize {
        request
            .input
            .iter()
            .map(|item| match item {
                CompletionInput::Message { content, .. } => self.inner.count_tokens(content),
                _ => 0,
            })
            .sum()
    }
}

#[async_trait]
impl LLMPort for PacedLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let response = self.inner.complete(request).await?;
        self.pace.answered(
            self.inner.model_name(),
            response.prefill,
            self.sent_tokens(request),
        );
        Ok(response)
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }

    fn chars_per_token(&self) -> f64 {
        self.inner.chars_per_token()
    }

    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }

    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.inner.count_tokens_exact(text).await
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.inner.counts_tokens_exactly()
    }

    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }

    fn supports_tool_calling(&self) -> bool {
        self.inner.supports_tool_calling()
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
}
