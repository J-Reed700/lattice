//! Opt-in diagnostic capture of synthetic eval inputs and public model answers.
//! Provider credentials, reasoning and replay state are never recorded.
use super::*;
use lattice_core::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, Pace, RequestLoad,
};
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::{AppError, Result};

pub(super) struct RecordingLlm {
    inner: Box<dyn LLMPort>,
    config: EvalConfig,
    sequence: AtomicU64,
}

impl RecordingLlm {
    pub(super) fn new(inner: Box<dyn LLMPort>, config: EvalConfig) -> Self {
        Self {
            inner,
            config,
            sequence: AtomicU64::new(0),
        }
    }

    fn record(&self, request: &CompletionRequest, response: &CompletionResponse) -> Result<()> {
        create_dir_all(&self.config.log_dir)?;
        let path = self.config.log_dir.join(format!(
            "lattice-learning-studio-model-calls-{}.jsonl",
            std::process::id()
        ));
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        serde_json::to_writer(&mut file, &json!({
            "harnessVersion":HARNESS_VERSION,"recordedAt":chrono::Utc::now().to_rfc3339(),
            "sequence":self.sequence.fetch_add(1, Ordering::Relaxed),
            "model":self.config.model,"provider":self.config.provider,
            "request":{"input":request.input,"schema":request.json_schema,"maxOutputTokens":request.max_output_tokens,"sampling":request.sampling},
            "response":{"text":response.text,"finishReason":response.finish_reason,"inputTokens":response.input_tokens,"outputTokens":response.output_tokens},
        })).map_err(|error| AppError::Serialization(error.to_string()))?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl LLMPort for RecordingLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let response = self.inner.complete(request).await?;
        self.record(request, &response)?;
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
    fn vision(&self) -> Option<VisionGeometry> {
        self.inner.vision()
    }
    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
    fn pace(&self) -> Option<Pace> {
        self.inner.pace()
    }
    fn requests(&self) -> Option<RequestLoad> {
        self.inner.requests()
    }
}
