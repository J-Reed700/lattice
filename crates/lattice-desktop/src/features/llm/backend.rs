//! What a backend answers for the scheduler in front of it: its own facts and
//! the one way to reach its wire, which takes only a request that was fitted
//! to the window.

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::services::model_contract::shape::Schema;
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::{AppError, Result};

use super::fitting::{Fitted, OutputLimits, Thinking};

/// Receives text as it arrives.
pub(crate) type TextSink<'a> = dyn Fn(String) -> Result<()> + Send + Sync + 'a;

/// Told that an attempt failed and the numbered one begins.
pub(crate) type RetrySink<'a> = dyn Fn(usize) -> Result<()> + Send + Sync + 'a;

/// A sink for a caller that does not want the text.
pub(crate) fn ignore_text(_: String) -> Result<()> {
    Ok(())
}

/// A sink for a caller that does not want to hear of retries.
pub(crate) fn ignore_retry(_: usize) -> Result<()> {
    Ok(())
}

/// What the caller wants delivered while the answer is written.
pub(crate) enum Progress<'a> {
    /// The whole answer at once.
    None,
    Streamed {
        on_text: &'a TextSink<'a>,
        on_reasoning: Option<&'a TextSink<'a>>,
        on_retry: Option<&'a RetrySink<'a>>,
    },
}

#[async_trait]
pub(crate) trait ModelBackend: Send + Sync {
    fn model_name(&self) -> &str;
    fn provider_name(&self) -> &str;
    fn window(&self) -> usize;
    fn thinking(&self, request: &CompletionRequest) -> Thinking;
    fn supports_native_thinking(&self) -> bool {
        self.thinking(&CompletionRequest::probe()).is_on()
    }
    fn output_limits(&self) -> OutputLimits;
    fn supports_tool_calling(&self) -> bool;
    fn vision(&self) -> Option<VisionGeometry>;
    fn keeps_read_prompts(&self) -> bool;
    /// Whether this backend holds an answer to `schema` as written.
    fn enforces(&self, schema: &Schema) -> bool;
    fn is_alive(&self) -> bool;
    async fn is_ready(&self) -> Result<bool>;
    async fn send(&self, fitted: &Fitted<'_>, progress: Progress<'_>)
        -> Result<CompletionResponse>;
}

/// Read a non-streamed HTTP JSON response body up to `max_bytes`, failing
/// before buffering unbounded bytes into memory.
pub(crate) async fn read_bounded_json(
    mut response: reqwest::Response,
    max_bytes: usize,
    on_oversize: impl FnOnce() -> AppError,
    on_transport: impl Fn(reqwest::Error) -> AppError,
    on_json: impl FnOnce(serde_json::Error) -> AppError,
) -> Result<serde_json::Value> {
    if response
        .content_length()
        .and_then(|len| usize::try_from(len).ok())
        .is_some_and(|len| len > max_bytes)
    {
        return Err(on_oversize());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(&on_transport)? {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(on_oversize());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(on_json)
}
