//! The decorator that puts a backend's scheduler in front of a port.
//!
//! Every method that reaches the model is admitted first, the older string
//! API included (at background priority: it has no request to carry one). The
//! same decorator owns the backend's calibrated token estimate, because it is
//! the one place that sees both what was sent and the usage reported back.

use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::Stream;

use super::{
    cancelled_error, AdmissionRequest, InferenceScheduler, Permit, ServerTokenizer, TokenEstimator,
};
use crate::application::ports::llm_port::{
    tokens_for_chars, CompletionInput, CompletionRequest, CompletionResponse, InferencePriority,
    StreamChunk, ToolDefinition,
};
use crate::application::ports::LLMPort;
use crate::shared::error::Result;

/// A port whose every model call goes through one backend's scheduler.
pub(crate) struct ScheduledLlm {
    inner: Arc<dyn LLMPort>,
    scheduler: Arc<InferenceScheduler>,
    estimator: TokenEstimator,
    /// The backend's own output ceiling, which a request without a cap of its
    /// own will reserve.
    output_limit: u32,
    tokenizer: Option<ServerTokenizer>,
}

impl ScheduledLlm {
    pub fn new(
        inner: Arc<dyn LLMPort>,
        scheduler: Arc<InferenceScheduler>,
        output_limit: u32,
    ) -> Self {
        Self {
            inner,
            scheduler,
            estimator: TokenEstimator::default(),
            output_limit,
            tokenizer: None,
        }
    }

    /// Count exactly with this llama-server's tokenizer.
    pub fn with_tokenizer(mut self, tokenizer: ServerTokenizer) -> Self {
        self.tokenizer = Some(tokenizer);
        self
    }

    /// Wait for a slot, and pin the request to it when the backend pins.
    async fn admit_typed<'r>(
        &self,
        request: &'r CompletionRequest,
    ) -> Result<(Permit, Cow<'r, CompletionRequest>)> {
        let output = request.effective_max_output_tokens(self.output_limit) as usize;
        let prompt = tokens_for_chars(prompt_chars(request), self.estimator.chars_per_token());
        let permit = self
            .scheduler
            .admit(
                AdmissionRequest {
                    priority: request.priority,
                    tokens: prompt.saturating_add(output),
                    cache_key: request.cache_key.clone(),
                },
                request.cancel.as_ref(),
            )
            .await?;
        let request = match permit.slot() {
            Some(slot) => {
                let mut pinned = request.clone();
                pinned.assigned_slot = Some(slot);
                Cow::Owned(pinned)
            }
            None => Cow::Borrowed(request),
        };
        Ok((permit, request))
    }

    /// The string API carries no request: background priority, no slot pin.
    async fn admit_text(&self, prompt: &str, context: &[String]) -> Result<Permit> {
        let chars = prompt.chars().count()
            + context
                .iter()
                .map(|entry| entry.chars().count())
                .sum::<usize>();
        let prompt = tokens_for_chars(chars, self.estimator.chars_per_token());
        self.scheduler
            .admit(
                AdmissionRequest {
                    priority: InferencePriority::Background,
                    tokens: prompt.saturating_add(self.output_limit as usize),
                    cache_key: None,
                },
                None,
            )
            .await
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
    /// provider state (signed reasoning, images) is not text a tokenizer read
    /// character by character, so a request carrying it teaches nothing.
    fn observe(&self, request: &CompletionRequest, response: &CompletionResponse) {
        if response.input_tokens == 0
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

    async fn completed(
        &self,
        request: &CompletionRequest,
        call: impl std::future::Future<Output = Result<CompletionResponse>>,
    ) -> Result<CompletionResponse> {
        let response = self.run(request, call).await?;
        self.observe(request, &response);
        Ok(response)
    }
}

/// Characters a request puts in front of the model: every message, call and
/// result, plus the tool schemas, which are read like any other prompt text.
pub(super) fn prompt_chars(request: &CompletionRequest) -> usize {
    let input: usize = request
        .input
        .iter()
        .map(|item| match item {
            CompletionInput::Message { role, content } => {
                role.chars().count() + content.chars().count()
            }
            CompletionInput::ToolCall {
                name, arguments, ..
            } => name.chars().count() + arguments.to_string().chars().count(),
            CompletionInput::ToolResult { output, .. } => output.chars().count(),
            CompletionInput::Native { value } => value.to_string().chars().count(),
        })
        .sum();
    let tools: usize = request
        .tools
        .iter()
        .map(|tool| {
            tool.name.chars().count()
                + tool.description.chars().count()
                + tool.parameters.to_string().chars().count()
        })
        .sum();
    input + tools
}

/// A stream that holds its slot until it is finished or dropped.
struct Held<S> {
    stream: S,
    _permit: Permit,
}

impl<S: Stream + Unpin> Stream for Held<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

#[async_trait]
impl LLMPort for ScheduledLlm {
    fn supports_typed_completions(&self) -> bool {
        self.inner.supports_typed_completions()
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_permit, request) = self.admit_typed(request).await?;
        self.completed(&request, self.inner.complete(&request))
            .await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let (_permit, request) = self.admit_typed(request).await?;
        self.completed(
            &request,
            self.inner.complete_with_progress(&request, on_text),
        )
        .await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let (_permit, request) = self.admit_typed(request).await?;
        self.completed(
            &request,
            self.inner
                .complete_with_retry_progress(&request, on_text, on_retry),
        )
        .await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let (_permit, request) = self.admit_typed(request).await?;
        self.completed(
            &request,
            self.inner
                .complete_with_reasoning_progress(&request, on_text, on_reasoning, on_retry),
        )
        .await
    }

    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<String> {
        let _permit = self.admit_text(prompt, context).await?;
        self.inner.generate(prompt, context, images).await
    }

    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        let permit = self.admit_text(prompt, context).await?;
        let stream = self
            .inner
            .generate_streaming(prompt, context, images)
            .await?;
        Ok(Box::new(Held {
            stream,
            _permit: permit,
        }))
    }

    async fn generate_streaming_with_tools(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin + '_>> {
        let permit = self.admit_text(prompt, context).await?;
        let stream = self
            .inner
            .generate_streaming_with_tools(prompt, context, images, tools)
            .await?;
        Ok(Box::new(Held {
            stream,
            _permit: permit,
        }))
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }

    fn chars_per_token(&self) -> f64 {
        self.estimator.chars_per_token()
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

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
}
