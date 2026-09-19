//! What a turn did, as it does it.
//!
//! One structure is streamed while the turn runs and persisted when it ends, so
//! what the reader watched is what the record says. The old design had two
//! half-answers to the same question — an `activity` event with a sentence and
//! no history, and a `retrying` event that overwrote the answer bubble — and
//! neither survived the turn. A step both goes out on the wire and stays in the
//! recorder, which is the whole point: the timeline under a finished answer is
//! the same list the reader watched tick past.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::Emitter;
use tracing::warn;

use super::ChatStreamEventDto;

/// Steps one turn may record. More than this and the timeline is a log, not a
/// record of what happened; a turn that hits the cap is pathological anyway.
const MAX_STEPS: usize = 200;

/// A step's `detail` is a hint, not a transcript: the query, the host, a tool's
/// arguments. Anything longer is cut.
const MAX_DETAIL_CHARS: usize = 200;

/// What kind of work a step was. Stable codes, not prose — the label is what a
/// person reads, this is what the UI groups and tests match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnStepKind {
    Route,
    Plan,
    SearchDocuments,
    Sufficiency,
    CorrectiveSearch,
    WebSearch,
    ReadPage,
    Wiki,
    OpenDocument,
    Tool,
    Generate,
    Verify,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnStepState {
    Running,
    Done,
    Failed,
}

/// One thing the turn did, with how long it took and what came of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnStepDto {
    /// Stable within the turn. A finish event carries the id of its start, and
    /// the UI merges the two rather than appending a second row.
    pub id: String,
    pub kind: TurnStepKind,
    /// A human sentence — "Searching your documents", "Reading example.com".
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub state: TurnStepState,
    /// Offset from the start of the turn, not a wall clock: a persisted record
    /// has to mean the same thing in a week.
    pub started_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// What the step produced, in the register of the label: "8 passages from
    /// 3 files", "not enough support: low term coverage".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// The model that answered **this** turn, which is not necessarily the one the
/// conversation is filed under: a chat can be re-pointed between turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnModelDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnTimingDto {
    /// Time to the persisted answer, not to the last token.
    pub total_ms: u64,
    pub router_ms: u64,
    pub retrieval_ms: u64,
    pub generation_ms: u64,
    pub verification_ms: u64,
    /// Time inside tool calls, which is part of `generation_ms`, not beside it.
    pub tool_ms: u64,
}

/// Token counts, each absent when the provider did not report one. Absent is
/// not zero: a local model that says nothing about its usage has not used no
/// tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnTokensDto {
    pub completion: Option<u64>,
    pub context_used: Option<u64>,
}

/// What the router decided, and how sure it was.
///
/// `resolve_router_decision` used to throw both of these away the moment it had
/// them, so an answer could be steered by a 0.31-confidence guess and say
/// nothing about it.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnRouterDto {
    pub action: String,
    pub confidence: f32,
    pub rationale: Option<String>,
}

/// The whole record of one turn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnRecordDto {
    pub model: Option<TurnModelDto>,
    pub steps: Vec<TurnStepDto>,
    pub timing: TurnTimingDto,
    pub tokens: TurnTokensDto,
    pub router: Option<TurnRouterDto>,
}

/// A step that has begun. Hold it until the work is over, then hand it back to
/// [`TurnRecorder::end`].
#[derive(Debug, Clone)]
pub struct StepId(Option<String>);

impl StepId {
    /// A step the recorder refused (the cap) has no id, and finishing it is a
    /// no-op rather than an error the caller has to think about.
    fn none() -> Self {
        Self(None)
    }
}

/// Emits and accumulates at the same time.
///
/// Owned by the turn and threaded wherever the stream emitter is threaded. The
/// emit is best-effort: losing a progress note must never fail a generation
/// that is otherwise going fine.
pub struct TurnRecorder {
    conversation_id: String,
    request_id: String,
    started: Instant,
    emit: Box<dyn Fn(ChatStreamEventDto) + Send + Sync>,
    steps: Mutex<Vec<TurnStepDto>>,
    next_id: AtomicUsize,
}

impl std::fmt::Debug for TurnRecorder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TurnRecorder")
            .field("conversation_id", &self.conversation_id)
            .field("request_id", &self.request_id)
            .field("steps", &self.steps.lock().map(|steps| steps.len()).ok())
            .finish()
    }
}

impl TurnRecorder {
    pub fn new<R: tauri::Runtime>(
        window: &tauri::Window<R>,
        conversation_id: &str,
        request_id: &str,
    ) -> Self {
        let window = window.clone();
        Self::with_emitter(
            conversation_id,
            request_id,
            Box::new(move |payload| {
                if let Err(error) = window.emit("llm-stream", payload) {
                    warn!(%error, "Failed to emit a turn step");
                }
            }),
        )
    }

    /// A recorder that accumulates and sends nothing. For tests, and for any
    /// turn with no window to speak to.
    pub fn silent(conversation_id: &str, request_id: &str) -> Self {
        Self::with_emitter(conversation_id, request_id, Box::new(|_| {}))
    }

    fn with_emitter(
        conversation_id: &str,
        request_id: &str,
        emit: Box<dyn Fn(ChatStreamEventDto) + Send + Sync>,
    ) -> Self {
        Self {
            conversation_id: conversation_id.to_owned(),
            request_id: request_id.to_owned(),
            started: Instant::now(),
            emit,
            steps: Mutex::new(Vec::new()),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Milliseconds since the turn began.
    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Start a step and say so. Hold the returned id until the work is over.
    pub fn begin(
        &self,
        kind: TurnStepKind,
        label: impl Into<String>,
        detail: Option<String>,
    ) -> StepId {
        let step = TurnStepDto {
            id: format!("s{}", self.next_id.fetch_add(1, Ordering::Relaxed)),
            kind,
            label: label.into(),
            detail: detail.map(|text| clip(&text)),
            state: TurnStepState::Running,
            started_at_ms: self.elapsed_ms(),
            duration_ms: None,
            result: None,
        };
        let Ok(mut steps) = self.steps.lock() else {
            return StepId::none();
        };
        if steps.len() >= MAX_STEPS {
            return StepId::none();
        }
        let id = StepId(Some(step.id.clone()));
        steps.push(step.clone());
        drop(steps);
        self.send(step);
        id
    }

    /// Start a step whose end is guaranteed.
    ///
    /// The tool loop and the retrieval pipeline both return early from the
    /// middle of a step — a budget exhausted, a stop pressed, a search that
    /// threw — and a step left running forever is worse than one marked
    /// failed. Dropping the guard without a verdict records a failure.
    pub fn begin_guarded(
        &self,
        kind: TurnStepKind,
        label: impl Into<String>,
        detail: Option<String>,
    ) -> StepGuard<'_> {
        StepGuard {
            recorder: self,
            id: self.begin(kind, label, detail),
        }
    }

    /// Finish a step that is running, and say so.
    ///
    /// Only a running step is changed, so the guard's drop cannot overwrite a
    /// verdict the caller already gave.
    pub fn end(&self, id: &StepId, state: TurnStepState, result: Option<String>) {
        let Some(step_id) = id.0.as_deref() else {
            return;
        };
        let Ok(mut steps) = self.steps.lock() else {
            return;
        };
        let Some(step) = steps
            .iter_mut()
            .find(|step| step.id == step_id && step.state == TurnStepState::Running)
        else {
            return;
        };
        step.state = state;
        step.duration_ms = Some(self.elapsed_ms().saturating_sub(step.started_at_ms));
        step.result = result.map(|text| clip(&text));
        let finished = step.clone();
        drop(steps);
        self.send(finished);
    }

    /// Record something that is already over — no waiting, no duration worth
    /// naming. A router decision is the shape of this: by the time there is
    /// anything to say, it has happened.
    pub fn note(
        &self,
        kind: TurnStepKind,
        label: impl Into<String>,
        detail: Option<String>,
        result: Option<String>,
    ) {
        let id = self.begin(kind, label, detail);
        self.end(&id, TurnStepState::Done, result);
    }

    /// Everything recorded so far, oldest first.
    pub fn steps(&self) -> Vec<TurnStepDto> {
        self.steps
            .lock()
            .map(|steps| steps.clone())
            .unwrap_or_default()
    }

    fn send(&self, step: TurnStepDto) {
        (self.emit)(ChatStreamEventDto {
            status: Some("step".to_owned()),
            step: Some(step),
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        });
    }
}

/// A running step that cannot be forgotten. See
/// [`TurnRecorder::begin_guarded`].
pub struct StepGuard<'a> {
    recorder: &'a TurnRecorder,
    id: StepId,
}

impl StepGuard<'_> {
    pub fn done(self, result: Option<String>) {
        self.recorder.end(&self.id, TurnStepState::Done, result);
    }

    pub fn failed(self, result: Option<String>) {
        self.recorder.end(&self.id, TurnStepState::Failed, result);
    }
}

impl Drop for StepGuard<'_> {
    fn drop(&mut self) {
        // Reached only when the caller never gave a verdict: the turn walked
        // away from this step, which is a failure however it got there.
        self.recorder.end(&self.id, TurnStepState::Failed, None);
    }
}

fn clip(text: &str) -> String {
    crate::shared::text_utils::safe_truncate(text.trim(), MAX_DETAIL_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn capturing() -> (TurnRecorder, Arc<Mutex<Vec<ChatStreamEventDto>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let recorder = TurnRecorder::with_emitter(
            "conv-1",
            "req-1",
            Box::new(move |payload| sink.lock().unwrap().push(payload)),
        );
        (recorder, seen)
    }

    /// The point of the whole thing: the reader watches the same list the
    /// record ends up holding.
    #[test]
    fn a_finish_event_carries_the_id_of_its_start() {
        let (recorder, seen) = capturing();

        let step = recorder.begin(TurnStepKind::SearchDocuments, "Searching your documents", None);
        recorder.end(
            &step,
            TurnStepState::Done,
            Some("8 passages from 3 files".into()),
        );

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let start = seen[0].step.as_ref().unwrap();
        let finish = seen[1].step.as_ref().unwrap();
        assert_eq!(start.id, finish.id);
        assert_eq!(start.state, TurnStepState::Running);
        assert_eq!(finish.state, TurnStepState::Done);
        assert_eq!(finish.result.as_deref(), Some("8 passages from 3 files"));
        assert_eq!(seen[0].status.as_deref(), Some("step"));
    }

    #[test]
    fn the_recorder_keeps_what_it_emitted() {
        let (recorder, _) = capturing();

        let step = recorder.begin(TurnStepKind::Generate, "Thinking", None);
        recorder.end(&step, TurnStepState::Done, None);
        recorder.note(TurnStepKind::Verify, "Checking the answer", None, None);

        let steps = recorder.steps();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].kind, TurnStepKind::Generate);
        assert_eq!(steps[0].state, TurnStepState::Done);
        assert!(steps[0].duration_ms.is_some());
        assert_eq!(steps[1].kind, TurnStepKind::Verify);
    }

    /// A step that is still running has no duration. Reporting zero would say
    /// it finished instantly, which is a different and untrue claim.
    #[test]
    fn a_running_step_has_no_duration() {
        let (recorder, _) = capturing();

        recorder.begin(TurnStepKind::WebSearch, "Searching the web", None);

        let steps = recorder.steps();
        assert_eq!(steps[0].state, TurnStepState::Running);
        assert_eq!(steps[0].duration_ms, None);
    }

    #[test]
    fn a_failed_step_says_so_and_keeps_its_reason() {
        let (recorder, _) = capturing();

        let step = recorder.begin(TurnStepKind::ReadPage, "Reading example.com", None);
        recorder.end(
            &step,
            TurnStepState::Failed,
            Some("HTTP 403 Forbidden".into()),
        );

        let steps = recorder.steps();
        assert_eq!(steps[0].state, TurnStepState::Failed);
        assert_eq!(steps[0].result.as_deref(), Some("HTTP 403 Forbidden"));
    }

    #[test]
    fn detail_is_clipped_rather_than_carrying_a_transcript() {
        let (recorder, _) = capturing();

        recorder.note(
            TurnStepKind::Tool,
            "Running something",
            Some("x".repeat(5_000)),
            None,
        );

        assert!(recorder.steps()[0].detail.as_ref().unwrap().chars().count() <= MAX_DETAIL_CHARS);
    }

    /// A runaway turn must not persist a megabyte of timeline, and finishing a
    /// step the cap refused must not panic.
    #[test]
    fn the_step_list_is_capped() {
        let (recorder, _) = capturing();

        let mut refused = None;
        for _ in 0..(MAX_STEPS + 10) {
            refused = Some(recorder.begin(TurnStepKind::Tool, "Running something", None));
        }

        assert_eq!(recorder.steps().len(), MAX_STEPS);
        recorder.end(refused.as_ref().unwrap(), TurnStepState::Done, None);
    }

    /// A step the turn walked away from — a budget exhausted mid-generation, a
    /// stop press — must not sit in the record spinning forever.
    #[test]
    fn a_step_abandoned_without_a_verdict_is_recorded_as_failed() {
        let (recorder, _) = capturing();

        drop(recorder.begin_guarded(TurnStepKind::Generate, "Thinking", None));

        assert_eq!(recorder.steps()[0].state, TurnStepState::Failed);
    }

    #[test]
    fn a_verdict_survives_the_guard_going_out_of_scope() {
        let (recorder, _) = capturing();

        recorder
            .begin_guarded(TurnStepKind::Generate, "Thinking", None)
            .done(Some("wrote 412 words".into()));

        let steps = recorder.steps();
        assert_eq!(steps[0].state, TurnStepState::Done);
        assert_eq!(steps[0].result.as_deref(), Some("wrote 412 words"));
    }

    /// Kind and state reach the UI as the stable codes the design names, not as
    /// Rust's capitalization.
    #[test]
    fn kinds_and_states_serialize_as_the_codes_the_ui_matches_on() {
        let json = serde_json::to_value(TurnStepDto {
            id: "s0".into(),
            kind: TurnStepKind::CorrectiveSearch,
            label: "Searching again".into(),
            detail: None,
            state: TurnStepState::Running,
            started_at_ms: 0,
            duration_ms: None,
            result: None,
        })
        .unwrap();

        assert_eq!(json["kind"], "corrective_search");
        assert_eq!(json["state"], "running");
        assert_eq!(json["startedAtMs"], 0);
        assert!(json.get("durationMs").is_none());
        assert!(json.get("detail").is_none());
    }
}
