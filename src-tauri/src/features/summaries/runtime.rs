//! Lazily-resolved models for background summary work.
//!
//! Summarization happens minutes after wiring, on a task that owns nothing:
//! the utility model may not be loaded yet, may have been swapped, or may not
//! exist at all. A port rather than two `Arc`s keeps that lifecycle where it
//! belongs (the container) and keeps this feature testable with stubs.

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::{EmbeddingPort, LLMPort};

#[async_trait]
pub trait SummaryRuntimePort: Send + Sync {
    /// The utility model, or `None` when none is configured or loadable.
    async fn utility_llm(&self) -> Option<Arc<dyn LLMPort>>;

    /// The active embedder, or `None` when it cannot be loaded.
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>>;
}
