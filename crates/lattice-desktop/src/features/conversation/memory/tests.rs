//! Guarantees the compaction job owes its callers.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §12, §16.
//!
//! These run against fakes on purpose. The real repository lives behind
//! `features::`, which application code may not import, and the real utility
//! model is the thing whose misbehaviour is being simulated. The fake port is
//! not a stub: it enforces the revision compare-and-swap, because one of these
//! tests is about what happens when that check fires.

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
    MAX_QUOTE_BYTES,
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
        references: Vec::new(),
        replayed: None,
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

fn proposed_supersession(
    item_id: &str,
    replacement: &str,
    message_id: &str,
    quote: &str,
) -> serde_json::Value {
    serde_json::json!({
        "item_id": item_id,
        "state": "superseded",
        "replacement_candidate_id": replacement,
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
        "entries": pairs
            .iter()
            .map(|(of, verdict)| serde_json::json!({
                "of": of,
                "reason": "The quoted passage says this.",
                "verdict": verdict,
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
    shared: Vec<MemoryItem>,
    span_reads: Vec<String>,
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
                shared: Vec::new(),
                span_reads: Vec::new(),
            }),
        })
    }

    fn share(&self, item: MemoryItem) {
        self.inner.lock().shared.push(item);
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
    async fn shared_items(&self, _conversation_id: &str, _text: &str) -> Result<Vec<MemoryItem>> {
        Ok(self.inner.lock().shared.clone())
    }

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
        conversation_id: &str,
        spans: &[SourceSpanRef],
        _limits: SourceReadLimits,
    ) -> Result<Vec<ResolvedSpan>> {
        let mut inner = self.inner.lock();
        inner.span_reads.push(conversation_id.to_string());
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
    Review,
    Summarize,
    Shrink,
    Forgetting,
}

struct FakeLlm {
    extraction: Mutex<VecDeque<Result<String>>>,
    review: Mutex<VecDeque<Result<String>>>,
    summary: Mutex<VecDeque<Result<String>>>,
    restating: Mutex<Vec<String>>,
    existing: Mutex<Vec<String>>,
    calls: Mutex<Vec<Task>>,
    prompts: Mutex<Vec<String>>,
    request_limits: Mutex<Vec<(Option<String>, Option<u32>)>>,
    samplings: Mutex<Vec<Option<SamplingOverride>>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

/// The labelled sentences of a prompt's passages: each passage's rows as (handle, text).
fn labelled_rows(payload: &serde_json::Value) -> Vec<Vec<(String, String)>> {
    ["segments", "context_passages"]
        .iter()
        .filter_map(|key| payload.get(*key).and_then(serde_json::Value::as_array))
        .flatten()
        .map(|passage| {
            passage["text"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .filter_map(|row| {
                    let (label, words) = row.strip_prefix('[')?.split_once("] ")?;
                    Some((label.to_string(), words.to_string()))
                })
                .collect()
        })
        .collect()
}

/// The handles of the sentences a scripted quote lies in.
fn sentences_of_quote(passages: &[Vec<(String, String)>], quote: &str) -> Vec<String> {
    let quote = quote.replace(['\r', '\n'], " ");
    for passage in passages {
        if let Some((label, _)) = passage.iter().find(|(_, words)| words.contains(&quote)) {
            return vec![label.clone()];
        }
    }
    for passage in passages {
        let inside: Vec<String> = passage
            .iter()
            .filter(|(_, words)| quote.contains(words.as_str()))
            .map(|(label, _)| label.clone())
            .collect();
        if !inside.is_empty() {
            return inside;
        }
    }
    Vec::new()
}

/// An answer scripted in the old patch form, written as the layer's items: each
/// quote becomes the labelled sentences of the prompt it lies in, each message
/// id and candidate id goes, and a transition holds the addition it names. Any
/// other script is returned as it is.
fn as_items(scripted: &str, prompt: &str, existing: &[String]) -> String {
    let Ok(old) = serde_json::from_str::<serde_json::Value>(scripted) else {
        return scripted.to_string();
    };
    let Some(add) = old.get("add").and_then(serde_json::Value::as_array) else {
        return scripted.to_string();
    };
    let shown: serde_json::Value = serde_json::Deserializer::from_str(prompt)
        .into_iter::<serde_json::Value>()
        .next()
        .and_then(std::result::Result::ok)
        .unwrap_or_default();
    let passages = labelled_rows(&shown);
    let cited = |evidence: &serde_json::Value, purpose: bool| -> Vec<serde_json::Value> {
        evidence
            .as_array()
            .into_iter()
            .flatten()
            .map(|quote| {
                let sentences =
                    sentences_of_quote(&passages, quote["quote"].as_str().unwrap_or_default());
                if purpose {
                    serde_json::json!({"purpose": quote["purpose"], "sentences": sentences})
                } else {
                    serde_json::json!({"sentences": sentences})
                }
            })
            .collect()
    };
    let addition = |item: &serde_json::Value| {
        serde_json::json!({
            "evidence": cited(&item["evidence"], true),
            "kind": item["kind"],
            "label": item["label"],
        })
    };
    let candidate = |id: &serde_json::Value| add.iter().find(|item| item["candidate_id"] == *id);
    let handle = |id: &str| {
        existing
            .iter()
            .position(|known| known == id)
            .map_or_else(|| "none".to_string(), |at| format!("m{}", at + 1))
    };
    let transitions = old["transitions"].as_array().cloned().unwrap_or_default();
    let mut nested: Vec<serde_json::Value> = Vec::new();
    let mut moves: Vec<serde_json::Value> = Vec::new();
    for transition in &transitions {
        let grounds = cited(&transition["evidence"], false);
        let replacement = candidate(&transition["replacement_candidate_id"]);
        let superseded = transition["state"] == "superseded";
        let proposal = match (candidate(&transition["item_id"]), replacement) {
            (Some(earlier), Some(later)) if superseded => {
                nested.extend([
                    earlier["candidate_id"].clone(),
                    later["candidate_id"].clone(),
                ]);
                serde_json::json!({
                    "kind": "correct",
                    "evidence": grounds,
                    "earlier": addition(earlier),
                    "later": addition(later),
                })
            }
            (_, Some(later)) if superseded => {
                nested.push(later["candidate_id"].clone());
                serde_json::json!({
                    "kind": "supersede",
                    "evidence": grounds,
                    "item_id": handle(transition["item_id"].as_str().unwrap_or_default()),
                    "replacement": addition(later),
                })
            }
            _ => serde_json::json!({
                "kind": "resolve",
                "evidence": grounds,
                "item_id": handle(transition["item_id"].as_str().unwrap_or_default()),
            }),
        };
        moves.push(serde_json::json!({ "proposal": proposal }));
    }
    let mut items: Vec<serde_json::Value> = add
        .iter()
        .filter(|item| !nested.contains(&item["candidate_id"]))
        .map(|item| serde_json::json!({"proposal": {"kind": "add", "addition": addition(item)}}))
        .collect();
    items.extend(moves);
    serde_json::json!({ "items": items }).to_string()
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
            restating: Mutex::new(Vec::new()),
            existing: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            prompts: Mutex::new(Vec::new()),
            request_limits: Mutex::new(Vec::new()),
            samplings: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<Task> {
        self.calls.lock().clone()
    }

    fn knows_items(&self, ids: Vec<String>) {
        *self.existing.lock() = ids;
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
                payload["previous_summary"].as_str().map(|rows| {
                    rows.lines()
                        .filter_map(|row| row.split_once("] ").map(|(_, text)| text))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
            .collect()
    }

    /// Summary lines holding `text` are judged to restate a forgotten fact.
    fn restating(&self, text: &str) {
        self.restating.lock().push(text.to_string());
    }

    /// The scripted summary `text`, written as the edit that makes it the
    /// whole summary: every line the request shows under `key` removed, the
    /// text inserted at the start.
    fn rewriting(prompt: &str, key: &str, text: &str) -> String {
        let payload: serde_json::Value = serde_json::from_str(prompt).expect("summary prompt JSON");
        let shown: Vec<&str> = payload[key]
            .as_str()
            .unwrap_or_default()
            .lines()
            .filter_map(|row| row.strip_prefix('[')?.split_once(']'))
            .map(|(handle, _)| handle)
            .collect();
        serde_json::json!({
            "insert": [{"after": "start", "with": text}],
            "move": [],
            "remove": shown,
            "replace": [],
        })
        .to_string()
    }

    /// Every line the request shows is clear of the forgotten facts, except
    /// the ones holding a text of `restating`.
    fn judged(&self, prompt: &str) -> String {
        let payload: serde_json::Value =
            serde_json::from_str(prompt).expect("forgetting prompt JSON");
        let restating = self.restating.lock().clone();
        let entries: Vec<serde_json::Value> = payload["lines"]
            .as_array()
            .expect("forgetting lines")
            .iter()
            .filter_map(|row| row.as_str()?.strip_prefix('[')?.split_once("] "))
            .map(|(handle, text)| {
                let restates = restating.iter().any(|fact| text.contains(fact.as_str()));
                serde_json::json!({
                    "of": handle,
                    "verdict": if restates { "restates" } else { "clear" },
                })
            })
            .collect();
        serde_json::json!({ "entries": entries }).to_string()
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
            Some("review_memory_changes") => Task::Review,
            Some("shrink_working_summary") => Task::Shrink,
            Some("check_summary_for_forgotten_facts") => Task::Forgetting,
            _ if prompt.contains("\"extract_memory\"") => Task::Extract,
            _ if prompt.contains("\"review_memory_changes\"") => Task::Review,
            _ if prompt.contains("\"shrink_working_summary\"") => Task::Shrink,
            _ => Task::Summarize,
        };
        self.calls.lock().push(task);
        self.prompts.lock().push(prompt.to_string());
        let queue = match task {
            Task::Extract => &self.extraction,
            Task::Review => &self.review,
            Task::Summarize | Task::Shrink => &self.summary,
            Task::Forgetting => return Ok(self.judged(prompt)),
        };
        let scripted = queue.lock().pop_front().unwrap_or_else(|| {
            Err(AppError::Other(format!(
                "no scripted reply left for {task:?}"
            )))
        });
        match (task, scripted) {
            (Task::Extract, Ok(text)) => Ok(as_items(&text, prompt, &self.existing.lock())),
            (Task::Summarize, Ok(text)) => Ok(Self::rewriting(prompt, "previous_summary", &text)),
            (Task::Shrink, Ok(text)) => Ok(Self::rewriting(prompt, "summary", &text)),
            (_, other) => other,
        }
    }
}

#[async_trait]
impl LLMPort for FakeLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.request_limits.lock().push((
            request.reasoning_effort.clone(),
            request.max_output_tokens(),
        ));
        self.samplings.lock().push(request.sampling);
        self.requests.lock().push(request.clone());
        let users: Vec<String> = request
            .input()
            .iter()
            .filter_map(|input| match input {
                lattice_core::application::ports::llm_port::CompletionInput::Message {
                    role,
                    content,
                } if role == "user" => Some(content.clone()),
                _ => None,
            })
            .collect();
        let prompt = users
            .iter()
            .rev()
            .find(|content| content.contains("\"task\":\""))
            .or(users.last())
            .cloned()
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
    llm.knows_items(
        port.items()
            .iter()
            .chain(port.inner.lock().shared.iter())
            .map(|item| item.id.as_str().to_string())
            .collect(),
    );
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
        "the first that applies gives the verdict",
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
            super::summarize::FOLD_SYSTEM.contains(required),
            "summariser prompt no longer says: {required}"
        );
    }
}

/// llama.cpp throws away the whole grammar when any `maxLength` or `maxItems`
/// exceeds its repetition threshold, so no schema bound may.
#[test]
fn every_schema_bound_stays_within_the_llama_cpp_grammar_repetition_limit() {
    {
        let name = "turn intent";
        let schema = crate::features::conversation::chat::intent::intent_schema();
        let (bound, path) =
            lattice_core::application::ports::llm_port::largest_repetition_bound(&schema);
        assert!(
            bound <= lattice_core::application::ports::llm_port::GRAMMAR_REPETITION_LIMIT as u64,
            "{name} schema bound {bound} at {path} exceeds llama.cpp's grammar limit of {}",
            lattice_core::application::ports::llm_port::GRAMMAR_REPETITION_LIMIT
        );
    }
}

fn object_keys(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// The first `properties` object holding `key`, anywhere in `schema`.
fn properties_holding<'a>(
    schema: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    match schema {
        serde_json::Value::Object(map) => map
            .get("properties")
            .filter(|properties| properties.get(key).is_some())
            .or_else(|| {
                map.values()
                    .find_map(|child| properties_holding(child, key))
            }),
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|child| properties_holding(child, key)),
        _ => None,
    }
}

/// llama.cpp builds the grammar in schema order. An addition asks for its
/// evidence before its kind and label, as the sorted keys did.
#[test]
fn the_extraction_grammar_asks_for_the_grounds_before_the_decision() {
    use lattice_core::application::services::model_contract::shape::{Pointers, Shape};
    let mut scope =
        lattice_core::application::services::model_contract::shown::Scope::for_question();
    let showing = scope.request();
    type Answer = <super::extract::MemoryExtraction as lattice_core::application::services::model_contract::ask::Answered>::Answer;
    let schema =
        <Answer as Shape>::schema(&showing, Pointers::Plain).expect("the extraction schema");
    let addition = properties_holding(schema.as_json(), "label").expect("an addition");
    assert_eq!(object_keys(Some(addition)), ["evidence", "kind", "label"]);
}

/// The passages to extract are the last thing the extractor reads before it
/// answers, after the rules.
#[test]
fn the_extraction_request_ends_with_the_passages_to_extract() {
    use super::extract::{Input, MemoryExtraction};
    use lattice_core::application::services::model_contract::ask::Answered;
    let source = message(
        "m1",
        1,
        SourceRole::User,
        "Never deploy to production on a Friday.",
    );
    let segments = segment_message(&source, 512, 32);
    let input = Input::of(&segments, &[source], &[], &[]);
    let mut scope =
        lattice_core::application::services::model_contract::shown::Scope::for_question();
    let mut showing = scope.request();
    let says = MemoryExtraction::says(&input, &mut showing).expect("the extraction request");
    let payload: serde_json::Value =
        serde_json::from_str(&says.task).expect("extraction payload JSON");

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
    assert_eq!(
        payload
            .pointer("/segments/0/text")
            .and_then(serde_json::Value::as_str),
        Some("[s1] Never deploy to production on a Friday.")
    );
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
async fn an_unreadable_answer_is_asked_for_once_more_and_then_commits_nothing() {
    let messages = four_turns();
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![
            Ok("I could not produce JSON, sorry.".to_string()),
            Ok("{ still not valid".to_string()),
            Ok("{ never asked for".to_string()),
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

    assert_eq!(
        llm.count(Task::Extract),
        2,
        "the answer and its one correction"
    );
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
        vec![Ok(verdict_response(&[("r1", "supported")]))],
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
async fn a_correction_here_reaches_a_fact_from_another_chat() {
    let messages = four_turns();
    let port = FakePort::new(messages.clone(), Vec::new(), 0);
    let mut shared = committed_constraint("item-shared", &messages[0]);
    shared.conversation_id = "other-chat".to_string();
    shared.created_at_sequence = 0;
    shared.changed_at_sequence = 0;
    port.share(shared);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![],
            vec![proposed_transition(
                "item-shared",
                "m3",
                "Actually you may deploy staging.",
            )],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[("r1", "supported")]))],
        vec![Ok("Deployment is now allowed on staging.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("a correction to a shared item commits here");

    assert!(llm.prompts_for(Task::Extract)[0].contains("Deployment requires approval"));
    assert!(port
        .inner
        .lock()
        .span_reads
        .contains(&"other-chat".to_string()));
    let [retired] = outcome.commit.updates.as_slice() else {
        panic!(
            "expected the shared item alone: {:?}",
            outcome.commit.updates
        );
    };
    assert_eq!(retired.conversation_id, "other-chat");
    assert_ne!(retired.state, MemoryState::Active);
}

/// The conflict is recorded here, and a commit refuses a span from another
/// chat, so it must not carry the shared item's own passages.
#[tokio::test]
async fn an_unclear_correction_to_a_shared_item_quotes_only_this_chat() {
    let messages = four_turns();
    let port = FakePort::new(messages.clone(), Vec::new(), 0);
    let mut shared = committed_constraint("item-shared", &messages[0]);
    shared.conversation_id = "other-chat".to_string();
    shared.created_at_sequence = 0;
    shared.changed_at_sequence = 0;
    port.share(shared);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![],
            vec![proposed_transition(
                "item-shared",
                "m3",
                "Actually you may deploy staging.",
            )],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[("r1", "ambiguous")]))],
        vec![Ok("Deployment may have changed.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("an unclear correction still commits");

    assert!(outcome.commit.updates.is_empty());
    let [conflict] = outcome.commit.inserts.as_slice() else {
        panic!("expected the conflict alone: {:?}", outcome.commit.inserts);
    };
    assert_eq!(conflict.kind, MemoryKind::UnresolvedChange);
    let quoted: Vec<&str> = conflict
        .evidence
        .iter()
        .map(|span| span.message_id.as_str())
        .collect();
    assert_eq!(quoted, ["m3"]);
}

#[tokio::test]
async fn a_shared_item_does_not_become_a_requirement_of_the_summary() {
    let messages = four_turns();
    let port = FakePort::new(messages.clone(), Vec::new(), 0);
    let mut shared = committed_constraint("item-shared", &messages[0]);
    shared.conversation_id = "other-chat".to_string();
    shared.created_at_sequence = 0;
    shared.changed_at_sequence = 0;
    port.share(shared);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        vec![Ok("Talked about deployment.".to_string())],
    );
    let job = job(port.clone(), llm.clone(), small_config());

    job.run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("the run commits");

    for prompt in llm.prompts_for(Task::Summarize) {
        assert!(!prompt.contains("Deployment requires approval"), "{prompt}");
    }
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
        vec![Ok(verdict_response(&[("r1", "unsupported")]))],
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

/// The one review call for a pass that replaces a constraint with a new item
/// and ends another with nothing in its place.
async fn reviewed_retirements() -> String {
    let messages = four_turns();
    let mut on_hold = committed_constraint("item-hold", &messages[0]);
    on_hold.label = "Staging deploys are on hold".to_string();
    let port = FakePort::new(
        messages.clone(),
        vec![committed_constraint("item-old", &messages[0]), on_hold],
        0,
    );
    let quote = "Actually you may deploy staging.";
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![proposed_constraint(
                "c1",
                "Staging deploys need no approval",
                "m3",
                quote,
            )],
            vec![
                proposed_supersession("item-old", "c1", "m3", quote),
                proposed_transition("item-hold", "m3", quote),
            ],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[
            ("r1", "supported"),
            ("r2", "supported"),
            ("r3", "supported"),
        ]))],
        vec![Ok("Staging may be deployed.".to_string())],
    );
    let job = job(port, llm.clone(), small_config());

    job.run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("the pass commits");

    assert_eq!(llm.count(Task::Review), 1);
    llm.prompts_for(Task::Review).remove(0)
}

fn review_of(prompt: &str, id: &str) -> serde_json::Value {
    let payload: serde_json::Value = serde_json::from_str(prompt).expect("review prompt JSON");
    payload["reviews"]
        .as_array()
        .expect("reviews")
        .iter()
        .find(|review| review["id"] == id)
        .unwrap_or_else(|| panic!("no review for {id}: {prompt}"))
        .clone()
}

#[tokio::test]
async fn a_superseding_change_is_reviewed_as_proposing_its_replacement_not_the_item_it_retires() {
    let prompt = reviewed_retirements().await;

    let transition = review_of(&prompt, "r2");
    assert_eq!(
        transition["proposed_label"],
        "Staging deploys need no approval"
    );
    assert_eq!(
        transition["existing_item_label"],
        "Deployment requires approval"
    );
    assert_eq!(
        transition["existing_item_evidence"][0]["text"],
        "Do not deploy until I approve."
    );

    let addition = review_of(&prompt, "r1");
    assert_eq!(
        addition["proposed_label"],
        "Staging deploys need no approval"
    );
    assert_eq!(addition["existing_item_label"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_retirement_with_no_replacement_is_reviewed_with_no_proposed_label() {
    let prompt = reviewed_retirements().await;

    let transition = review_of(&prompt, "r3");
    assert_eq!(
        transition.get("proposed_label"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(
        transition["existing_item_label"],
        "Staging deploys are on hold"
    );
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
            ("r1", "supported"),
            ("r2", "unsupported"),
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

/// Measured, not style: without this sentence Qwen3.8-27B answers `add: []` for
/// windows holding the user's rule, and the rule never reaches memory.
#[test]
fn the_extractor_is_told_what_an_empty_add_means() {
    assert!(super::prompts::EXTRACTOR_SYSTEM.contains("Every requirement, goal, decision, fact, preference or unresolved question the user states goes in add; add is empty only when no segment holds one."));
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
    assert!(!prompt.contains("Understood, I will wait."));
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
        vec![Ok(verdict_response(&[("r1", "supported")]))],
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
    let second: serde_json::Value =
        serde_json::from_str(&llm.prompts_for(Task::Extract).pop().unwrap()).unwrap();
    let segments = second["segments"].as_array().expect("segments");
    assert_eq!(
        segments.len(),
        1,
        "the covered turns must not be re-extracted"
    );
    assert!(segments[0]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Noted again: staging only.")));
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

/// The forgotten value comes back in words no span marks, and the summariser
/// writes it down: the line is left out, the others stand.
#[tokio::test]
async fn a_summary_line_restating_a_forgotten_value_is_not_kept() {
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
        vec![Ok("The locker was rekeyed, scratch 4471.
The rekeying is noted."
            .to_string())],
    );
    llm.restating("4471");
    let job = job(port.clone(), llm.clone(), small_config());

    job.run(CONVERSATION, refolding(6)).await.unwrap();

    assert_eq!(llm.count(Task::Forgetting), 1);
    assert_eq!(port.summary().as_deref(), Some("The rekeying is noted."));
}

/// Nothing was forgotten, so no line of the summary is checked against it.
#[tokio::test]
async fn a_summary_is_not_checked_when_nothing_was_forgotten() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        summary_reply(),
    );

    job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(llm.count(Task::Forgetting), 0);
    assert_eq!(port.summary().as_deref(), Some("Approval gate recorded."));
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
        vec![Ok(verdict_response(&[("r1", "supported")]))],
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

/// What a model answers now and then in place of a review: the first id,
/// "supported", and a reason that says nothing.
fn hollow_reply(of: &str, reason: &str) -> String {
    serde_json::json!({"entries": [{"of": of, "reason": reason, "verdict": "supported"}]})
        .to_string()
}

fn deployment_constraint() -> Vec<Result<String>> {
    vec![Ok(extraction_response(
        vec![proposed_constraint(
            "c1",
            "Deployment requires approval",
            "m1",
            "Do not deploy until I approve.",
        )],
        vec![],
        &PREFIX_SEGMENTS,
    ))]
}

fn two_constraints() -> Vec<Result<String>> {
    vec![Ok(extraction_response(
        vec![
            proposed_constraint(
                "c1",
                "Deployment requires approval",
                "m1",
                "Do not deploy until I approve.",
            ),
            proposed_constraint(
                "c2",
                "Staging may be deployed",
                "m3",
                "Actually you may deploy staging.",
            ),
        ],
        vec![],
        &PREFIX_SEGMENTS,
    ))]
}

fn reviewed_ids(prompt: &str) -> Vec<String> {
    let payload: serde_json::Value = serde_json::from_str(prompt).expect("review prompt JSON");
    payload["reviews"]
        .as_array()
        .expect("reviews")
        .iter()
        .map(|review| review["id"].as_str().expect("id").to_string())
        .collect()
}

fn summary_reply() -> Vec<Result<String>> {
    vec![Ok("Approval gate recorded.".to_string())]
}

#[tokio::test]
async fn a_verdict_that_gives_no_reason_is_not_a_review() {
    for reason in ["", "   ", "x", "placeholder"] {
        let port = FakePort::new(four_turns(), Vec::new(), 0);
        let llm = FakeLlm::new(
            deployment_constraint(),
            vec![
                Ok(hollow_reply("r1", reason)),
                Ok(hollow_reply("r1", reason)),
            ],
            summary_reply(),
        );
        let job = job(port, llm, small_config());

        let outcome = job
            .run(CONVERSATION, CompactionRequest::default())
            .await
            .unwrap();

        assert!(outcome.review_degraded, "reason {reason:?}");
        assert_eq!(
            outcome.commit.inserts[0].review,
            MemoryReview::Ambiguous,
            "a requirement nobody read is carried as ambiguous, reason {reason:?}"
        );
    }
}

#[tokio::test]
async fn a_hollow_reply_is_asked_for_again_and_the_real_reading_stands() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        deployment_constraint(),
        vec![
            Ok(hollow_reply("r1", "")),
            Ok(verdict_response(&[("r1", "unsupported")])),
        ],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(llm.count(Task::Review), 2);
    assert_eq!(outcome.review_calls, 2);
    assert!(!outcome.review_degraded);
    assert!(
        outcome.commit.inserts.is_empty(),
        "the hollow \"supported\" must not outlive the reading that rejected it"
    );
}

#[tokio::test]
async fn reviews_a_reply_left_out_are_asked_for_again_on_their_own() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        two_constraints(),
        vec![
            Ok(verdict_response(&[("r1", "supported")])),
            Ok(verdict_response(&[("r2", "supported")])),
        ],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    let prompts = llm.prompts_for(Task::Review);
    assert_eq!(prompts.len(), 2);
    assert_eq!(reviewed_ids(&prompts[0]), ["r1", "r2"]);
    assert_eq!(reviewed_ids(&prompts[1]), ["r2"]);
    assert!(!outcome.review_degraded);
    assert_eq!(outcome.commit.inserts.len(), 2);
    assert!(outcome
        .commit
        .inserts
        .iter()
        .all(|item| item.review == MemoryReview::Supported));
}

#[tokio::test]
async fn a_review_still_unread_after_its_correction_stays_ambiguous() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        two_constraints(),
        vec![
            Ok(verdict_response(&[("r1", "supported")])),
            Ok(hollow_reply("r2", "")),
            Ok(hollow_reply("r2", "")),
        ],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(
        llm.count(Task::Review),
        3,
        "the left-out review is asked for once, corrected once, and no further"
    );
    assert!(outcome.review_degraded);
    let review_of = |label: &str| {
        outcome
            .commit
            .inserts
            .iter()
            .find(|item| item.label == label)
            .map(|item| item.review)
    };
    assert_eq!(
        review_of("Deployment requires approval"),
        Some(MemoryReview::Supported)
    );
    assert_eq!(
        review_of("Staging may be deployed"),
        Some(MemoryReview::Ambiguous)
    );
}

#[tokio::test]
async fn a_review_reply_that_cannot_be_read_is_asked_for_again() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        deployment_constraint(),
        vec![
            Ok("not the answer".to_string()),
            Ok(verdict_response(&[("r1", "supported")])),
        ],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(llm.count(Task::Review), 2);
    assert!(!outcome.review_degraded);
    assert_eq!(outcome.commit.inserts[0].review, MemoryReview::Supported);
}

#[tokio::test]
async fn a_provider_failure_is_not_asked_for_again() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        deployment_constraint(),
        vec![
            Err(AppError::ServiceNotAvailable("no model".into())),
            Ok(verdict_response(&[("r1", "supported")])),
        ],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(
        llm.count(Task::Review),
        1,
        "the transport has already retried what can be retried"
    );
    assert!(outcome.review_degraded);
    assert_eq!(outcome.commit.inserts[0].review, MemoryReview::Ambiguous);
}

#[tokio::test]
async fn a_reason_in_a_script_written_without_spaces_is_a_reason() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        deployment_constraint(),
        vec![Ok(hollow_reply("r1", "用户明确说明了这一要求。"))],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(llm.count(Task::Review), 1);
    assert!(!outcome.review_degraded);
    assert_eq!(outcome.commit.inserts[0].review, MemoryReview::Supported);
}

#[tokio::test]
async fn a_later_ask_cannot_change_a_reading_it_did_not_ask_for() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        two_constraints(),
        vec![
            Ok(verdict_response(&[("r1", "supported")])),
            Ok(verdict_response(&[
                ("r1", "unsupported"),
                ("r2", "supported"),
            ])),
        ],
        summary_reply(),
    );
    let job = job(port, llm, small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.commit.inserts.len(), 2);
    assert!(outcome
        .commit
        .inserts
        .iter()
        .all(|item| item.review == MemoryReview::Supported));
}

#[tokio::test]
async fn the_review_request_names_each_change_by_its_handle_alone() {
    let prompt = reviewed_retirements().await;

    assert_eq!(reviewed_ids(&prompt), ["r1", "r2", "r3"]);
    for identity in ["item-old", "item-hold", "addition:", "transition:", "c1\""] {
        assert!(!prompt.contains(identity), "{identity} reached {prompt}");
    }
}

#[tokio::test]
async fn a_verdict_for_a_change_the_request_did_not_show_is_not_read() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        two_constraints(),
        vec![
            Ok(verdict_response(&[
                ("r1", "supported"),
                ("r9", "supported"),
            ])),
            Ok(verdict_response(&[("r2", "unsupported")])),
        ],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    let prompts = llm.prompts_for(Task::Review);
    assert_eq!(prompts.len(), 2);
    assert_eq!(reviewed_ids(&prompts[1]), ["r2"]);
    assert!(!outcome.review_degraded);
    let [kept] = outcome.commit.inserts.as_slice() else {
        panic!(
            "expected the supported addition alone: {:?}",
            outcome.commit.inserts
        );
    };
    assert_eq!(kept.label, "Deployment requires approval");
}

#[tokio::test]
async fn a_verdict_without_a_reason_is_corrected_on_its_own_while_the_others_stand() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let both = serde_json::json!({"entries": [
        {"of": "r1", "reason": "The quoted passage says this.", "verdict": "supported"},
        {"of": "r2", "reason": "", "verdict": "supported"},
    ]})
    .to_string();
    let llm = FakeLlm::new(
        two_constraints(),
        vec![Ok(both), Ok(verdict_response(&[("r2", "supported")]))],
        summary_reply(),
    );
    let job = job(port, llm.clone(), small_config());

    let outcome = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    let prompts = llm.prompts_for(Task::Review);
    assert_eq!(prompts.len(), 2);
    assert_eq!(reviewed_ids(&prompts[1]), ["r2"]);
    assert!(!outcome.review_degraded);
    assert!(outcome
        .commit
        .inserts
        .iter()
        .all(|item| item.review == MemoryReview::Supported));
}

fn review_with_context(text: &str) -> super::verify::Input {
    use super::verify::{Excerpt, Input, Passage, Review, Subject};
    use lattice_core::application::services::model_contract::shown::{Internal, Told};
    let harbour = |text: &str| Passage {
        message_id: Told("m3".into()),
        sequence: Told(3),
        role: Told("user".into()),
        text: Some(Told(text.into())),
    };
    let review = Review {
        subject: Subject::Addition("c1".into()),
        change: Told("add_mandatory_item".into()),
        kind: Told("constraint".into()),
        label: Some(Told("Tide tables are posted".into())),
        proposed: vec![harbour("The tide table is posted.")],
        existing: Vec::new(),
        existing_label: None,
        context: vec![Excerpt {
            message_id: Told("m3".into()),
            sequence: Told(3),
            role: Told("user".into()),
            text: Told(text.into()),
            quoted: Internal::of(Some((0, 25))),
        }],
    };
    Input::of(&[&review], 4.0)
}

fn surrounding_shown(input: &super::verify::Input) -> Vec<serde_json::Value> {
    use super::verify::MemoryReview;
    use lattice_core::application::services::model_contract::ask::Answered;
    use lattice_core::application::services::model_contract::shown::Scope;
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let says = MemoryReview::says(input, &mut showing).expect("the review request");
    let task: serde_json::Value = serde_json::from_str(&says.task).expect("review task JSON");
    task["reviews"][0]["surrounding_original_text"]
        .as_array()
        .expect("surrounding text")
        .clone()
}

#[test]
fn the_surrounding_text_is_cut_to_the_room_the_window_leaves() {
    use super::verify::MemoryReview;
    use lattice_core::application::services::model_contract::ask::Answered;
    let long = "The tide table is posted at the quay. ".repeat(20);
    let input = review_with_context(&long);

    let whole = surrounding_shown(&input);
    assert_eq!(whole[0]["text"], long.as_str());
    assert_eq!(whole[0]["truncated"], false);

    let none = MemoryReview::within(&input, 0).expect("a cut input");
    assert!(surrounding_shown(&none).is_empty());

    let cut = MemoryReview::within(&input, 1).expect("a cut input");
    let shown = surrounding_shown(&cut);
    assert_eq!(shown[0]["truncated"], true);
    assert_eq!(shown[0]["text"], "The tide table is posted.");

    let roomy = MemoryReview::within(&input, 100_000).expect("a cut input");
    assert_eq!(surrounding_shown(&roomy)[0]["truncated"], false);
}

#[test]
fn the_review_request_holds_no_identity_of_the_app() {
    use super::verify::{Excerpt, Input, MemoryReview, Passage, Review, Subject};
    use lattice_core::application::services::model_contract::shown::Told;
    use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
    let mut marks = Marks::new();
    let passage = Passage {
        message_id: marks.told("message-quay-3".to_owned()),
        sequence: Told(3),
        role: marks.told("assistant".to_owned()),
        text: Some(marks.told("The kiln closes at dusk.".to_owned())),
    };
    let excerpt = Excerpt {
        message_id: marks.told("message-quay-3".to_owned()),
        sequence: Told(3),
        role: marks.told("assistant".to_owned()),
        text: marks.told("The kiln closes at dusk, and the quay with it.".to_owned()),
        quoted: marks.internal(Some((120_000_usize, 120_500_usize))),
    };
    let review = Review {
        subject: marks.reference(Subject::Transition("candidate-kiln-9".to_owned())),
        change: marks.told("supersede_item".to_owned()),
        kind: marks.told("constraint".to_owned()),
        label: Some(marks.told("The kiln closes at dusk".to_owned())),
        proposed: vec![passage.clone()],
        existing: vec![passage],
        existing_label: Some(marks.told("The kiln closes at noon".to_owned())),
        context: vec![excerpt],
    };
    let input = Input {
        reviews: vec![review],
        ..Input::of(&[], 4.0)
    };
    holds::<MemoryReview>(&input, &marks);
}

fn add_item(kind: &str, label: &str, sentence: &str) -> serde_json::Value {
    serde_json::json!({"proposal": {"kind": "add", "addition": {
        "evidence": [{"purpose": "assertion", "sentences": [sentence]}],
        "kind": kind,
        "label": label,
    }}})
}

fn items_answer(items: Vec<serde_json::Value>) -> String {
    serde_json::json!({ "items": items }).to_string()
}

#[tokio::test]
async fn a_proposal_the_rules_refuse_is_left_out_and_the_others_stand() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(items_answer(vec![
            add_item("constraint", "Deployment requires approval", "s1"),
            add_item("constraint", "The assistant will wait", "s2"),
        ]))],
        vec![Ok(verdict_response(&[("r1", "supported")]))],
        vec![Ok("Approval gate recorded.".to_string())],
    );

    let outcome = job(port.clone(), llm, small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.commit.inserts.len(), 1);
    assert_eq!(
        outcome.commit.inserts[0].label,
        "Deployment requires approval"
    );
    assert!(port.errors().is_empty());
    assert_eq!(port.watermark(), 4);
}

#[tokio::test]
async fn an_answer_whose_proposals_cannot_be_read_leaves_the_batch_unprocessed() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let unreadable = || {
        Ok(items_answer(vec![add_item(
            "constraint",
            "Deployment requires approval",
            "s99",
        )]))
    };
    let llm = FakeLlm::new(vec![unreadable(), unreadable()], vec![], vec![]);

    job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an answer with no proposal that stands cannot be committed");

    assert_eq!(llm.count(Task::Extract), 2, "asked once more");
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0);
    assert_eq!(port.errors(), vec!["malformed_json".to_string()]);
}

#[tokio::test]
async fn an_answer_whose_proposals_the_rules_all_refuse_leaves_the_batch_unprocessed() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let refused = || {
        Ok(items_answer(vec![add_item(
            "constraint",
            "The assistant will wait",
            "s2",
        )]))
    };
    let llm = FakeLlm::new(vec![refused(), refused()], vec![], vec![]);

    job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an answer with no proposal that stands cannot be committed");

    assert_eq!(llm.count(Task::Extract), 2, "asked once more");
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0);
    assert_eq!(port.errors(), vec!["malformed_json".to_string()]);
}

#[tokio::test]
async fn an_answer_with_no_proposals_is_an_empty_patch() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(items_answer(Vec::new()))],
        vec![],
        vec![Ok("Nothing new to record.".to_string())],
    );

    let outcome = job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert!(outcome.commit.updates.is_empty() && outcome.commit.inserts.is_empty());
    assert_eq!(llm.count(Task::Review), 0, "nothing to review");
    assert!(port.errors().is_empty());
}

#[tokio::test]
async fn an_answer_that_does_not_end_leaves_the_batch_unprocessed() {
    let whole = items_answer(vec![add_item(
        "constraint",
        "Deployment requires approval",
        "s1",
    )]);
    let cut = whole.trim_end_matches("]}").to_string();
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(vec![Ok(cut)], vec![], vec![]);

    job(port.clone(), llm.clone(), small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an answer that did not end cannot be committed");

    assert_eq!(llm.count(Task::Extract), 1);
    assert_eq!(port.commits(), 0);
    assert_eq!(port.watermark(), 0);
    assert_eq!(port.errors(), vec!["malformed_json".to_string()]);
}

#[tokio::test]
async fn the_quoted_span_is_the_pointed_sentence_found_by_the_app_among_repeats() {
    let mut messages = four_turns();
    messages[0] = message("m1", 1, SourceRole::User, "Hold the kiln. Hold the kiln.");
    let port = FakePort::new(messages, Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(items_answer(vec![add_item(
            "constraint",
            "The kiln is held",
            "s2",
        )]))],
        vec![Ok(verdict_response(&[("r1", "supported")]))],
        vec![Ok("Kiln held.".to_string())],
    );

    let outcome = job(port, llm, small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    let span = &outcome.commit.inserts[0].evidence[0];
    assert_eq!(span.message_id, "m1");
    assert_eq!((span.start_byte, span.end_byte), (15, 29));
}

#[tokio::test]
async fn a_supersede_proposal_retires_the_existing_item_by_its_handle() {
    let messages = four_turns();
    let port = FakePort::new(
        messages.clone(),
        vec![committed_constraint("item-old", &messages[0])],
        0,
    );
    let llm = FakeLlm::new(
        vec![Ok(items_answer(vec![serde_json::json!({"proposal": {
            "kind": "supersede",
            "evidence": [{"sentences": ["s3"]}],
            "item_id": "m1",
            "replacement": {
                "evidence": [{"purpose": "assertion", "sentences": ["s3"]}],
                "kind": "constraint",
                "label": "Staging may be deployed",
            },
        }})]))],
        vec![Ok(verdict_response(&[
            ("r1", "supported"),
            ("r2", "supported"),
        ]))],
        vec![Ok("Staging may be deployed.".to_string())],
    );

    let outcome = job(port.clone(), llm, small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .unwrap();

    assert_eq!(outcome.commit.inserts.len(), 1);
    assert_eq!(
        port.item("item-old").map(|item| item.state),
        Some(MemoryState::Superseded)
    );
}

#[test]
fn the_extraction_request_holds_no_identity_of_the_app() {
    use super::extract::{Context, Input, Memory, MemoryExtraction, Place, Segment, Sentence};
    use lattice_core::application::services::model_contract::shown::{Internal, Told};
    use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
    let mut marks = Marks::new();
    let sentence = |marks: &mut Marks, passage: usize, words: &str| Sentence {
        place: marks.internal(Place {
            message_id: "message-quay-3".to_owned(),
            passage,
            index: 0,
            start: 120_000,
            end: 120_040,
        }),
        text: marks.told(words.to_owned()),
    };
    let segment = Segment {
        sequence: Told(3),
        role: marks.told("user".to_owned()),
        whole_message: Told(true),
        context_before: marks.told("The tide table was posted at the quay.".to_owned()),
        context_after: marks.told("The kiln was cold by evening.".to_owned()),
        sentences: vec![sentence(&mut marks, 0, "The kiln closes at dusk.")],
    };
    let context = Context {
        sequence: Told(2),
        role: marks.told("assistant".to_owned()),
        truncated: Told(false),
        sentences: vec![sentence(&mut marks, 1, "The quay is open until noon.")],
    };
    let memory = Memory {
        id: marks.internal(MemoryId::from_string("memory-kiln-77").unwrap()),
        kind: marks.told("constraint".to_owned()),
        label: marks.told("The harbour gate stays shut".to_owned()),
        first_stated_at_sequence: Told(1),
        last_changed_at_sequence: Told(2),
    };
    let input = Input {
        segments: vec![segment],
        context: vec![context],
        memories: vec![memory],
        messages: Internal::of(Vec::new()),
        active: Internal::of(Vec::new()),
    };
    holds::<MemoryExtraction>(&input, &marks);
}

// ---------------------------------------------------------------------------
// Prompt snapshots
// ---------------------------------------------------------------------------

fn assert_snapshot(llm: &FakeLlm, id: &str) {
    let requests = llm
        .requests
        .lock()
        .iter()
        .filter(|request| {
            request
                .contract()
                .is_some_and(|call| call.id.as_str() == id)
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(!requests.is_empty(), "no request was sent for {id}");
    lattice_core::application::services::model_contract::test_support::snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        id,
        &requests,
    );
}

/// A pass over a chat with two committed items, one superseded and one
/// retired by the same message, and one forgotten fact.
async fn snapshot_pass() -> Arc<FakeLlm> {
    let messages = four_turns();
    let mut on_hold = committed_constraint("item-hold", &messages[0]);
    on_hold.label = "Staging deploys are on hold".to_string();
    let port = FakePort::new(
        messages.clone(),
        vec![committed_constraint("item-old", &messages[0]), on_hold],
        0,
    );
    port.inner
        .lock()
        .forgotten
        .push("The user's locker code".to_string());
    let quote = "Actually you may deploy staging.";
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(
            vec![proposed_constraint(
                "c1",
                "Staging deploys need no approval",
                "m3",
                quote,
            )],
            vec![
                proposed_supersession("item-old", "c1", "m3", quote),
                proposed_transition("item-hold", "m3", quote),
            ],
            &PREFIX_SEGMENTS,
        ))],
        vec![Ok(verdict_response(&[
            ("r1", "supported"),
            ("r2", "supported"),
            ("r3", "supported"),
        ]))],
        vec![Ok("Staging may be deployed.".to_string())],
    );
    job(port, llm.clone(), small_config())
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect("the pass commits");
    llm
}

#[tokio::test]
async fn prompt_snapshot_memory_extraction() {
    assert_snapshot(snapshot_pass().await.as_ref(), "memory-extraction");
}

#[tokio::test]
async fn prompt_snapshot_memory_review() {
    assert_snapshot(snapshot_pass().await.as_ref(), "memory-review");
}

#[tokio::test]
async fn prompt_snapshot_memory_fold() {
    assert_snapshot(snapshot_pass().await.as_ref(), "memory-fold");
}

#[tokio::test]
async fn prompt_snapshot_memory_forgetting() {
    assert_snapshot(snapshot_pass().await.as_ref(), "memory-forgetting");
}

#[tokio::test]
async fn prompt_snapshot_memory_shrink() {
    let port = FakePort::new(four_turns(), Vec::new(), 0);
    let llm = FakeLlm::new(
        vec![Ok(extraction_response(vec![], vec![], &PREFIX_SEGMENTS))],
        vec![],
        vec![
            Ok((0..40).map(|i| format!("word{i} ")).collect::<String>()),
            Ok("Approval gate; staging allowed.".to_string()),
        ],
    );
    let outcome = job(
        port,
        llm.clone(),
        CompactionConfig {
            summary_token_budget: 10,
            ..small_config()
        },
    )
    .run(CONVERSATION, CompactionRequest::default())
    .await
    .unwrap();
    assert!(outcome.summary_regenerated);
    assert_snapshot(&llm, "memory-shrink");
}
