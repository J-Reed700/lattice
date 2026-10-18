//! One generation of the tool loop: streamed into the answer bubble, its
//! reasoning shown on its step, bounded by Stop and by the turn's deadline.
//!
//! The last round can need two of these (see `final_round`), which is why it
//! is a function rather than the body of the loop.

use super::clock::TurnClock;
use super::final_round::LeakedCallFilter;
use super::{answer_was_cut_short, StreamEmitter, CUT_SHORT_NOTE};
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::turn_record::{StepGuard, TurnRecorder, TurnStepKind};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// A finished generation and its step, still open so the caller can say what
/// the round came to.
pub(super) struct Generation<'r> {
    pub(super) response: CompletionResponse,
    pub(super) step: StepGuard<'r>,
}

/// Run one generation. `hold_back_calls` keeps calls written into the text
/// out of the bubble: on a round that may not call tools they are never run.
pub(super) async fn generate<'r>(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    emitter: &StreamEmitter<'_>,
    recorder: &'r TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
    label: &str,
    hold_back_calls: bool,
) -> Result<Generation<'r>> {
    let started = Instant::now();
    // Begun before the call so a round that never produces a token is still a
    // visible, timed step rather than a gap. The guard ends it even on the
    // budget and cancellation returns below.
    let step = recorder.begin_guarded(TurnStepKind::Generate, label, None);
    let reasoning_progress = Mutex::new(ReasoningProgress::new());
    let filter = Mutex::new(LeakedCallFilter::new(hold_back_calls));
    let remaining = clock.remaining()?;
    request.time_budget = Some(remaining);
    let mut response = {
        let first_text_received = AtomicBool::new(false);
        let on_text = |text: String| {
            if !text.is_empty() && !first_text_received.swap(true, Ordering::Relaxed) {
                info!(
                    label,
                    first_text_ms = started.elapsed().as_millis() as u64,
                    "LLM first answer text received"
                );
            }
            let shown = filter
                .lock()
                .map_or_else(|_| text.clone(), |mut filter| filter.push(&text));
            if shown.is_empty() {
                return Ok(());
            }
            emitter.content(&shown)
        };
        let on_reasoning = |delta: String| {
            let update = reasoning_progress
                .lock()
                .ok()
                .and_then(|mut progress| progress.push(&delta));
            if let Some(reasoning) = update.as_deref() {
                step.set_reasoning(Some(reasoning));
            }
            Ok(())
        };
        // A provider-level retry is its own step. It used to overwrite the
        // answer bubble's text, so a turn that recovered ended up showing
        // "Model response failed. Retrying..." in place of the answer it went
        // on to write.
        let on_retry = |attempt: usize| {
            if let Ok(mut progress) = reasoning_progress.lock() {
                progress.reset();
            }
            step.set_reasoning(None);
            recorder.note(
                TurnStepKind::Retry,
                "Retrying the model",
                None,
                Some(format!("attempt {attempt}")),
            );
            Ok(())
        };
        let completion = timeout(
            remaining,
            llm.complete_with_reasoning_progress(&*request, &on_text, &on_reasoning, &on_retry),
        );
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(cancelled_error()),
            result = completion => result.map_err(|_| clock.exhausted())?,
        }
    }?;
    let held = filter
        .lock()
        .map(|mut filter| filter.finish())
        .unwrap_or_default();
    if !held.is_empty() {
        emitter.content(&held)?;
    }
    if let Some(reasoning) = response
        .reasoning
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        // Flush the last sub-threshold delta and make the persisted step
        // agree exactly with the provider's completed value.
        step.set_reasoning(Some(reasoning));
    } else if let Ok(progress) = reasoning_progress.lock() {
        step.set_reasoning(progress.text());
    }
    if answer_was_cut_short(
        &response.finish_reason,
        &response.text,
        !response.tool_calls.is_empty(),
    )? {
        emitter.content(CUT_SHORT_NOTE)?;
        response.text.push_str(CUT_SHORT_NOTE);
    }
    Ok(Generation { response, step })
}

/// What to show while the model is producing no text.
///
/// The first round is plain thinking; later rounds follow tool results, and
/// saying so is the difference between "stuck" and "on its third source".
pub(super) fn thinking_label(iteration: usize) -> &'static str {
    if iteration == 0 {
        "Thinking"
    } else {
        "Reading what it found and thinking"
    }
}

/// Live reasoning is a progress surface, not an unbounded transcript. Batch
/// token-sized deltas before emitting a replacement step so React does not
/// rerender once per token, then let the completed provider value replace this
/// capture before the step is persisted.
struct ReasoningProgress {
    text: String,
    chars: usize,
    last_sent_chars: usize,
    last_sent_at: Instant,
}

impl ReasoningProgress {
    const MAX_CHARS: usize = 256_000;
    const MIN_EMIT_CHARS: usize = 160;
    const MAX_EMIT_DELAY: Duration = Duration::from_millis(120);

    fn new() -> Self {
        Self {
            text: String::new(),
            chars: 0,
            last_sent_chars: 0,
            last_sent_at: Instant::now(),
        }
    }

    fn push(&mut self, delta: &str) -> Option<String> {
        let remaining = Self::MAX_CHARS.saturating_sub(self.chars);
        if remaining > 0 {
            let kept = lattice_core::shared::text::safe_truncate(delta, remaining);
            self.chars = self.chars.saturating_add(kept.chars().count());
            self.text.push_str(&kept);
        }
        let should_emit = self.last_sent_chars == 0
            || self.chars.saturating_sub(self.last_sent_chars) >= Self::MIN_EMIT_CHARS
            || self.last_sent_at.elapsed() >= Self::MAX_EMIT_DELAY;
        if !should_emit || self.text.trim().is_empty() {
            return None;
        }
        self.last_sent_chars = self.chars;
        self.last_sent_at = Instant::now();
        Some(self.text.clone())
    }

    fn reset(&mut self) {
        self.text.clear();
        self.chars = 0;
        self.last_sent_chars = 0;
        self.last_sent_at = Instant::now();
    }

    fn text(&self) -> Option<&str> {
        (!self.text.trim().is_empty()).then_some(self.text.as_str())
    }
}
