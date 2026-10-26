//! The decorator that puts a backend's scheduler in front of a port.
//!
//! Every method that reaches the model is admitted first, at the priority its
//! request carries. The same decorator owns the backend's calibrated token estimate, because it is
//! the one place that sees both what was sent and the usage reported back, and
//! the backend's pace, measured from the timing it reports on those requests.
//!
//! Before admission, a request whose prompt alone fills the port's configured
//! context window is refused, whichever backend is behind it: nothing that
//! cannot fit waits for a slot or reaches a server. Any other request is sent,
//! its answer cut to the room the prompt leaves.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;

use super::rejection::{window_refusal, RequestSize};
use super::{
    cancelled_error, AdmissionRequest, InferenceScheduler, PaceEstimator, Permit, ServerTokenizer,
    TokenEstimator,
};
use lattice_core::application::ports::llm_port::{
    tokens_for_chars, CompletionInput, CompletionRequest, CompletionResponse, Pace, RequestLoad,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::Result;

/// A port whose every model call goes through one backend's scheduler.
pub(crate) struct ScheduledLlm {
    inner: Arc<dyn LLMPort>,
    scheduler: Arc<InferenceScheduler>,
    estimator: TokenEstimator,
    pace: PaceEstimator,
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
            pace: PaceEstimator::default(),
            output_limit,
            tokenizer: None,
        }
    }

    /// Count exactly with this llama-server's tokenizer.
    pub fn with_tokenizer(mut self, tokenizer: ServerTokenizer) -> Self {
        self.tokenizer = Some(tokenizer);
        self
    }

    /// The room the request's prompt leaves in the configured window for an
    /// answer; or, before it is queued or sent, the refusal of a request whose
    /// prompt leaves none.
    ///
    /// The answer's size is a ceiling, never a demand, so nothing else is
    /// refused: a caller that sized its answer to the room its own count left,
    /// and counts the prompt a few tokens lower than this does, gets a few
    /// tokens less answer. The calibrated estimate decides when the prompt
    /// plainly leaves room. When it does not, a backend with a tokenizer
    /// counts the prompt's text exactly before anything is refused. Opaque
    /// provider state (signed reasoning) is not text and is left out of both
    /// counts, so it never causes a refusal. Pictures are charged in both by
    /// their declared geometry, never as their bytes.
    async fn room_for_answer(&self, request: &CompletionRequest) -> Result<usize> {
        let window = self.inner.max_context_tokens();
        let image_tokens = request.image_tokens();
        let estimate = tokens_for_chars(
            prompt_texts(request).map(|text| text.chars().count()).sum(),
            self.estimator.chars_per_token(),
        )
        .saturating_add(image_tokens);
        if estimate < window {
            return Ok(window - estimate);
        }
        let (prompt_tokens, counted_exactly) = match &self.tokenizer {
            Some(_) => match self.count_tokens_exact(&prompt_text(request)).await {
                Ok(exact) => (exact.saturating_add(image_tokens), true),
                Err(error) => {
                    tracing::debug!(%error, "The server could not count the prompt; keeping the estimate");
                    (estimate, false)
                }
            },
            None => (estimate, false),
        };
        if prompt_tokens < window {
            return Ok(window - prompt_tokens);
        }
        Err(window_refusal(
            self.inner.model_name(),
            RequestSize {
                prompt_tokens,
                counted_exactly,
                output_tokens: 0,
                image_tokens,
                image_count: request.image_count(),
            },
            window,
        ))
    }

    /// Wait for a slot, and pin the request to it when the backend pins.
    async fn admit_typed<'r>(
        &self,
        request: &'r CompletionRequest,
    ) -> Result<(Permit, Cow<'r, CompletionRequest>)> {
        let room = self.room_for_answer(request).await?;
        let ceiling = request.effective_max_output_tokens(self.output_limit) as usize;
        // A llama-server counts the prompt exactly and cuts the answer to the
        // room that leaves (`LlamaCppLlm::measured_body`). Every other backend
        // has its answer cut here, so no provider is asked for more than its
        // window holds.
        let cut = self.tokenizer.is_none() && ceiling > room;
        let output = if cut { room } else { ceiling };
        let prompt = tokens_for_chars(prompt_chars(request), self.estimator.chars_per_token())
            .saturating_add(request.image_tokens());
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
        let mut request = Cow::Borrowed(request);
        if cut {
            request.to_mut().max_output_tokens = Some(u32::try_from(room).unwrap_or(u32::MAX));
        }
        if let Some(slot) = permit.slot() {
            request.to_mut().assigned_slot = Some(slot);
        }
        Ok((permit, request))
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
/// result, plus the tool schemas, which are read like any other prompt text,
/// plus opaque provider state, which takes room however it is counted.
pub(super) fn prompt_chars(request: &CompletionRequest) -> usize {
    let text: usize = prompt_texts(request).map(|text| text.chars().count()).sum();
    let native: usize = request
        .input
        .iter()
        .map(|item| match item {
            CompletionInput::Native { value } => value.to_string().chars().count(),
            _ => 0,
        })
        .sum();
    text + native
}

/// The text a tokenizer reads in a request: every message with its role,
/// every call and result, then the tool schemas. Opaque provider state
/// (signed reasoning) and pictures' bytes are not text and are left out; a
/// picture's alt stands in for it.
fn prompt_texts(request: &CompletionRequest) -> impl Iterator<Item = Cow<'_, str>> {
    let input = request.input.iter().flat_map(|item| match item {
        CompletionInput::Message { role, content } => {
            vec![
                Cow::Borrowed(role.as_str()),
                Cow::Borrowed(content.as_str()),
            ]
        }
        CompletionInput::ToolCall {
            name, arguments, ..
        } => vec![
            Cow::Borrowed(name.as_str()),
            Cow::Owned(arguments.to_string()),
        ],
        CompletionInput::Multimodal { role, .. } => vec![Cow::Borrowed(role.as_str()), item.text()],
        CompletionInput::ToolResult { output, .. } => vec![Cow::Borrowed(output.as_str())],
        CompletionInput::Native { .. } => Vec::new(),
    });
    let tools = request.tools.iter().flat_map(|tool| {
        [
            Cow::Borrowed(tool.name.as_str()),
            Cow::Borrowed(tool.description.as_str()),
            Cow::Owned(tool.parameters.to_string()),
        ]
    });
    input.chain(tools)
}

/// [`prompt_texts`] as one text, for a tokenizer to count.
fn prompt_text(request: &CompletionRequest) -> String {
    prompt_texts(request).collect::<Vec<_>>().join("\n")
}

#[async_trait]
impl LLMPort for ScheduledLlm {
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

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
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
        self.inner.thinks_before_answering()
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
