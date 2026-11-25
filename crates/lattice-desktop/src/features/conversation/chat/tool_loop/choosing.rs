//! The options of a call that chooses sentences by handle for research notes.

use lattice_core::application::ports::llm_port::{CompletionRequest, SamplingOverride};
use lattice_core::application::services::grounded_generation::CallOptions;
use std::time::Duration;

/// A selection, not a composition: the same text should be chosen from the
/// same way every time, and thinking did not help Claude choose and took two
/// to three times as long.
pub(super) fn choosing_call(request: &CompletionRequest, remaining: Duration) -> CallOptions {
    CallOptions {
        priority: request.priority,
        cancel: request.cancel.clone(),
        cache_key: request.cache_key.clone(),
        sampling: Some(SamplingOverride::deterministic()),
        time_budget: Some(remaining),
        reasoning_effort: Some("none".into()),
        ..Default::default()
    }
}
