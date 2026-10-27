//! A loaded model pinned for one turn or job.

use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;

use super::{Holder, LoadedLlm};
use crate::infrastructure::ml::instance_pool::Lease;

/// The client a turn or job runs on. It answers exactly as the loaded model
/// does; holding it — or any clone of it, such as one handed to a sub-task —
/// keeps the model counted as in flight, so admission never unloads it, and
/// keeps its holder listed in `list_loaded_models`. Dropping the last clone
/// releases the pin.
pub(crate) struct PinnedLlm {
    loaded: Arc<LoadedLlm>,
    _lease: Lease<Holder>,
}

impl PinnedLlm {
    pub(super) fn new(loaded: Arc<LoadedLlm>, lease: Lease<Holder>) -> Self {
        Self {
            loaded,
            _lease: lease,
        }
    }

    fn inner(&self) -> &dyn LLMPort {
        self.loaded.llm.as_ref()
    }
}

/// Every method forwards, provided ones included: a method left to its default
/// here would bypass the loaded client's own version of it.
#[async_trait]
impl LLMPort for PinnedLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.inner().complete(request).await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.inner().complete_with_progress(request, on_text).await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.inner()
            .complete_with_retry_progress(request, on_text, on_retry)
            .await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.inner()
            .complete_with_reasoning_progress(request, on_text, on_reasoning, on_retry)
            .await
    }

    fn model_name(&self) -> &str {
        self.inner().model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner().max_context_tokens()
    }

    fn chars_per_token(&self) -> f64 {
        self.inner().chars_per_token()
    }

    fn pace(&self) -> Option<lattice_core::application::ports::llm_port::Pace> {
        self.inner().pace()
    }

    fn requests(&self) -> Option<lattice_core::application::ports::llm_port::RequestLoad> {
        self.inner().requests()
    }

    fn hold_cache(
        &self,
        key: &str,
    ) -> Option<lattice_core::application::ports::llm_port::CacheHold> {
        self.inner().hold_cache(key)
    }

    fn count_tokens(&self, text: &str) -> usize {
        self.inner().count_tokens(text)
    }

    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.inner().count_tokens_exact(text).await
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.inner().counts_tokens_exactly()
    }

    async fn is_ready(&self) -> Result<bool> {
        self.inner().is_ready().await
    }

    fn supports_tool_calling(&self) -> bool {
        self.inner().supports_tool_calling()
    }

    fn thinks_before_answering(&self) -> bool {
        self.inner().thinks_before_answering()
    }

    fn vision(&self) -> Option<lattice_core::domain::models::vision::VisionGeometry> {
        self.inner().vision()
    }

    fn provider_name(&self) -> &str {
        self.inner().provider_name()
    }

    fn is_alive(&self) -> bool {
        self.inner().is_alive()
    }
}
