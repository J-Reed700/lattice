//! A model that waits out a temporary outage instead of failing the work.
//! Outline generation and repair run as one request with progress and a stop
//! button, so a 503 while loading, a rate limit, a dropped connection or a
//! timeout is retried call by call with a backoff growing to five minutes,
//! and every completed call stands. Any other error fails the call.

use std::future::Future;
use std::time::Duration;

use crate::features::learning::outline_progress::OutlineProgress;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{Backoff, JobFailure};

/// Where a wait is reported, and how it learns that the work was cancelled.
pub(in crate::features::learning) trait WaitObserver:
    Send + Sync
{
    /// The next attempt is `seconds` away.
    fn waiting(&self, seconds: u64);
    fn check_cancelled(&self) -> Result<()>;
}

impl WaitObserver for OutlineProgress {
    fn waiting(&self, seconds: u64) {
        self.detail(format!(
            "Waiting for the model server (retrying in {seconds}s)"
        ));
    }
    fn check_cancelled(&self) -> Result<()> {
        OutlineProgress::check_cancelled(self)
    }
}

/// Run `attempt` until it succeeds or fails with a non-transient error,
/// waiting `backoff` between transient failures. `attempt` receives the
/// number of retries so far.
pub(in crate::features::learning) async fn patiently<T, F, Fut>(
    observer: &dyn WaitObserver,
    backoff: Backoff,
    mut attempt: F,
) -> Result<T>
where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut retries = 0usize;
    loop {
        match attempt(retries).await {
            Err(error) if JobFailure::is_transient(&error) => {
                let delay = backoff.delay(u32::try_from(retries).unwrap_or(u32::MAX));
                tracing::warn!(%error, retries, delay_seconds = delay.as_secs(), "The model server is temporarily unavailable; waiting to retry");
                let until = tokio::time::Instant::now() + delay;
                loop {
                    observer.check_cancelled()?;
                    let left = until.saturating_duration_since(tokio::time::Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    observer.waiting(left.as_secs_f64().ceil() as u64);
                    tokio::time::sleep(left.min(Duration::from_secs(1))).await;
                }
                retries += 1;
            }
            result => return result,
        }
    }
}

/// `inner`, retrying each request through a temporary outage.
pub(in crate::features::learning) struct PatientModel<'a> {
    inner: &'a dyn LLMPort,
    observer: &'a dyn WaitObserver,
    backoff: Backoff,
}

impl<'a> PatientModel<'a> {
    pub(in crate::features::learning) fn new(
        inner: &'a dyn LLMPort,
        observer: &'a dyn WaitObserver,
    ) -> Self {
        Self {
            inner,
            observer,
            backoff: Backoff::default(),
        }
    }

    #[cfg(test)]
    pub(in crate::features::learning) fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }
}

#[async_trait::async_trait]
impl LLMPort for PatientModel<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        patiently(self.observer, self.backoff, |_| {
            self.inner.complete(request)
        })
        .await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        patiently(self.observer, self.backoff, |_| {
            self.inner.complete_with_progress(request, on_text)
        })
        .await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        patiently(self.observer, self.backoff, |retries| async move {
            // A retry discards the failed attempt's partial text.
            if retries > 0 {
                on_retry(retries)?;
            }
            self.inner
                .complete_with_retry_progress(request, on_text, on_retry)
                .await
        })
        .await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        patiently(self.observer, self.backoff, |retries| async move {
            if retries > 0 {
                on_retry(retries)?;
            }
            self.inner
                .complete_with_reasoning_progress(request, on_text, on_reasoning, on_retry)
                .await
        })
        .await
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
    fn thinks_before_answering(&self) -> bool {
        self.inner.thinks_before_answering()
    }
    fn pace(&self) -> Option<lattice_core::application::ports::llm_port::Pace> {
        self.inner.pace()
    }
    fn requests(&self) -> Option<lattice_core::application::ports::llm_port::RequestLoad> {
        self.inner.requests()
    }
    fn hold_cache(
        &self,
        key: &str,
    ) -> Option<lattice_core::application::ports::llm_port::CacheHold> {
        self.inner.hold_cache(key)
    }
    fn keeps_read_prompts(&self) -> bool {
        self.inner.keeps_read_prompts()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        patiently(self.observer, self.backoff, |_| {
            self.inner.count_tokens_exact(text)
        })
        .await
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
    fn vision(&self) -> Option<lattice_core::domain::models::vision::VisionGeometry> {
        self.inner.vision()
    }
    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
}

#[cfg(test)]
#[path = "patient_model_tests.rs"]
mod tests;
