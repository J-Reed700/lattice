//! What a backend answers for the scheduler in front of it: its own facts and
//! the one way to reach its wire, which takes only a request that was fitted
//! to the window.

use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::Result;

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
    fn output_limits(&self) -> OutputLimits;
    fn supports_tool_calling(&self) -> bool;
    fn vision(&self) -> Option<VisionGeometry>;
    fn keeps_read_prompts(&self) -> bool;
    fn is_alive(&self) -> bool;
    async fn is_ready(&self) -> Result<bool>;
    async fn send(&self, fitted: &Fitted<'_>, progress: Progress<'_>)
        -> Result<CompletionResponse>;
}
