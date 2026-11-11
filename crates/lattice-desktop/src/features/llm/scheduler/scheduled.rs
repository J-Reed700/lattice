//! The decorator that puts a backend's scheduler in front of a port.
//!
//! Every method that reaches the model is admitted first, at the priority its
//! request carries. The same decorator owns the backend's calibrated token estimate, because it is
//! the one place that sees both what was sent and the usage reported back, and
//! the backend's pace, measured from the timing it reports on those requests.
//!
//! Before admission, a request whose prompt alone fills the backend's window
//! is refused, whichever backend is behind it: nothing that cannot fit waits
//! for a slot or reaches a server. Any other request is fitted ([`fit`]) and
//! sent with the room its prompt leaves, which the backend's reply ceiling is
//! cut to.

use std::sync::Arc;

use async_trait::async_trait;

use super::{
    cancelled_error, AdmissionRequest, InferenceScheduler, PaceEstimator, ServerTokenizer,
    TokenEstimator,
};
use crate::features::llm::backend::{ModelBackend, Progress};
use crate::features::llm::fitting::{fit, prompt_chars, TokenCounter};
use lattice_core::application::ports::llm_port::{
    CacheHold, CompletionInput, CompletionRequest, CompletionResponse, Pace, RequestLoad,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::current_rank;

/// A port whose every model call goes through one backend's scheduler.
pub(crate) struct ScheduledLlm {
    inner: Arc<dyn ModelBackend>,
    scheduler: Arc<InferenceScheduler>,
    estimator: TokenEstimator,
    pace: PaceEstimator,
    tokenizer: Option<ServerTokenizer>,
}

impl ScheduledLlm {
    pub fn new(inner: Arc<dyn ModelBackend>, scheduler: Arc<InferenceScheduler>) -> Self {
        Self {
            inner,
            scheduler,
            estimator: TokenEstimator::default(),
            pace: PaceEstimator::default(),
            tokenizer: None,
        }
    }

    /// Count exactly with this llama-server's tokenizer.
    pub fn with_tokenizer(mut self, tokenizer: ServerTokenizer) -> Self {
        self.tokenizer = Some(tokenizer);
        self
    }

    /// Fit the request, wait for a slot, pin the request to it when the
    /// backend pins, and send it.
    async fn send(
        &self,
        request: &CompletionRequest,
        progress: Progress<'_>,
    ) -> Result<CompletionResponse> {
        let fitted = fit(self.inner.as_ref(), self, request).await?;
        let permit = self
            .scheduler
            .admit(
                AdmissionRequest {
                    priority: request.priority,
                    tokens: fitted.reservation(),
                    cache_key: request.cache_key.clone(),
                    rank: current_rank(),
                },
                request.cancel.as_ref(),
            )
            .await?;
        let fitted = fitted.in_slot(permit.slot());
        let response = self
            .run(request, self.inner.send(&fitted, progress))
            .await?;
        self.observe(request, &response);
        Ok(response)
    }

    /// Run an admitted call, aborting it when the request is cancelled.
    /// Dropping the provider's future closes its connection, which is what
    /// stops the server generating.
    async fn run<T>(
        &self,
        request: &CompletionRequest,
        call: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        match &request.cancel {
            Some(token) => tokio::select! {
                biased;
                _ = token.cancelled() => Err(cancelled_error()),
                result = call => result,
            },
            None => call.await,
        }
    }

    /// Calibrate the estimate from what the backend says it read. Opaque
    /// provider state (signed reasoning) and pictures are not text a tokenizer
    /// read character by character, so a request carrying them teaches nothing.
    /// Pictures are encoded, not read, at a pace of their own, so a request
    /// carrying them leaves the pace alone too.
    fn observe(&self, request: &CompletionRequest, response: &CompletionResponse) {
        if let Some(timing) = response.timing.filter(|_| !request.has_images()) {
            self.pace.observe(&timing);
        }
        if response.input_tokens == 0
            || request.has_images()
            || request
                .input
                .iter()
                .any(|item| matches!(item, CompletionInput::Native { .. }))
        {
            return;
        }
        self.estimator
            .observe(prompt_chars(request), response.input_tokens);
    }
}

#[async_trait]
impl TokenCounter for ScheduledLlm {
    fn chars_per_token(&self) -> f64 {
        self.estimator.chars_per_token()
    }

    /// The tokenizer's count, when there is one: without it `count_tokens_exact`
    /// is only the estimate.
    async fn exact(&self, text: &str) -> Option<usize> {
        self.tokenizer.as_ref()?;
        match self.count_tokens_exact(text).await {
            Ok(exact) => Some(exact),
            Err(error) => {
                tracing::debug!(%error, "The server could not count the prompt; keeping the estimate");
                None
            }
        }
    }
}

#[async_trait]
impl LLMPort for ScheduledLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.send(request, Progress::None).await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let progress = Progress::Streamed {
            on_text,
            on_reasoning: None,
            on_retry: None,
        };
        self.send(request, progress).await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let progress = Progress::Streamed {
            on_text,
            on_reasoning: None,
            on_retry: Some(on_retry),
        };
        self.send(request, progress).await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let progress = Progress::Streamed {
            on_text,
            on_reasoning: Some(on_reasoning),
            on_retry: Some(on_retry),
        };
        self.send(request, progress).await
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.window()
    }

    fn chars_per_token(&self) -> f64 {
        self.estimator.chars_per_token()
    }

    fn pace(&self) -> Option<Pace> {
        self.pace.pace()
    }

    fn requests(&self) -> Option<RequestLoad> {
        Some(self.scheduler.load())
    }

    fn hold_cache(&self, key: &str) -> Option<CacheHold> {
        self.scheduler
            .capacity()
            .pins_slots
            .then(|| CacheHold::new(self.scheduler.hold(key)))
    }

    fn keeps_read_prompts(&self) -> bool {
        self.inner.keeps_read_prompts()
    }

    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        let Some(tokenizer) = &self.tokenizer else {
            return Ok(self.count_tokens(text));
        };
        let tokens = tokenizer.count(text).await?;
        self.estimator.observe(
            text.chars().count(),
            u64::try_from(tokens).unwrap_or(u64::MAX),
        );
        Ok(tokens)
    }

    fn counts_tokens_exactly(&self) -> bool {
        self.tokenizer.is_some()
    }

    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }

    fn supports_tool_calling(&self) -> bool {
        self.inner.supports_tool_calling()
    }

    fn thinks_before_answering(&self) -> bool {
        self.inner.thinking(&CompletionRequest::default()).is_on()
    }

    fn vision(&self) -> Option<VisionGeometry> {
        self.inner.vision()
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
}
