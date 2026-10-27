//! Guarantees the compaction job owes its callers.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §12, §16.
//!
//! These run against fakes on purpose. The real repository lives behind
//! `features::`, which application code may not import, and the real utility
//! model is the thing whose misbehaviour is being simulated. The fake port is
//! not a stub: it enforces the revision compare-and-swap, because one of these
//! tests is about what happens when that check fires.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use lattice_core::application::ports::conversation_memory::{
    CommittedMemorySnapshot, ConversationMemoryPort, MemoryCommitCandidate, MemoryCommitError,
    MemoryCommitPreconditions, ResolvedSpan, SourcePage, SourceReadLimits, SourceSpanRef,
};
use lattice_core::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, SamplingOverride,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::memory::{
    compute_digest, ConversationMemoryState, EvidencePurpose, EvidenceSpan, MemoryId, MemoryItem,
    MemoryKind, MemoryReview, MemorySnapshot, MemoryState, MemoryValidity, SourceMessage,
    SourceRole, SummaryRefold, SuppressedSpan, MAX_EVIDENCE_PER_ITEM, MAX_OPERATIONS_PER_RESPONSE,
    MAX_PROPOSAL_BYTES, MAX_QUOTE_BYTES,
};
use lattice_core::shared::error::{AppError, Result};

use super::segment::{clip_chars, segment_message};
use super::selection::select_boundary;
use super::{
    CompactionConfig, CompactionJob, CompactionRequest, CompactionStatus,
    MAX_COMMIT_CONFLICT_RETRIES, MAX_REPAIR_ATTEMPTS, MAX_SOURCE_BYTES_PER_ATTEMPT,
    MAX_SOURCE_MESSAGES_PER_ATTEMPT, MIN_RETAINED_TURNS,
};

const CONVERSATION: &str = "conv-memory";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn message(id: &str, sequence: i64, role: SourceRole, content: &str) -> SourceMessage {
    SourceMessage {
        id: id.to_string(),
        conversation_id: CONVERSATION.to_string(),
        sequence,
        role,
        content: content.to_string(),
        content_digest: compute_digest(content),
        status: "completed".to_string(),
        pictures: Vec::new(),
    }
}

/// Four complete turns. With the default retention that puts the first two
/// turns (`m1`..`m4`) in the compacted prefix and keeps the rest raw.
fn four_turns() -> Vec<SourceMessage> {
    vec![
        message("m1", 1, SourceRole::User, "Do not deploy until I approve."),
        message("m2", 2, SourceRole::Assistant, "Understood, I will wait."),
        message(
            "m3",
            3,
            SourceRole::User,
            "Actually you may deploy staging.",
        ),
        message("m4", 4, SourceRole::Assistant, "Noted, staging only."),
        message("m5", 5, SourceRole::User, "What about production?"),
        message("m6", 6, SourceRole::Assistant, "Still blocked."),
        message("m7", 7, SourceRole::User, "Understood, thanks."),
        message("m8", 8, SourceRole::Assistant, "Any time."),
    ]
}

/// A span over the whole of one message.
fn whole_span(source: &SourceMessage, purpose: EvidencePurpose) -> EvidenceSpan {
    EvidenceSpan {
        message_id: source.id.clone(),
        sequence: source.sequence,
        role: source.role,
        start_byte: 0,
        end_byte: source.content.len() as u32,
        content_digest: source.content_digest.clone(),
        purpose,
    }
}

/// An already-committed active constraint, quoting `source`.
fn committed_constraint(id: &str, source: &SourceMessage) -> MemoryItem {
    let now = chrono::Utc::now();
    MemoryItem {
        id: MemoryId::from_string(id).unwrap(),
        conversation_id: CONVERSATION.to_string(),
        kind: MemoryKind::Constraint,
        state: MemoryState::Active,
        label: "Deployment requires approval".to_string(),
        evidence: vec![whole_span(source, EvidencePurpose::Assertion)],
        created_at_sequence: source.sequence,
        changed_at_sequence: source.sequence,
        superseded_by: None,
        revision: 1,
        review: MemoryReview::Supported,
        related_item_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}

fn extraction_response(
    add: Vec<serde_json::Value>,
    transitions: Vec<serde_json::Value>,
    segment_ids: &[&str],
) -> String {
    serde_json::json!({
        "schema_version": 1,
        "add": add,
        "transitions": transitions,
        "processed_segment_ids": segment_ids,
    })
    .to_string()
}

fn proposed_constraint(
    candidate: &str,
    label: &str,
    message_id: &str,
    quote: &str,
) -> serde_json::Value {
    proposed_item(candidate, "constraint", label, message_id, quote)
}

fn proposed_item(
    candidate: &str,
    kind: &str,
    label: &str,
    message_id: &str,
    quote: &str,
) -> serde_json::Value {
    serde_json::json!({
        "candidate_id": candidate,
        "kind": kind,
        "label": label,
        "evidence": [{
            "message_id": message_id,
            "quote": quote,
            "occurrence": null,
            "purpose": "assertion",
        }],
    })
}

fn proposed_transition(item_id: &str, message_id: &str, quote: &str) -> serde_json::Value {
    serde_json::json!({
        "item_id": item_id,
        "state": "resolved",
        "replacement_candidate_id": null,
        "evidence": [{
            "message_id": message_id,
            "quote": quote,
            "occurrence": null,
            "purpose": "transition",
        }],
    })
}

fn verdict_response(pairs: &[(&str, &str)]) -> String {
    serde_json::json!({
        "verdicts": pairs
            .iter()
            .map(|(id, verdict)| serde_json::json!({
                "id": id,
                "verdict": verdict,
                "reason": "test",
            }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Fake port
// ---------------------------------------------------------------------------

struct PortState {
    messages: Vec<SourceMessage>,
    items: Vec<MemoryItem>,
    summary: Option<String>,
    /// The summary's `up_to_message_id`, so a delete can drop the summary the
    /// way the schema's cascade and clamp trigger do.
    summary_up_to: Option<String>,
    /// What a user's forget wrote, as `conversation_memory_suppressions` holds it.
    suppressions: Vec<SuppressedSpan>,
    /// Labels of forgotten items, oldest first.
    forgotten: Vec<String>,
    /// Cancelled by the next commit, to stop a run between two pages.
    cancel_on_commit: Option<CancellationToken>,
    state: ConversationMemoryState,
    transcript_revision: i64,
    committed: Vec<MemoryCommitCandidate>,
    operations: HashSet<String>,
    errors: Vec<String>,
    /// Bump the transcript revision just before the next commit, to make the
    /// compare-and-swap fire exactly once.
    conflict_next_commit: bool,
}

struct FakePort {
    inner: Mutex<PortState>,
}

impl FakePort {
    fn new(messages: Vec<SourceMessage>, items: Vec<MemoryItem>, watermark: i64) -> Arc<Self> {
        let mut state = ConversationMemoryState::empty(CONVERSATION);
        state.memory_revision = 3;
        state.processed_through_sequence = watermark;
        state.source_transcript_revision = 7;
        Arc::new(Self {
            inner: Mutex::new(PortState {
                messages,
                items,
                summary: None,
                summary_up_to: None,
                suppressions: Vec::new(),
                forgotten: Vec::new(),
                cancel_on_commit: None,
                state,
                transcript_revision: 7,
                committed: Vec::new(),
                operations: HashSet::new(),
                errors: Vec::new(),
                conflict_next_commit: false,
            }),
        })
    }

    fn conflict_once(&self) {
        self.inner.lock().conflict_next_commit = true;
    }

    fn cancel_on_commit(&self, token: CancellationToken) {
        self.inner.lock().cancel_on_commit = Some(token);
    }

    fn watermark(&self) -> i64 {
        self.inner.lock().state.processed_through_sequence
    }

    fn memory_revision(&self) -> i64 {
        self.inner.lock().state.memory_revision
    }

    fn commits(&self) -> usize {
        self.inner.lock().committed.len()
    }

    fn errors(&self) -> Vec<String> {
        self.inner.lock().errors.clone()
    }

    fn items(&self) -> Vec<MemoryItem> {
        self.inner.lock().items.clone()
    }

    fn item(&self, id: &str) -> Option<MemoryItem> {
        self.items().into_iter().find(|item| item.id.as_str() == id)
    }

    fn summary(&self) -> Option<String> {
        self.inner.lock().summary.clone()
    }

    /// A summary an earlier run folded through the message at `through`.
    fn summarized_through(&self, text: &str, through: i64) {
        let mut inner = self.inner.lock();
        let up_to = inner
            .messages
            .iter()
            .find(|message| message.sequence == through)
            .map(|message| message.id.clone())
            .expect("a message at the summary's end");
        inner.summary = Some(text.to_string());
        inner.summary_up_to = Some(up_to);
    }

    /// Sequence of the message the stored summary was folded through.
    fn summary_boundary(&self) -> Option<i64> {
        let inner = self.inner.lock();
        let up_to = inner.summary_up_to.as_ref()?;
        inner
            .messages
            .iter()
            .find(|message| &message.id == up_to)
            .map(|message| message.sequence)
    }

    /// The watermark every commit wrote, in order.
    fn committed_watermarks(&self) -> Vec<i64> {
        self.inner
            .lock()
            .committed
            .iter()
            .map(|candidate| candidate.commit.processed_through_sequence)
            .collect()
    }

    /// The operation label of every commit, in order.
    fn operations(&self) -> Vec<String> {
        self.inner
            .lock()
            .committed
            .iter()
            .map(|candidate| candidate.operation.clone())
            .collect()
    }

    /// Forget an item the way `knowledge.rs` does: its assertion spans become
    /// suppressions, it stops being active, the summary that may repeat it is
    /// cleared, and in-flight commits are invalidated.
    fn forget(&self, item_id: &str) {
        let mut inner = self.inner.lock();
        let item = inner
            .items
            .iter_mut()
            .find(|item| item.id.as_str() == item_id)
            .expect("the item to forget");
        item.state = MemoryState::Resolved;
        let label = item.label.clone();
        let spans: Vec<SuppressedSpan> = item
            .assertions()
            .map(|span| SuppressedSpan {
                message_id: span.message_id.clone(),
                start_byte: i64::from(span.start_byte),
                end_byte: i64::from(span.end_byte),
                content_digest: span.content_digest.clone(),
            })
            .collect();
        inner.suppressions.extend(spans);
        inner.forgotten.push(label);
        inner.summary = None;
        inner.summary_up_to = None;
        inner.state.memory_revision += 1;
    }

    /// Append a message as turn completion does: the transcript moves on.
    fn append(&self, message: SourceMessage) {
        let mut inner = self.inner.lock();
        inner.messages.push(message);
        inner.transcript_revision += 1;
    }

    /// Delete a message no extracted item cites, as the schema does it.
    ///
    /// `up_to_message_id` cascades, and `trg_conversation_memory_clamp_ad` drops
    /// a summary whose boundary is at or after the deleted message and clamps
    /// the watermark to the highest remaining sequence. The ledger stays
    /// `ready`: only a cited delete asks for a rebuild.
    fn delete_uncited(&self, id: &str) {
        let mut inner = self.inner.lock();
        assert!(
            !inner
                .items
                .iter()
                .any(|item| item.evidence.iter().any(|span| span.message_id == id)),
            "a cited delete rebuilds instead; this helper models the uncited path"
        );
        let index = inner
            .messages
            .iter()
            .position(|message| message.id == id)
            .expect("the message to delete");
        let boundary = inner.summary_up_to.as_ref().and_then(|up_to| {
            inner
                .messages
                .iter()
                .find(|message| &message.id == up_to)
                .map(|message| message.sequence)
        });
        let deleted = inner.messages.remove(index);
        if boundary.is_some_and(|boundary| deleted.sequence <= boundary) {
            inner.summary = None;
            inner.summary_up_to = None;
        }
        let highest = inner
            .messages
            .iter()
            .map(|message| message.sequence)
            .max()
            .unwrap_or(0);
        inner.state.processed_through_sequence =
            inner.state.processed_through_sequence.min(highest);
        inner.transcript_revision += 1;
    }
}

#[async_trait]
impl ConversationMemoryPort for FakePort {
    async fn load_snapshot(&self, _conversation_id: &str) -> Result<MemorySnapshot> {
        let inner = self.inner.lock();
        Ok(MemorySnapshot {
            state: inner.state.clone(),
            transcript_revision: inner.transcript_revision,
            active_items: inner
                .items
                .iter()
                .filter(|item| item.state == MemoryState::Active)
                .cloned()
                .collect(),
            summary: inner.summary.clone(),
            summary_through_sequence: inner.summary_up_to.as_ref().and_then(|up_to| {
                inner
                    .messages
                    .iter()
                    .find(|message| &message.id == up_to)
                    .map(|message| message.sequence)
            }),
            latest_sequence: inner
                .messages
                .iter()
                .map(|message| message.sequence)
                .max()
                .unwrap_or(0),
        })
    }

    async fn page_source_messages(
        &self,
        _conversation_id: &str,
        after_sequence: i64,
        through_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        let inner = self.inner.lock();
        let mut rows: Vec<SourceMessage> = inner
            .messages
            .iter()
            .filter(|message| {
                message.sequence > after_sequence && message.sequence <= through_sequence
            })
            .cloned()
            .collect();
        rows.sort_by_key(|message| message.sequence);

        let mut messages: Vec<SourceMessage> = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = false;
        for row in rows {
            if messages.len() >= limits.max_messages {
                truncated = true;
                break;
            }
            let size = row.content.len();
            if !messages.is_empty() && bytes + size > limits.max_bytes {
                truncated = true;
                break;
            }
            bytes += size;
            messages.push(row);
        }
        let next_after_sequence = messages
            .last()
            .map(|message| message.sequence)
            .unwrap_or(after_sequence);
        Ok(SourcePage {
            has_more: truncated || next_after_sequence < through_sequence,
            next_after_sequence,
            messages,
        })
    }

    async fn page_recent_source_messages(
        &self,
        _conversation_id: &str,
        after_sequence: i64,
        through_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage> {
        let inner = self.inner.lock();
        let mut rows: Vec<SourceMessage> = inner
            .messages
            .iter()
            .filter(|message| {
                message.sequence > after_sequence && message.sequence <= through_sequence
            })
            .cloned()
            .collect();
        rows.sort_by_key(|message| std::cmp::Reverse(message.sequence));

        let mut messages: Vec<SourceMessage> = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = false;
        for row in rows {
            if messages.len() >= limits.max_messages
                || (!messages.is_empty() && bytes + row.content.len() > limits.max_bytes)
            {
                truncated = true;
                break;
            }
            bytes += row.content.len();
            messages.push(row);
        }
        messages.reverse();
        Ok(SourcePage {
            messages,
            has_more: truncated,
            next_after_sequence: after_sequence,
        })
    }

    async fn read_source_spans(
        &self,
        _conversation_id: &str,
        spans: &[SourceSpanRef],
        _limits: SourceReadLimits,
    ) -> Result<Vec<ResolvedSpan>> {
        let inner = self.inner.lock();
        Ok(spans
            .iter()
            .map(|span| {
                let found = inner
                    .messages
                    .iter()
                    .find(|message| message.id == span.message_id);
                ResolvedSpan {
                    message_id: span.message_id.clone(),
                    sequence: found.map(|message| message.sequence).unwrap_or(0),
                    role: found
                        .map(|message| message.role)
                        .unwrap_or(SourceRole::User),
                    text: found.and_then(|message| {
                        message
                            .content
                            .get(span.start_byte as usize..span.end_byte as usize)
                            .map(str::to_string)
                    }),
                }
            })
            .collect())
    }

    async fn suppressed_spans(
        &self,
        _conversation_id: &str,
        message_ids: &[String],
    ) -> Result<Vec<SuppressedSpan>> {
        Ok(self
            .inner
            .lock()
            .suppressions
            .iter()
            .filter(|span| message_ids.contains(&span.message_id))
            .cloned()
            .collect())
    }

    async fn forgotten_labels(&self, _conversation_id: &str) -> Result<Vec<String>> {
        let mut labels: Vec<String> = Vec::new();
        for label in self.inner.lock().forgotten.iter().rev() {
            if !labels.contains(label) {
                labels.push(label.clone());
            }
        }
        Ok(labels)
    }

    async fn commit_memory(
        &self,
        preconditions: &MemoryCommitPreconditions,
        candidate: &MemoryCommitCandidate,
    ) -> std::result::Result<CommittedMemorySnapshot, MemoryCommitError> {
        let mut inner = self.inner.lock();
        if inner.operations.contains(&preconditions.operation_id) {
            return Ok(CommittedMemorySnapshot {
                memory_revision: inner.state.memory_revision,
                transcript_revision: inner.transcript_revision,
                processed_through_sequence: inner.state.processed_through_sequence,
                active_mandatory_count: 0,
                active_optional_count: 0,
                was_already_committed: true,
            });
        }
        if inner.conflict_next_commit {
            inner.conflict_next_commit = false;
            inner.transcript_revision += 1;
            return Err(MemoryCommitError::TranscriptConflict {
                expected: preconditions.expected_transcript_revision,
                actual: inner.transcript_revision,
            });
        }
        if preconditions.expected_transcript_revision != inner.transcript_revision {
            return Err(MemoryCommitError::TranscriptConflict {
                expected: preconditions.expected_transcript_revision,
                actual: inner.transcript_revision,
            });
        }
        if preconditions.expected_memory_revision != inner.state.memory_revision {
            return Err(MemoryCommitError::MemoryConflict {
                expected: preconditions.expected_memory_revision,
                actual: inner.state.memory_revision,
            });
        }

        for item in &candidate.commit.inserts {
            inner.items.push(item.clone());
        }
        for item in &candidate.commit.updates {
            if let Some(existing) = inner
                .items
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                *existing = item.clone();
            }
        }
        if let Some(summary) = &candidate.summary {
            inner.summary = Some(summary.summary_text.clone());
            inner.summary_up_to = Some(summary.up_to_message_id.clone());
        }
        inner.state.memory_revision += 1;
        inner.state.processed_through_sequence = candidate.commit.processed_through_sequence;
        inner.state.source_transcript_revision = candidate.transcript_revision;
        inner.state.validity = MemoryValidity::Ready;
        inner.state.last_error_code = None;
        inner.operations.insert(preconditions.operation_id.clone());
        inner.committed.push(candidate.clone());
        if let Some(token) = inner.cancel_on_commit.take() {
            token.cancel();
        }

        let mandatory = inner
            .items
            .iter()
            .filter(|item| item.is_mandatory_now())
            .count();
        let optional = inner
            .items
            .iter()
            .filter(|item| item.state == MemoryState::Active && !item.kind.is_mandatory())
            .count();
        Ok(CommittedMemorySnapshot {
            memory_revision: inner.state.memory_revision,
            transcript_revision: inner.transcript_revision,
            processed_through_sequence: inner.state.processed_through_sequence,
            active_mandatory_count: mandatory,
            active_optional_count: optional,
            was_already_committed: false,
        })
    }

    async fn record_memory_error(&self, _conversation_id: &str, code: &str) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.errors.push(code.to_string());
        inner.state.last_error_code = Some(code.to_string());
        Ok(())
    }

    async fn mark_rebuild_required(&self, _conversation_id: &str, code: &str) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.state.validity = MemoryValidity::RebuildRequired;
        inner.state.last_error_code = Some(code.to_string());
        inner.items.clear();
        inner.summary = None;
        inner.summary_up_to = None;
        Ok(())
    }

    async fn page_inactive_items(
        &self,
        _conversation_id: &str,
        _offset: i64,
        _limit: i64,
    ) -> Result<Vec<MemoryItem>> {
        Ok(self
            .items()
            .into_iter()
            .filter(|item| item.state != MemoryState::Active)
            .collect())
    }

    async fn load_state(&self, _conversation_id: &str) -> Result<ConversationMemoryState> {
        Ok(self.inner.lock().state.clone())
    }
}

// ---------------------------------------------------------------------------
// Fake utility model
// ---------------------------------------------------------------------------

/// Which prompt the job just sent. Dispatching on the task rather than call
/// order keeps a test readable when a step is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Extract,
    Repair,
    Review,
    Summarize,
    Shrink,
}

struct FakeLlm {
    extraction: Mutex<VecDeque<Result<String>>>,
    review: Mutex<VecDeque<Result<String>>>,
    summary: Mutex<VecDeque<Result<String>>>,
    calls: Mutex<Vec<Task>>,
    prompts: Mutex<Vec<String>>,
    request_limits: Mutex<Vec<(Option<String>, Option<u32>)>>,
    samplings: Mutex<Vec<Option<SamplingOverride>>>,
}

impl FakeLlm {
    fn new(
        extraction: Vec<Result<String>>,
        review: Vec<Result<String>>,
        summary: Vec<Result<String>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            extraction: Mutex::new(extraction.into()),
            review: Mutex::new(review.into()),
            summary: Mutex::new(summary.into()),
            calls: Mutex::new(Vec::new()),
            prompts: Mutex::new(Vec::new()),
            request_limits: Mutex::new(Vec::new()),
            samplings: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<Task> {
        self.calls.lock().clone()
    }

    fn count(&self, task: Task) -> usize {
        self.calls().iter().filter(|call| **call == task).count()
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().clone()
    }

    /// The prompts sent for one task, in order.
    fn prompts_for(&self, task: Task) -> Vec<String> {
        self.calls()
            .into_iter()
            .zip(self.prompts())
            .filter(|(call, _)| *call == task)
            .map(|(_, prompt)| prompt)
            .collect()
    }

    /// For each summariser call, the sequences it was asked to fold.
    fn folded_sequences(&self) -> Vec<Vec<i64>> {
        self.prompts_for(Task::Summarize)
            .iter()
            .map(|prompt| {
                let payload: serde_json::Value =
                    serde_json::from_str(prompt).expect("summary prompt JSON");
                payload["new_passages"]
                    .as_array()
                    .expect("summary passages")
                    .iter()
                    .map(|passage| passage["sequence"].as_i64().expect("passage sequence"))
                    .collect()
            })
            .collect()
    }

    /// For each summariser call, the text of the passages it was asked to fold.
    fn folded_texts(&self) -> Vec<String> {
        self.prompts_for(Task::Summarize)
            .iter()
            .map(|prompt| {
                let payload: serde_json::Value =
                    serde_json::from_str(prompt).expect("summary prompt JSON");
                payload["new_passages"]
                    .as_array()
                    .expect("summary passages")
                    .iter()
                    .filter_map(|passage| passage["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect()
    }

    /// For each summariser call, the facts it was told were forgotten.
    fn forgotten_labels(&self) -> Vec<Vec<String>> {
        self.prompts_for(Task::Summarize)
            .iter()
            .map(|prompt| {
                let payload: serde_json::Value =
                    serde_json::from_str(prompt).expect("summary prompt JSON");
                payload["forgotten_by_user"]
                    .as_array()
                    .map(|labels| {
                        labels
                            .iter()
                            .filter_map(|label| label.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    /// For each summariser call, the summary it was asked to extend.
    fn previous_summaries(&self) -> Vec<Option<String>> {
        self.prompts_for(Task::Summarize)
            .iter()
            .map(|prompt| {
                let payload: serde_json::Value =
                    serde_json::from_str(prompt).expect("summary prompt JSON");
                payload["previous_summary"].as_str().map(str::to_string)
            })
            .collect()
    }

    fn request_limits(&self) -> Vec<(Option<String>, Option<u32>)> {
        self.request_limits.lock().clone()
    }

    fn samplings(&self) -> Vec<Option<SamplingOverride>> {
        self.samplings.lock().clone()
    }

    fn reply(&self, prompt: &str) -> Result<String> {
        let parsed_task = serde_json::from_str::<serde_json::Value>(prompt)
            .ok()
            .and_then(|v| v.get("task").and_then(|t| t.as_str()).map(str::to_string));
        let task = match parsed_task.as_deref() {
            Some("extract_memory") => Task::Extract,
            Some("repair_memory_patch") => Task::Repair,
            Some("review_memory_changes") => Task::Review,
            Some("shrink_working_summary") => Task::Shrink,
            _ if prompt.contains("\"extract_memory\"") => Task::Extract,
            _ if prompt.contains("\"repair_memory_patch\"") => Task::Repair,
            _ if prompt.contains("\"review_memory_changes\"") => Task::Review,
            _ if prompt.contains("\"shrink_working_summary\"") => Task::Shrink,
            _ => Task::Summarize,
        };
        self.calls.lock().push(task);
        self.prompts.lock().push(prompt.to_string());
        let queue = match task {
            Task::Extract | Task::Repair => &self.extraction,
            Task::Review => &self.review,
            Task::Summarize | Task::Shrink => &self.summary,
        };
        queue.lock().pop_front().unwrap_or_else(|| {
            Err(AppError::Other(format!(
                "no scripted reply left for {task:?}"
            )))
        })
    }
}

#[async_trait]
impl LLMPort for FakeLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.request_limits
            .lock()
            .push((request.reasoning_effort.clone(), request.max_output_tokens));
        self.samplings.lock().push(request.sampling);
        let prompt = request
            .input
            .iter()
            .filter_map(|input| match input {
                lattice_core::application::ports::llm_port::CompletionInput::Message {
                    role,
                    content,
                } if role == "user" => Some(content.clone()),
                _ => None,
            })
            .next_back()
            .unwrap_or_default();
        Ok(CompletionResponse {
            text: self.reply(&prompt)?,
            ..Default::default()
        })
    }

    fn model_name(&self) -> &str {
        "fake-utility"
    }

    fn max_context_tokens(&self) -> usize {
        8_192
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Words, so a test can set a token budget it can reason about.
fn word_counter() -> super::TokenCounter {
    Arc::new(|text: &str| text.split_whitespace().count())
}

fn job(port: Arc<FakePort>, llm: Arc<FakeLlm>, config: CompactionConfig) -> CompactionJob {
    CompactionJob::new(port, llm, word_counter(), config)
}

fn small_config() -> CompactionConfig {
    CompactionConfig {
        keep_recent_messages: 2,
        ..Default::default()
    }
}

/// The segment ids for a prefix of whole messages.
const PREFIX_SEGMENTS: [&str; 4] = ["m1:s0", "m2:s0", "m3:s0", "m4:s0"];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn the_enforced_limits_are_the_ones_the_design_documents() {
    assert_eq!(MAX_REPAIR_ATTEMPTS, 2);
    assert_eq!(MAX_COMMIT_CONFLICT_RETRIES, 1);
    assert_eq!(MAX_SOURCE_MESSAGES_PER_ATTEMPT, 256);
    assert_eq!(MAX_SOURCE_BYTES_PER_ATTEMPT, 1024 * 1024);
    assert_eq!(MIN_RETAINED_TURNS, 2);

    // The payload caps are the domain's, reused rather than restated: two
    // numbers that must agree are one number that cannot disagree.
    assert_eq!(MAX_PROPOSAL_BYTES, 64 * 1024);
    assert_eq!(MAX_OPERATIONS_PER_RESPONSE, 32);
    assert_eq!(MAX_EVIDENCE_PER_ITEM, 4);
    assert_eq!(MAX_QUOTE_BYTES, 2048);

    let config = CompactionConfig::default();
    assert_eq!(
        config.deadline, None,
        "memory runs end by cancellation, not a clock"
    );
    assert_eq!(
        config.max_source_messages_per_attempt,
        MAX_SOURCE_MESSAGES_PER_ATTEMPT
    );
    assert_eq!(
        config.max_source_bytes_per_attempt,
        MAX_SOURCE_BYTES_PER_ATTEMPT
    );
}

#[test]
fn the_extractor_prompt_states_every_rule_the_design_requires() {
    // §15's bullets, each pinned by the phrase that carries it. A future edit
    // that drops one of these is dropping a rule, not a sentence.
    for required in [
        "requirements, goals, decisions, facts, preferences",
        "mandatory kind for a hard requirement",
        "never paraphrase",
        "zero-based match index",
        "quoted third-party material",
        "Do not add a completed one-shot command",
        "Never classify an action request as a decision",
        "does not turn details chosen or reported only by the assistant",
        "Praise or feedback",
        "an item you do not mention stays exactly as it is",
        "Never collapse contradictory earlier and later statements",
        "Preserve negation",
        "never infer that approval of a draft",
        "untrusted data",
        "Return every segment id",
    ] {
        assert!(
            super::prompts::EXTRACTOR_SYSTEM.contains(required),
            "extractor prompt no longer says: {required}"
        );
    }
    for required in [
        "surrounding original text",
        "\"supported\", \"ambiguous\", or \"unsupported\"",
        "user fact, preference, or unresolved question",
        "same subject",
        "presents conflicting alternatives",
        "preserve the uncertainty",
        "Never treat an assistant message",
    ] {
        assert!(
            super::prompts::VERIFIER_SYSTEM.contains(required),
            "verifier prompt no longer says: {required}"
        );
    }
    // Both calls send their schema, so the server holds the shape. Asking for
    // the format in the prompt as well is what costs a small model accuracy.
    for system in [
        super::prompts::EXTRACTOR_SYSTEM,
        super::prompts::VERIFIER_SYSTEM,
    ] {
        for demand in [
            "Return only JSON",
            "matching the requested schema",
            "markdown",
        ] {
            assert!(
                !system.contains(demand),
                "prompt still demands a format: {demand}"
            );
        }
    }
    for required in [
        "attempts that failed",
        "stored separately",
        "unrequested assistant proposal",
        "explicitly adopts that proposal",
        "what was denied or refused",
        "Respect the requested budget",
        "fallible",
    ] {
        assert!(
            super::prompts::SUMMARIZER_SYSTEM.contains(required),
            "summariser prompt no longer says: {required}"
        );
    }

    let schema = super::prompts::patch_schema();
    assert_eq!(
        schema.pointer("/properties/add/items/properties/evidence/items/properties/purpose/enum"),
        Some(&serde_json::json!(["assertion", "antecedent"])),
        "typed additions may use assistant antecedent context but never transition evidence"
    );
    assert_eq!(
        schema.pointer(
            "/properties/transitions/items/properties/evidence/items/properties/purpose/enum"
        ),
        Some(&serde_json::json!(["transition"])),
        "typed transitions must not advertise assertion evidence as valid"
    );
}

/// Memory payloads go to the model as compact JSON with sorted keys:
/// indentation would spend window tokens on whitespace in every call.
#[test]
fn memory_payloads_are_compact_json() {
    let payload = super::prompts::render_summary_shrink_prompt(
        "Zellteilung:\n- Mitose läuft in vier Phasen ab.",
        120,
        80,
    );
    assert!(!payload.contains('\n'), "{payload}");
    assert!(
        payload.starts_with("{\"budget_tokens\":80,\"instructions\":"),
        "{payload}"
    );
    assert!(
        payload.contains("\"task\":\"shrink_working_summary\""),
        "{payload}"
    );
}

/// llama.cpp throws away the whole grammar when any `maxLength` or `maxItems`
/// exceeds its repetition threshold, so no schema bound may.
#[test]
fn every_schema_bound_stays_within_the_llama_cpp_grammar_repetition_limit() {
    for (name, schema) in [
        ("patch", super::prompts::patch_schema()),
        ("verdict", super::prompts::verdict_schema()),
        (
            "turn intent",
            crate::features::conversation::chat::intent::intent_schema(),
        ),
    ] {
        let (bound, path) = super::prompts::largest_repetition_bound(&schema);
        assert!(
            bound <= super::prompts::GRAMMAR_REPETITION_LIMIT as u64,
            "{name} schema bound {bound} at {path} exceeds llama.cpp's grammar limit of {}",
            super::prompts::GRAMMAR_REPETITION_LIMIT
        );
    }
}

fn object_keys(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// llama.cpp builds the grammar in schema order. Sorted keys ask for an item's
/// evidence before its kind and label and for a reason before its verdict;
/// the workspace's `preserve_order` must not turn that around.
#[test]
fn memory_grammars_ask_for_the_grounds_before_the_decision() {
    let patch = super::prompts::patch_schema();
    assert_eq!(
        object_keys(patch.pointer("/properties")),
        [
            "add",
            "processed_segment_ids",
            "schema_version",
            "transitions"
        ]
    );
    assert_eq!(
        object_keys(patch.pointer("/properties/add/items/properties")),
        ["candidate_id", "evidence", "kind", "label"]
    );
    let verdict = super::prompts::verdict_schema();
    assert_eq!(
        object_keys(verdict.pointer("/properties/verdicts/items/properties")),
        ["id", "reason", "verdict"]
    );
}

/// The passages to extract are the last thing the extractor reads before it
/// answers, after the coverage manifest and the rules.
#[test]
fn the_extraction_request_ends_with_the_passages_to_extract() {
    let source = message(
        "m1",
        1,
        SourceRole::User,
        "Never deploy to production on a Friday.",
    );
    let segments = segment_message(&source, 512, 32);
    let rendered = super::prompts::render_extractor_prompt(&segments, &[], &[]);
    let payload: serde_json::Value =
        serde_json::from_str(&rendered).expect("extraction payload JSON");

    let keys = object_keys(Some(&payload));
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "payload keys must stay in sorted order");
    assert!(
        keys.ends_with(&["segments".to_string(), "task".to_string()]),
        "segments must come last before the task name: {keys:?}"
    );
    let segment_keys = object_keys(payload.pointer("/segments/0"));
    let mut sorted_segment_keys = segment_keys.clone();
    sorted_segment_keys.sort_unstable();
    assert_eq!(segment_keys, sorted_segment_keys);
}

#[test]
fn a_repair_request_resends_the_original_passages_instead_of_asking_the_model_to_remember_them() {
    let original = r#"{"segments":[{"message_id":"m1","text":"Use port 8443."}]}"#;
    let rendered = super::prompts::render_repair_prompt(
        "missing segment id",
        "incomplete_coverage",
        &["m1"],
        &["m1:s0".to_string()],
        original,
        r#"{"processed_segment_ids":[]}"#,
    );
    let payload: serde_json::Value = serde_json::from_str(&rendered).expect("repair payload JSON");

    assert_eq!(payload["original_extraction_request"], original);
    assert!(payload["instructions"]
        .as_str()
        .is_some_and(|text| text.contains("does not require proposing an addition")));
}

#[test]
fn primary_segments_tile_a_long_message_with_no_gaps_or_overlap() {
    let body = (0..400)
        .map(|index| format!("Paragraph {index} about the café build → prod.\n\n"))
        .collect::<String>();
    let source = message("big", 1, SourceRole::User, &body);

    let segments = segment_message(&source, 512, 32);
    assert!(segments.len() > 1, "a long message should split");
    assert!(segments.iter().all(|segment| !segment.whole_message));

    let rebuilt: String = segments
        .iter()
        .map(|segment| segment.text.clone())
        .collect();
    assert_eq!(rebuilt, body, "primary segments must tile the message");

    let mut expected_start = 0usize;
    for segment in &segments {
        assert_eq!(segment.start_byte, expected_start);
        assert!(segment.end_byte > segment.start_byte);
        expected_start = segment.end_byte;
    }
    assert_eq!(expected_start, body.len());
    assert!(segments
        .iter()
        .any(|segment| !segment.leading_overlap.is_empty()));
}

#[test]
fn the_pending_user_message_and_two_complete_turns_stay_out_of_the_prefix() {
    let mut messages = four_turns();
    messages.push(message("m9", 9, SourceRole::User, "And now?"));

    let cut = select_boundary(&messages, true, 1).expect("something to compact");
    let prefix = &messages[..cut];
    assert!(
        prefix.iter().all(|message| message.sequence <= 4),
        "the last two turns plus the pending message must stay raw"
    );
    // The cut lands on a turn boundary: the message after it starts a turn.
    assert_eq!(messages[cut].role, SourceRole::User);

    // Only recent turns: nothing to compact rather than splitting a turn.
    let short = vec![
        message("s1", 1, SourceRole::User, "Hello"),
        message("s2", 2, SourceRole::Assistant, "Hi"),
        message("s3", 3, SourceRole::User, "Again"),
    ];
    assert_eq!(select_boundary(&short, true, 1), None);
}

#[test]
fn a_window_the_work_cap_cut_short_still_keeps_the_recent_turns_when_it_can() {
    let messages = four_turns();

    // The cap may stop only a message or two from the end, so the retention rule
    // is applied to a capped window too: the last two turns stay raw.
    assert_eq!(select_boundary(&messages, false, 2), Some(4));

    // A window smaller than the tail it would have to retain still advances, on
    // a turn boundary. Retaining everything would never move the watermark.
    assert_eq!(select_boundary(&messages[..4], false, 2), Some(2));
}

#[tokio::test]
async fn a_timeout_or_cancellation_leaves_the_committed_state_usable_and_the_watermark_unmoved() {
    let messages = four_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![committed_constraint("item-old", &messages[0])],
        0,
    );
    let llm = FakeLlm::new(vec![], vec![], vec![]);

    let expired = job(
        port.clone(),
        llm.clone(),
        CompactionConfig {
            deadline: Some(Duration::ZERO),
            ..small_config()
        },
    );
    let error = expired
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an expired deadline cannot compact");
    assert!(error.to_string().contains("deadline"), "{error}");

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let running = job(port.clone(), llm.clone(), small_config());
    running
        .run(
            CONVERSATION,
            CompactionRequest {
                cancellation: Some(cancelled),
                ..Default::default()
            },
        )
        .await
        .expect_err("a cancelled job cannot compact");

    assert_eq!(
        llm.calls(),
        Vec::new(),
        "no model call should have happened"
    );
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0, "the watermark must not move");
    assert_eq!(port.memory_revision(), 3);
    assert_eq!(
        port.item("item-old").map(|item| item.state),
        Some(MemoryState::Active),
        "the previously committed constraint must still be usable"
    );
    assert_eq!(
        port.errors(),
        vec!["deadline_exceeded".to_string()],
        "a cancellation is a user decision, not a recorded fault"
    );
}

#[tokio::test]
async fn invalid_json_exhausts_bounded_repairs_and_commits_nothing() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![
            Ok("I could not produce JSON, sorry.".to_string()),
            Ok("{ still not valid".to_string()),
            Ok("{ still not valid either".to_string()),
        ],
        vec![],
        vec![],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let error = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an unparseable patch cannot be committed");
    assert!(error.to_string().contains("rejected"), "{error}");

    assert_eq!(llm.count(Task::Extract), 1);
    assert_eq!(llm.count(Task::Repair), 2, "exactly two repair attempts");
    assert_eq!(
        llm.count(Task::Summarize),
        0,
        "no summary for a rejected pass"
    );
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0);
    assert_eq!(port.errors(), vec!["malformed_json".to_string()]);
}

#[tokio::test]
async fn a_repair_that_validates_is_committed_and_reported_as_repaired() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![
            Ok("nonsense".to_string()),
            Ok(extraction_response(
                vec![proposed_constraint(
                    "c1",
                    "Deployment requires approval",
                    "m1",
                    "Do not deploy until I approve.",
                )],
                vec![],
                &PREFIX_SEGMENTS,
            )),
        ],
        vec![Ok(verdict_response(&[("addition:c1", "supported")]))],
        vec![Ok("Approval gate discussed; staging allowed.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert_eq!(outcome.resume_after_sequence, None);
    assert_eq!(outcome.repaired_batches, 1);
    assert_eq!(outcome.extraction_calls, 2);
    assert_eq!(outcome.commit.inserts.len(), 1);
    assert_eq!(outcome.commit.inserts[0].review, MemoryReview::Supported);
    assert_eq!(outcome.processed_through_sequence, 4);
    assert_eq!(
        llm.request_limits(),
        vec![
            (None, Some(8_192)),
            (None, Some(8_192)),
            (None, Some(4_096)),
            (None, Some(8_192)),
        ],
        "structured utility requests keep model-default reasoning and the review output budget"
    );
    assert!(
        llm.samplings()
            .iter()
            .all(|sampling| *sampling == Some(SamplingOverride::deterministic())),
        "every utility request decodes greedily: the same passages must yield the same patch, \
         not one that depends on the chat temperature"
    );
    assert_eq!(port.commits(), 1);
    assert_eq!(port.watermark(), 4);
}

#[tokio::test]
async fn a_response_that_omits_an_existing_active_item_leaves_that_item_active() {
    let messages = four_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![committed_constraint("item-old", &messages[0])],
        0,
    );
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        vec![Ok("Nothing new to record.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert!(
        outcome.commit.updates.is_empty() && outcome.commit.inserts.is_empty(),
        "an empty patch must not change the ledger"
    );
    assert_eq!(llm.count(Task::Review), 0, "nothing to review");
    assert_eq!(
        outcome.status,
        CompactionStatus::Committed,
        "keeping the recent tail raw is the design, not unfinished work"
    );
    let item = port.item("item-old").expect("the item must still exist");
    assert_eq!(item.state, MemoryState::Active);
    assert_eq!(item.revision, 1, "an untouched item is not rewritten");
    assert_eq!(port.watermark(), 4, "coverage still advances");
}

#[tokio::test]
async fn an_unsupported_retirement_is_dropped_without_stalling_source_coverage() {
    let messages = four_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![committed_constraint("item-old", &messages[0])],
        0,
    );
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![],
            vec![proposed_transition(
                "item-old",
                "m3",
                "Actually you may deploy staging.",
            )],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[(
            "transition:item-old",
            "unsupported",
        )]))],
        vec![Ok("Staging exception raised but unresolved.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("a confidently unsupported transition should be ignored");

    assert_eq!(
        llm.count(Task::Review),
        1,
        "a transition is always reviewed"
    );
    assert!(outcome.commit.updates.is_empty());
    assert!(outcome.commit.inserts.is_empty());
    assert_eq!(port.commits(), 1, "the reviewed source is still covered");
    assert_eq!(port.watermark(), 4);

    let old = port.item("item-old").expect("the restriction must survive");
    assert_eq!(old.state, MemoryState::Active);
    assert_eq!(old.superseded_by, None);
}

#[tokio::test]
async fn an_unsupported_addition_is_filtered_without_poisoning_supported_memory() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![
                proposed_constraint(
                    "c1",
                    "Deployment requires approval",
                    "m1",
                    "Do not deploy until I approve.",
                ),
                proposed_item(
                    "c2",
                    "preference",
                    "Production deploys are forbidden outright",
                    "m3",
                    "Actually you may deploy staging.",
                ),
            ],
            vec![],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[
            ("addition:c1", "supported"),
            ("addition:c2", "unsupported"),
        ]))],
        vec![Ok("Approval gate discussed; staging allowed.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("supported memory should survive an unrelated rejected proposal");

    assert_eq!(llm.count(Task::Summarize), 1);
    assert_eq!(outcome.commit.inserts.len(), 1);
    assert_eq!(
        outcome.commit.inserts[0].label,
        "Deployment requires approval"
    );
    assert_eq!(port.commits(), 1);
    assert_eq!(port.watermark(), 4);
    assert!(port.errors().is_empty());
}

#[tokio::test]
async fn an_unreviewed_optional_addition_is_not_committed_as_ambiguous_memory() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![proposed_item(
                "c1",
                "preference",
                "The user approved the draft",
                "m3",
                "Actually you may deploy staging.",
            )],
            vec![],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[]))],
        vec![],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let error = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an optional addition without a reviewer verdict must stay unprocessed");

    assert!(
        error.to_string().contains("ambiguous_optional_addition"),
        "{error}"
    );
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0);
}

/// What extraction accepted does not depend on the summary: a fold that will
/// not fit its pool after one regeneration leaves the summary as it was, and
/// the extraction commits.
#[tokio::test]
async fn an_oversized_summary_is_regenerated_once_then_left_as_it_was_while_extraction_commits() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let bloated = (0..80)
        .map(|index| format!("word{index} "))
        .collect::<String>();
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        vec![Ok(bloated.clone()), Ok(bloated)],
    );
    let job = job(
        port.clone(),
        llm.clone(),
        CompactionConfig {
            summary_token_budget: 10,
            ..small_config()
        },
    );

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("the extraction commits without the summary");

    assert_eq!(llm.count(Task::Summarize), 1);
    assert_eq!(llm.count(Task::Shrink), 1, "exactly one regeneration");
    assert_eq!(
        outcome.commit.summary, None,
        "the oversized summary is not committed"
    );
    assert_eq!(port.commits(), 1, "the extraction is");
    assert_eq!(port.watermark(), outcome.processed_through_sequence);
    assert!(outcome.processed_through_sequence > 0);
}

#[tokio::test]
async fn a_summary_that_fits_after_one_regeneration_is_committed() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        vec![
            Ok((0..40).map(|i| format!("word{i} ")).collect::<String>()),
            Ok("Approval gate; staging allowed.".to_string()),
        ],
    );
    let job = job(
        port.clone(),
        llm.clone(),
        CompactionConfig {
            summary_token_budget: 10,
            ..small_config()
        },
    );

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();
    assert!(outcome.summary_regenerated);
    assert_eq!(outcome.summary_tokens, 4);
    assert_eq!(port.commits(), 1);
}

#[tokio::test]
async fn a_response_echoing_fewer_segment_ids_than_submitted_is_rejected_as_incomplete() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let short = extraction_response(vec![], vec![], &["m1:s0"]);
    let llm = FakeLlm::new(
        vec![Ok(short.clone()), Ok(short.clone()), Ok(short)],
        vec![],
        vec![],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    job.run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an incomplete coverage report cannot be committed");

    assert_eq!(llm.count(Task::Repair), 2);
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0);
    assert_eq!(port.errors(), vec!["incomplete_coverage".to_string()]);
}

#[tokio::test]
async fn the_work_cap_returns_resumable_progress_rather_than_skipping_source() {
    let messages = four_turns();
    let latest = 8;
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &["m1:s0", "m2:s0"]))],
        vec![],
        vec![Ok("Approval gate recorded.".to_string())],
    );
    let job = job(
        port.clone(),
        llm.clone(),
        CompactionConfig {
            max_source_messages_per_attempt: 4,
            ..small_config()
        },
    );

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.status, CompactionStatus::Partial);
    assert_eq!(outcome.source_messages_read, 4, "the cap bounded the read");
    assert_eq!(outcome.processed_through_sequence, 2);
    assert_eq!(
        outcome.resume_after_sequence,
        Some(2),
        "the remaining source is resumable, not skipped"
    );
    assert!(outcome.processed_through_sequence < latest);
    assert_eq!(port.watermark(), 2);
}

#[tokio::test]
async fn a_commit_conflict_is_retried_once_from_a_fresh_snapshot() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    port.conflict_once();
    let response = extraction_response(vec![], vec![], &PREFIX_SEGMENTS);
    let llm = FakeLlm::new(
        vec![Ok(response.clone()), Ok(response)],
        vec![],
        vec![
            Ok("First attempt summary.".to_string()),
            Ok("Second attempt summary.".to_string()),
        ],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.commit_conflict_retries, 1);
    assert_eq!(llm.count(Task::Extract), 2, "the retry re-reads the source");
    assert_eq!(port.commits(), 1, "only the second attempt published");
    assert_eq!(port.watermark(), 4);
    assert_eq!(port.errors(), Vec::<String>::new());
}

#[tokio::test]
async fn a_review_that_cannot_be_obtained_records_ambiguity_rather_than_support() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![proposed_constraint(
                "c1",
                "Deployment requires approval",
                "m1",
                "Do not deploy until I approve.",
            )],
            vec![],
            &PREFIX_SEGMENTS,
        ))],
        vec![Err(AppError::ServiceNotAvailable("no model".into()))],
        vec![Ok("Approval gate recorded.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert!(outcome.review_degraded);
    assert_eq!(outcome.commit.inserts.len(), 1, "the requirement is kept");
    assert_eq!(
        outcome.commit.inserts[0].review,
        MemoryReview::Ambiguous,
        "an unreviewed mandatory item is carried as ambiguous, not supported"
    );
}

#[tokio::test]
async fn a_failed_assistant_output_is_not_offered_as_extraction_source() {
    let mut messages = four_turns();
    messages[1].status = "failed".to_string();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![],
            vec![],
            &["m1:s0", "m3:s0", "m4:s0"],
        ))],
        vec![],
        vec![Ok("Approval gate recorded.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(
        outcome.segments_submitted, 3,
        "the failed assistant output is not a segment"
    );
    let prompt = llm
        .prompts()
        .into_iter()
        .next()
        .expect("an extraction prompt");
    assert!(!prompt.contains("m2:s0"));
    assert_eq!(
        outcome.processed_through_sequence, 4,
        "it is still covered by the watermark, not skipped forever"
    );
}

#[test]
fn clipping_reports_that_a_passage_continues() {
    assert_eq!(clip_chars("café build", 4), ("café", true));
    assert_eq!(clip_chars("café", 8), ("café", false));
}

/// A guard on the fake itself: a test that depends on the compare-and-swap is
/// only meaningful if the fake actually performs it.
#[tokio::test]
async fn the_fake_port_enforces_the_revision_compare_and_swap() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let candidate = MemoryCommitCandidate {
        commit: Default::default(),
        summary: None,
        source_message_ids: Vec::new(),
        transcript_revision: 7,
        extractor_model_identity: None,
        extractor_prompt_version: None,
        validator_version: None,
        operation: "compact".into(),
    };
    let stale = MemoryCommitPreconditions {
        conversation_id: CONVERSATION.into(),
        expected_transcript_revision: 6,
        expected_memory_revision: 3,
        operation_id: "op-1".into(),
    };
    let error = port
        .commit_memory(&stale, &candidate)
        .await
        .expect_err("a stale transcript revision must be refused");
    assert!(error.is_retryable());

    let mismatched = MemoryCommitPreconditions {
        expected_transcript_revision: 7,
        expected_memory_revision: 2,
        ..stale
    };
    let error = port
        .commit_memory(&mismatched, &candidate)
        .await
        .expect_err("a stale memory revision must be refused");
    assert_eq!(error.code(), "memory_conflict");
}

#[tokio::test]
async fn maintenance_consolidates_the_first_completed_turn_without_waiting_for_overflow() {
    let messages = four_turns()[..2].to_vec();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![proposed_constraint(
                "c1",
                "Approval required",
                "m1",
                "Do not deploy until I approve.",
            )],
            vec![],
            &["m1:s0", "m2:s0"],
        ))],
        vec![Ok(verdict_response(&[("addition:c1", "supported")]))],
        vec![Ok("Deployment discussed.".into())],
    );
    let outcome = job(port.clone(), llm, small_config())
        .run(
            CONVERSATION,
            CompactionRequest {
                trigger: super::CompactionTrigger::Maintenance,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert_eq!(port.watermark(), 2);
    assert_eq!(outcome.commit.inserts.len(), 1);
}

// ---------------------------------------------------------------------------
// A summary a delete dropped is rebuilt, not abandoned
// ---------------------------------------------------------------------------

fn maintenance() -> CompactionRequest {
    CompactionRequest {
        trigger: super::CompactionTrigger::Maintenance,
        ..Default::default()
    }
}

/// Consolidation after a turn whose window had to start after `through`, so
/// its prompt asked for the summary to cover through there.
fn refolding(through: i64) -> CompactionRequest {
    CompactionRequest {
        summary_refold: Some(SummaryRefold {
            window_start_sequence: through + 1,
            through_sequence: through,
        }),
        ..maintenance()
    }
}

/// Regenerate takes back the last answer and the summary that ended on it,
/// while the clamped watermark still covers the earlier turn: the next fold
/// folds that turn back in.
#[tokio::test]
async fn a_regenerated_answer_refolds_the_turns_the_dropped_summary_covered() {
    let port = FakePort::new(four_turns()[..4].to_vec(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![
            Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS)),
            Ok(extraction_response(vec![], vec![], &["m4b:s0"])),
        ],
        vec![],
        vec![
            Ok("Approval gate; staging allowed.".to_string()),
            Ok("Approval gate; staging only, production blocked.".to_string()),
        ],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    job.run(CONVERSATION, refolding(4)).await.unwrap();
    assert_eq!(port.watermark(), 4);
    assert_eq!(
        port.summary_boundary(),
        Some(4),
        "the refold the prompt asked for ends the summary on the latest answer"
    );

    // Regenerate: the answer goes, and nothing in the ledger cites it.
    port.delete_uncited("m4");
    assert_eq!(port.summary(), None, "the summary went with its boundary");
    assert_eq!(port.watermark(), 3);
    port.append(message(
        "m4b",
        5,
        SourceRole::Assistant,
        "Noted again: staging only.",
    ));

    let outcome = job.run(CONVERSATION, refolding(5)).await.unwrap();

    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert_eq!(
        llm.folded_sequences().last().cloned(),
        Some(vec![1, 2, 3, 5]),
        "the turn below the watermark is folded back in, not only the new answer"
    );
    assert_eq!(
        llm.previous_summaries().last().cloned(),
        Some(None),
        "nothing of the dropped summary is reused"
    );
    // Only the new answer is extracted: the ledger already covers the rest.
    assert_eq!(llm.count(Task::Extract), 2);
    let second = llm.prompts_for(Task::Extract).pop().unwrap();
    assert!(second.contains("m4b:s0"));
    for covered in ["m1:s0", "m2:s0", "m3:s0"] {
        assert!(
            !second.contains(covered),
            "{covered} must not be re-extracted"
        );
    }
    assert_eq!(port.watermark(), 5);
    assert_eq!(
        port.summary().as_deref(),
        Some("Approval gate; staging only, production blocked.")
    );
    assert_eq!(port.summary_boundary(), Some(5));
}

/// Deleting an early message drops the summary that covered it, since it may
/// repeat the deleted text, but leaves the ledger and the watermark standing.
/// The next fold, even with no new turn, rebuilds the summary from the
/// messages that remain: a `/compact` here, or the consolidation after a turn
/// whose window asks for one.
#[tokio::test]
async fn deleting_an_early_message_refolds_the_summary_from_the_messages_that_remain() {
    let port = FakePort::new(four_turns()[..4].to_vec(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        vec![
            Ok("Approval gate; staging allowed.".to_string()),
            Ok("Staging allowed.".to_string()),
        ],
    );
    let job = job(port.clone(), llm.clone(), small_config());
    job.run(CONVERSATION, refolding(4)).await.unwrap();

    port.delete_uncited("m1");
    assert_eq!(port.summary(), None);
    assert_eq!(port.watermark(), 4, "the ledger still covers what remains");

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert_eq!(
        llm.folded_sequences().last().cloned(),
        Some(vec![2, 3, 4]),
        "every remaining message the dropped summary covered is folded again"
    );
    assert_eq!(llm.count(Task::Extract), 1, "nothing is re-extracted");
    assert_eq!(outcome.extraction_calls, 0);
    assert!(outcome.commit.inserts.is_empty() && outcome.commit.updates.is_empty());
    assert_eq!(port.watermark(), 4, "the watermark does not move");
    assert_eq!(port.summary().as_deref(), Some("Staging allowed."));
    assert_eq!(port.summary_boundary(), Some(4));
    assert_eq!(outcome.boundary_message_id.as_deref(), Some("m4"));
}

/// A run can commit no summary when nothing it covered was eligible. A
/// conversation left that way has nothing to refold, and must not pay for a
/// model call each time it is asked to compact.
#[tokio::test]
async fn a_watermark_over_only_failed_outputs_does_not_refold() {
    let mut failed = vec![
        message(
            "f1",
            1,
            SourceRole::Assistant,
            "Partial answer that errored.",
        ),
        message("f2", 2, SourceRole::Assistant, "Another failed attempt."),
    ];
    for output in &mut failed {
        output.status = "failed".to_string();
    }
    let port = FakePort::new(failed, Vec::new(), 2);
    let llm = FakeLlm::new(vec![], vec![], vec![]);
    let job = job(port.clone(), llm.clone(), small_config());

    for request in [CompactionRequest::default(), maintenance()] {
        let outcome = job.run(CONVERSATION, request).await.unwrap();
        assert_eq!(outcome.status, CompactionStatus::NothingToCompact);
    }

    assert_eq!(
        llm.calls(),
        Vec::new(),
        "nothing eligible means no model call"
    );
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 2);
    assert_eq!(port.summary(), None);
}

/// A summary that ends at the watermark is extended: a later fold reads only
/// what is new, and a run with nothing new does nothing at all.
#[tokio::test]
async fn a_summary_that_reaches_the_watermark_is_extended_never_refolded() {
    let port = FakePort::new(four_turns()[..4].to_vec(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![
            Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS)),
            Ok(extraction_response(vec![], vec![], &["m5:s0", "m6:s0"])),
        ],
        vec![],
        vec![
            Ok("Approval gate; staging allowed.".to_string()),
            Ok("Approval gate; staging allowed; production blocked.".to_string()),
        ],
    );
    let job = job(port.clone(), llm.clone(), small_config());
    job.run(CONVERSATION, refolding(4)).await.unwrap();

    let idle = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();
    assert_eq!(idle.status, CompactionStatus::NothingToCompact);
    assert_eq!(llm.calls().len(), 2, "no call for a run with nothing new");

    let turns = four_turns();
    port.append(turns[4].clone());
    port.append(turns[5].clone());
    job.run(CONVERSATION, refolding(6)).await.unwrap();

    assert_eq!(
        llm.folded_sequences(),
        vec![vec![1, 2, 3, 4], vec![5, 6]],
        "the second fold carries the summary forward instead of rereading"
    );
    assert_eq!(
        llm.previous_summaries().last().cloned(),
        Some(Some("Approval gate; staging allowed.".to_string()))
    );
    assert_eq!(port.summary_boundary(), Some(6));
}

/// `count` alternating user and assistant messages of exactly ten bytes each,
/// so a refold byte budget maps onto a message count.
fn uniform_messages(count: i64) -> Vec<SourceMessage> {
    (1..=count)
        .map(|sequence| {
            let role = if sequence % 2 == 1 {
                SourceRole::User
            } else {
                SourceRole::Assistant
            };
            message(
                &format!("u{sequence}"),
                sequence,
                role,
                &format!("Message {sequence:02}"),
            )
        })
        .collect()
}

/// Two uniform messages per refold.
fn two_message_refolds() -> CompactionConfig {
    CompactionConfig {
        max_refold_bytes_per_attempt: 20,
        ..small_config()
    }
}

fn summaries(count: usize) -> Vec<Result<String>> {
    (1..=count)
        .map(|index| Ok(format!("Summary {index}.")))
        .collect()
}

/// A refold larger than one attempt's budget is caught up across runs when the
/// run has someone waiting on it: a `/compact` takes one page. Each run ends the
/// summary on the last message it folded, which is where the next one resumes,
/// and the extracted ledger is never offered again.
#[tokio::test]
async fn a_long_refold_resumes_from_the_summary_boundary_across_runs() {
    // The ledger covers all eight; a delete or a correction took the summary.
    let port = FakePort::new(uniform_messages(8), Vec::new(), 8);
    let llm = FakeLlm::new(vec![], vec![], summaries(4));
    let job = job(port.clone(), llm.clone(), two_message_refolds());

    let mut statuses = Vec::new();
    for _ in 0..4 {
        let outcome = job
            .run(CONVERSATION, CompactionRequest::default())
            .await
            .unwrap();
        statuses.push(outcome.status);
        assert_eq!(outcome.processed_through_sequence, 8, "the watermark stays");
    }

    assert_eq!(
        statuses,
        vec![
            CompactionStatus::Partial,
            CompactionStatus::Partial,
            CompactionStatus::Partial,
            CompactionStatus::Committed,
        ]
    );
    assert_eq!(
        llm.folded_sequences(),
        vec![vec![1, 2], vec![3, 4], vec![5, 6], vec![7, 8]],
        "oldest first, each message folded exactly once"
    );
    assert_eq!(
        llm.previous_summaries(),
        vec![
            None,
            Some("Summary 1.".to_string()),
            Some("Summary 2.".to_string()),
            Some("Summary 3.".to_string()),
        ],
        "each run carries the partial summary forward"
    );
    assert_eq!(llm.count(Task::Extract), 0, "nothing is re-extracted");
    assert_eq!(port.summary_boundary(), Some(8));
    assert!(
        llm.forgotten_labels().iter().all(Vec::is_empty),
        "nothing forgotten, nothing named"
    );

    let caught_up = job.run(CONVERSATION, maintenance()).await.unwrap();
    assert_eq!(caught_up.status, CompactionStatus::NothingToCompact);
    assert_eq!(llm.count(Task::Summarize), 4, "a finished refold stops");
}

/// Consolidation asked to refold, which no one waits on, folds the whole
/// backlog in one run, one committed page at a time. New turns are extracted with the first page, so
/// the ledger never waits on the summary, and their text is folded once the
/// refold reaches them, which keeps the summary's coverage free of gaps.
#[tokio::test]
async fn consolidation_folds_a_long_refold_page_by_page_in_one_run() {
    let messages = uniform_messages(8);
    let port = FakePort::new(messages[..6].to_vec(), Vec::new(), 6);
    port.append(messages[6].clone());
    port.append(messages[7].clone());
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &["u7:s0", "u8:s0"]))],
        vec![],
        summaries(4),
    );
    let job = job(port.clone(), llm.clone(), two_message_refolds());

    let outcome = job.run(CONVERSATION, refolding(8)).await.unwrap();

    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert!(!outcome.refold_pending);
    assert_eq!(
        port.committed_watermarks(),
        vec![8, 8, 8, 8],
        "the new turn is extracted with the first page"
    );
    assert_eq!(
        llm.folded_sequences(),
        vec![vec![1, 2], vec![3, 4], vec![5, 6], vec![7, 8]],
        "oldest first, each message folded once, the new turn last"
    );
    assert_eq!(
        llm.previous_summaries(),
        vec![
            None,
            Some("Summary 1.".to_string()),
            Some("Summary 2.".to_string()),
            Some("Summary 3.".to_string()),
        ],
        "each page carries the one before it forward"
    );
    assert_eq!(llm.count(Task::Extract), 1, "u7 and u8 are extracted once");
    assert_eq!(port.summary_boundary(), Some(8));
    assert_eq!(port.summary(), Some("Summary 4.".to_string()));
}

/// A consolidation stopped between pages keeps the pages it committed and
/// reports them, not a failure; the next run resumes where it stopped.
#[tokio::test]
async fn consolidation_stopped_between_pages_keeps_what_it_folded() {
    let port = FakePort::new(uniform_messages(8), Vec::new(), 8);
    let llm = FakeLlm::new(vec![], vec![], summaries(4));
    let job = job(port.clone(), llm.clone(), two_message_refolds());
    let cancellation = CancellationToken::new();
    port.cancel_on_commit(cancellation.clone());

    let stopped = job
        .run(
            CONVERSATION,
            CompactionRequest {
                cancellation: Some(cancellation),
                ..refolding(8)
            },
        )
        .await
        .expect("the first page's outcome, not the cancellation");

    assert_eq!(stopped.status, CompactionStatus::Partial);
    assert!(stopped.refold_pending);
    assert_eq!(port.summary_boundary(), Some(2));
    assert!(port.errors().is_empty(), "stopping is not an error");

    // The next turn's window still has to move, so it asks again.
    let resumed = job.run(CONVERSATION, refolding(8)).await.unwrap();
    assert_eq!(resumed.status, CompactionStatus::Committed);
    assert_eq!(
        llm.folded_sequences(),
        vec![vec![1, 2], vec![3, 4], vec![5, 6], vec![7, 8]]
    );
    assert_eq!(port.summary_boundary(), Some(8));
}

/// The one refold that cannot record progress: an ineligible output larger than
/// the whole budget, with no summary to carry forward. Retrying it would leave
/// the conversation with no summary for good, so the run gives that range up to
/// the ledger and recall and summarises what it can.
#[tokio::test]
async fn an_unfoldable_refold_page_does_not_stall_the_summary() {
    let mut messages = uniform_messages(4);
    messages[0] = message(
        "f1",
        1,
        SourceRole::Assistant,
        "A failed generation far longer than the refold budget.",
    );
    messages[0].status = "failed".to_string();
    let port = FakePort::new(messages[..2].to_vec(), Vec::new(), 2);
    port.append(messages[2].clone());
    port.append(messages[3].clone());
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &["u3:s0", "u4:s0"]))],
        vec![],
        summaries(1),
    );
    let job = job(port.clone(), llm.clone(), two_message_refolds());

    let outcome = job.run(CONVERSATION, refolding(4)).await.unwrap();

    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert_eq!(llm.folded_sequences(), vec![vec![3, 4]]);
    assert_eq!(port.watermark(), 4);
    assert_eq!(port.summary_boundary(), Some(4));
}

// ---------------------------------------------------------------------------
// A forgotten span never reaches the summariser
// ---------------------------------------------------------------------------

/// Two turns, each stating something the user may later ask to forget.
fn private_turns() -> Vec<SourceMessage> {
    vec![
        message(
            "k1",
            1,
            SourceRole::User,
            "My locker code is 4471, keep it handy.",
        ),
        message("k2", 2, SourceRole::Assistant, "Noted, I will keep it."),
        message(
            "k3",
            3,
            SourceRole::User,
            "My badge number is 9930 for the lab.",
        ),
        message("k4", 4, SourceRole::Assistant, "Got it, badge noted."),
    ]
}

const PRIVATE_SEGMENTS: [&str; 4] = ["k1:s0", "k2:s0", "k3:s0", "k4:s0"];

/// An active user fact whose one assertion quotes `needle` inside `source`,
/// labelled with it as an extracted label would be.
fn fact_quoting(id: &str, source: &SourceMessage, needle: &str) -> MemoryItem {
    let start = source.content.find(needle).expect("the quoted text") as u32;
    let mut item = committed_constraint(id, source);
    item.kind = MemoryKind::UserFact;
    item.label = format!("The user's code is {needle}");
    item.evidence = vec![EvidenceSpan {
        start_byte: start,
        end_byte: start + needle.len() as u32,
        ..whole_span(source, EvidencePurpose::Assertion)
    }];
    item
}

/// The summariser is told every forgotten fact, not only the most recent.
#[tokio::test]
async fn every_forgotten_fact_is_named_to_the_summariser() {
    let messages = private_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![fact_quoting("item-badge", &messages[2], "9930")],
        0,
    );
    {
        let mut inner = port.inner.lock();
        inner
            .forgotten
            .extend((0..40).map(|n| format!("Older forgotten fact {n}")));
    }
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PRIVATE_SEGMENTS))],
        vec![],
        summaries(2),
    );
    let job = job(port.clone(), llm.clone(), small_config());
    port.forget("item-badge");
    job.run(CONVERSATION, refolding(4)).await.unwrap();
    let named = llm.forgotten_labels().pop().unwrap();
    assert_eq!(named.len(), 41, "{named:?}");
    assert!(named.contains(&"Older forgotten fact 0".to_string()));
}

/// Forgetting must survive every fold: the one that first summarises the
/// message, and the refold that rebuilds the summary the forget cleared. The
/// span never reaches the summariser; the fact is named so it is left out.
#[tokio::test]
async fn a_forgotten_span_never_reaches_the_summariser() {
    let messages = private_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![
            fact_quoting("item-locker", &messages[0], "4471"),
            fact_quoting("item-badge", &messages[2], "9930"),
        ],
        0,
    );
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PRIVATE_SEGMENTS))],
        vec![],
        summaries(2),
    );
    let job = job(port.clone(), llm.clone(), small_config());

    // Forgotten before the message was ever summarised: the normal fold.
    port.forget("item-badge");
    job.run(CONVERSATION, refolding(4)).await.unwrap();
    let first = llm.folded_texts().pop().unwrap();
    assert!(!first.contains("9930"), "the forgotten badge number leaked");
    assert!(
        first.contains("My badge number is [forgotten] for the lab."),
        "the rest of the message is still summarised: {first}"
    );
    assert!(first.contains("4471"), "only forgotten spans are cut");
    assert_eq!(
        llm.forgotten_labels().pop().unwrap(),
        vec!["The user's code is 9930".to_string()]
    );

    // Forgotten after it was summarised: the forget clears the summary and
    // the refold rebuilds it from the original messages.
    port.forget("item-locker");
    assert_eq!(port.summary(), None);
    job.run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();
    assert_eq!(
        llm.folded_sequences().last().cloned(),
        Some(vec![1, 2, 3, 4])
    );
    let refold = llm.folded_texts().pop().unwrap();
    for forgotten in ["4471", "9930"] {
        assert!(!refold.contains(forgotten), "{forgotten} came back");
    }
    assert!(refold.contains("My locker code is [forgotten], keep it handy."));
    assert!(refold.contains("My badge number is [forgotten] for the lab."));
    assert!(refold.contains("Noted, I will keep it."));
    assert_eq!(
        llm.forgotten_labels().pop().unwrap(),
        vec![
            "The user's code is 4471".to_string(),
            "The user's code is 9930".to_string(),
        ],
        "most recently forgotten first"
    );
}

/// A later message that repeats a forgotten value is not a forgotten span, so
/// its words reach the summariser, with the fact named for leaving out.
#[tokio::test]
async fn a_restated_forgotten_value_is_named_for_the_summariser_to_leave_out() {
    let messages = private_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![fact_quoting("item-locker", &messages[0], "4471")],
        4,
    );
    port.forget("item-locker");
    port.append(message(
        "k5",
        5,
        SourceRole::User,
        "Scratch 4471, the locker was rekeyed.",
    ));
    port.append(message("k6", 6, SourceRole::Assistant, "Understood."));
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &["k5:s0", "k6:s0"]))],
        vec![],
        summaries(1),
    );
    let job = job(port.clone(), llm.clone(), small_config());

    job.run(CONVERSATION, refolding(6)).await.unwrap();

    assert_eq!(llm.folded_sequences(), vec![vec![1, 2, 3, 4, 5, 6]]);
    let folded = llm.folded_texts().pop().unwrap();
    assert!(folded.contains("My locker code is [forgotten], keep it handy."));
    assert!(folded.contains("Scratch 4471, the locker was rekeyed."));
    assert_eq!(
        llm.forgotten_labels().pop().unwrap(),
        vec!["The user's code is 4471".to_string()]
    );
    let prompt = llm.prompts_for(Task::Summarize).pop().unwrap();
    assert!(prompt.contains("Leave each of them out of the summary"));
}

/// A rebuild discards the ledger and summarises from sequence zero; the
/// user's forget outlives the items it was recorded against.
#[tokio::test]
async fn a_rebuild_folds_forgotten_spans_out_of_the_summary() {
    let messages = private_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![fact_quoting("item-locker", &messages[0], "4471")],
        4,
    );
    port.forget("item-locker");
    port.mark_rebuild_required(CONVERSATION, "source_rewritten")
        .await
        .unwrap();
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PRIVATE_SEGMENTS))],
        vec![],
        summaries(1),
    );
    let job = job(port.clone(), llm.clone(), small_config());

    job.run(CONVERSATION, refolding(4)).await.unwrap();

    assert_eq!(port.operations(), vec!["rebuild".to_string()]);
    let folded = llm.folded_texts().pop().unwrap();
    assert!(!folded.contains("4471"), "the forgotten code leaked");
    assert!(folded.contains("My locker code is [forgotten], keep it handy."));
    assert!(folded.contains("My badge number is 9930 for the lab."));
}

/// A rebuild extracts from the original messages, forgotten words included.
/// What it proposes from a forgotten span is dropped before review, so it is
/// never held as active memory, named to the summariser, or stored.
#[tokio::test]
async fn a_rebuild_does_not_extract_a_forgotten_span_again() {
    let messages = private_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![fact_quoting("item-locker", &messages[0], "4471")],
        4,
    );
    port.forget("item-locker");
    port.mark_rebuild_required(CONVERSATION, "source_rewritten")
        .await
        .unwrap();
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![
                proposed_constraint("c1", "Locker code 4471", "k1", "My locker code is 4471"),
                proposed_constraint(
                    "c2",
                    "Badge number 9930",
                    "k3",
                    "My badge number is 9930 for the lab.",
                ),
            ],
            vec![],
            &PRIVATE_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[("addition:c2", "supported")]))],
        summaries(1),
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job.run(CONVERSATION, refolding(4)).await.unwrap();

    let stored: Vec<&str> = outcome
        .commit
        .inserts
        .iter()
        .map(|item| item.label.as_str())
        .collect();
    assert_eq!(stored, vec!["Badge number 9930"]);
    let review = llm.prompts_for(Task::Review).pop().unwrap();
    assert!(!review.contains("4471"), "the forgotten code was reviewed");
    let summary: serde_json::Value =
        serde_json::from_str(&llm.prompts_for(Task::Summarize).pop().unwrap()).unwrap();
    assert_eq!(
        summary["active_requirement_labels_supplied_separately"],
        serde_json::json!(["Badge number 9930"])
    );
}

// ---------------------------------------------------------------------------
// The summary moves only when a prompt's window has to
// ---------------------------------------------------------------------------

#[tokio::test]
async fn consolidation_extracts_without_touching_the_summary_when_no_refold_is_asked() {
    // The first turn was extracted and summarized earlier; the second has
    // just been answered, and its prompt's window did not have to move.
    let port = FakePort::new(four_turns()[..4].to_vec(), Vec::new(), 2);
    port.summarized_through("Deployment waits for approval.", 2);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &["m3:s0", "m4:s0"]))],
        vec![],
        vec![Ok("A summary nobody asked for.".into())],
    );
    let outcome = job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, maintenance())
        .await
        .unwrap();

    assert_eq!(outcome.status, CompactionStatus::Committed);
    assert_eq!(port.watermark(), 4, "extraction still moves on");
    assert_eq!(llm.count(Task::Summarize), 0, "no summarizer call");
    assert_eq!(
        port.summary().as_deref(),
        Some("Deployment waits for approval."),
        "the prompt's summary stays as it was"
    );
    assert_eq!(
        port.summary_boundary(),
        Some(2),
        "and so does the window it anchors"
    );
    assert_eq!(
        outcome.summary.as_deref(),
        Some("Deployment waits for approval.")
    );
    assert_eq!(outcome.boundary_message_id, None);
}

#[tokio::test]
async fn consolidation_refolds_the_summary_over_the_turns_the_window_dropped() {
    // Extraction reached message 6 on earlier answers; the summary still ends
    // at message 2. The last prompt's window had to start at message 5, so it
    // asked for the summary to cover through 4.
    let port = FakePort::new(four_turns()[..8].to_vec(), Vec::new(), 6);
    port.summarized_through("Deployment waits for approval.", 2);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &["m7:s0", "m8:s0"]))],
        vec![],
        vec![Ok("Approval required; staging may deploy.".into())],
    );
    let outcome = job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, refolding(4))
        .await
        .unwrap();

    assert_eq!(port.watermark(), 8);
    assert_eq!(llm.count(Task::Summarize), 1);
    // The fold reads the dropped turns back from below the watermark, and
    // nothing the window still carries.
    assert_eq!(llm.folded_sequences(), vec![vec![3, 4]]);
    let fold = llm
        .prompts_for(Task::Summarize)
        .pop()
        .expect("a summarizer prompt");
    assert!(fold.contains("Actually you may deploy staging."), "{fold}");
    assert!(fold.contains("Noted, staging only."), "{fold}");
    assert!(!fold.contains("What about production?"), "{fold}");
    assert_eq!(
        llm.previous_summaries(),
        vec![Some("Deployment waits for approval.".to_string())]
    );
    assert_eq!(
        port.summary().as_deref(),
        Some("Approval required; staging may deploy.")
    );
    assert_eq!(port.summary_boundary(), Some(4));
    assert_eq!(outcome.boundary_message_id.as_deref(), Some("m4"));

    // A later turn that asked before this refold landed asks for nothing new:
    // its consolidation extracts the next turn and leaves the summary alone.
    port.append(message("m9", 9, SourceRole::User, "Ship staging tonight."));
    port.append(message(
        "m10",
        10,
        SourceRole::Assistant,
        "Staging tonight.",
    ));
    let again = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![],
            vec![],
            &["m9:s0", "m10:s0"],
        ))],
        vec![],
        vec![],
    );
    job(port.clone(), again.clone(), small_config())
        .run(
            CONVERSATION,
            CompactionRequest {
                summary_refold: Some(SummaryRefold {
                    window_start_sequence: 5,
                    through_sequence: 6,
                }),
                ..maintenance()
            },
        )
        .await
        .unwrap();
    assert_eq!(port.watermark(), 10);
    assert_eq!(again.count(Task::Summarize), 0);
    assert_eq!(port.summary_boundary(), Some(4));
}

#[tokio::test]
async fn a_refold_never_runs_past_what_extraction_accepted() {
    // The window asked for everything through message 6, but the work cap
    // stops extraction after the second turn: the summary ends where the
    // watermark does.
    let port = FakePort::new(four_turns()[..6].to_vec(), Vec::new(), 2);
    let llm = FakeLlm::new(
        vec![
            Ok(extraction_response(vec![], vec![], &["m3:s0", "m4:s0"])),
            Ok(extraction_response(vec![], vec![], &["m5:s0", "m6:s0"])),
        ],
        vec![],
        vec![Ok("Staging may deploy.".into())],
    );
    let outcome = job(
        port.clone(),
        llm.clone(),
        CompactionConfig {
            max_source_messages_per_attempt: 2,
            ..small_config()
        },
    )
    .run(CONVERSATION, refolding(6))
    .await
    .unwrap();
    assert_eq!(outcome.processed_through_sequence, 4);
    assert_eq!(llm.folded_sequences(), vec![vec![1, 2, 3, 4]]);
    assert_eq!(port.summary().as_deref(), Some("Staging may deploy."));
    assert_eq!(port.summary_boundary(), Some(4));
}
