//! What a cloud or OpenAI-compatible request cost in time, for the backend's
//! pace (`LLMPort::pace`): a llama-server `timings` object as reported,
//! otherwise a streamed answer timed by the app's clock (reading until the
//! model began its answer, shown or not, writing after). A whole answer
//! without timings is not timed.

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

/// The app's clock on one streamed request.
pub(super) struct StreamClock {
    sent: Instant,
    began: OnceLock<Instant>,
}

impl StreamClock {
    pub(super) fn started(sent: Instant) -> Self {
        Self {
            sent,
            began: OnceLock::new(),
        }
    }

    /// The model's answer began now; only the first call counts.
    pub(super) fn began(&self) {
        self.began.get_or_init(Instant::now);
    }

    /// The request's timing, ending now, for `prompt_tokens` read and
    /// `output_tokens` written. `None` when no answer began (a stream with no
    /// output at all), which leaves no edge between the two.
    pub(super) fn timing(&self, prompt_tokens: u64, output_tokens: u64) -> Option<ServedTiming> {
        let first = *self.began.get()?;
        Some(ServedTiming {
            prompt_tokens,
            prompt_seconds: first.duration_since(self.sent).as_secs_f64(),
            output_tokens,
            output_seconds: first.elapsed().as_secs_f64(),
        })
    }
}
