//! What a cloud or OpenAI-compatible request cost in time, for the backend's
//! pace (`LLMPort::pace`): a llama-server `timings` object as reported,
//! otherwise a streamed answer timed by the app's clock (reading until the
//! first delta, writing after). A whole answer without timings is not timed.

use super::field;
use crate::features::llm::llama_cpp::streaming::ServerTimings;
use lattice_core::application::ports::llm_port::ServedTiming;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Instant;

/// The server's own account of a completed response: a llama.cpp-style
/// `timings` object, when it sent one.
pub(super) fn reported(value: &Value) -> Option<ServedTiming> {
    value
        .get("timings")
        .and_then(ServerTimings::from_value)
        .map(|timings| timings.served())
}

/// The prompt tokens a completed response's usage says the backend read:
/// its prompt count less the cached tokens it reports
/// (`input_tokens_details` on the Responses API, `prompt_tokens_details` on
/// chat completions). Anthropic counts cache reads apart from
/// `input_tokens` already, and the tokens it wrote to the cache apart too,
/// though it read those like any others.
pub(super) fn evaluated_prompt_tokens(value: &Value) -> u64 {
    let usage = field(value, "usage");
    let prompt = field(usage, "input_tokens")
        .as_u64()
        .or_else(|| field(usage, "prompt_tokens").as_u64())
        .unwrap_or(0)
        + field(usage, "cache_creation_input_tokens")
            .as_u64()
            .unwrap_or(0);
    let cached = ["input_tokens_details", "prompt_tokens_details"]
        .iter()
        .find_map(|details| field(field(usage, details), "cached_tokens").as_u64())
        .unwrap_or(0);
    prompt.saturating_sub(cached)
}

/// The app's clock on one streamed request.
pub(super) struct StreamClock {
    sent: Instant,
    first_delta: OnceLock<Instant>,
}

impl StreamClock {
    pub(super) fn started(sent: Instant) -> Self {
        Self {
            sent,
            first_delta: OnceLock::new(),
        }
    }

    /// A streamed delta arrived now; only the first one counts.
    pub(super) fn delta(&self) {
        self.first_delta.get_or_init(Instant::now);
    }

    /// The request's timing, ending now, for `prompt_tokens` read and
    /// `output_tokens` written. `None` when nothing streamed (an answer of
    /// tool calls alone), which leaves no edge between the two.
    pub(super) fn timing(&self, prompt_tokens: u64, output_tokens: u64) -> Option<ServedTiming> {
        let first = *self.first_delta.get()?;
        Some(ServedTiming {
            prompt_tokens,
            prompt_seconds: first.duration_since(self.sent).as_secs_f64(),
            output_tokens,
            output_seconds: first.elapsed().as_secs_f64(),
        })
    }
}
