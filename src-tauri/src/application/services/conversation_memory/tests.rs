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
use futures::stream::Stream;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::application::ports::conversation_memory::{
    CommittedMemorySnapshot, ConversationMemoryPort, MemoryCommitCandidate, MemoryCommitError,
    MemoryCommitPreconditions, ResolvedSpan, SourcePage, SourceReadLimits, SourceSpanRef,
};
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::application::ports::LLMPort;
use crate::domain::conversation_memory::{
    compute_digest, ConversationMemoryState, EvidencePurpose, EvidenceSpan, MemoryId, MemoryItem,
    MemoryKind, MemoryReview, MemorySnapshot, MemoryState, MemoryValidity, SourceMessage,
    SourceRole, MAX_EVIDENCE_PER_ITEM, MAX_OPERATIONS_PER_RESPONSE, MAX_PROPOSAL_BYTES,
    MAX_QUOTE_BYTES,
};
use crate::shared::error::{AppError, Result};

use super::segment::{clip_chars, segment_message};
use super::selection::select_boundary;
use super::{
    CompactionConfig, CompactionJob, CompactionRequest, CompactionStatus, COMPACTION_DEADLINE,
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
        }
        inner.state.memory_revision += 1;
        inner.state.processed_through_sequence = candidate.commit.processed_through_sequence;
        inner.state.source_transcript_revision = candidate.transcript_revision;
        inner.state.validity = MemoryValidity::Ready;
        inner.state.last_error_code = None;
        inner.operations.insert(preconditions.operation_id.clone());
        inner.committed.push(candidate.clone());

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

    fn request_limits(&self) -> Vec<(Option<String>, Option<u32>)> {
        self.request_limits.lock().clone()
    }

    fn reply(&self, prompt: &str) -> Result<String> {
        let task = if prompt.contains("\"task\":\"extract_memory\"") {
            Task::Extract
        } else if prompt.contains("\"task\":\"repair_memory_patch\"") {
            Task::Repair
        } else if prompt.contains("\"task\":\"review_memory_changes\"") {
            Task::Review
        } else if prompt.contains("\"task\":\"shrink_working_summary\"") {
            Task::Shrink
        } else {
            Task::Summarize
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
    fn supports_typed_completions(&self) -> bool {
        true
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.request_limits
            .lock()
            .push((request.reasoning_effort.clone(), request.max_output_tokens));
        let prompt = request
            .input
            .iter()
            .filter_map(|input| match input {
                crate::application::ports::llm_port::CompletionInput::Message { role, content }
                    if role == "user" =>
                {
                    Some(content.clone())
                }
                _ => None,
            })
            .next_back()
            .unwrap_or_default();
        Ok(CompletionResponse {
            text: self.reply(&prompt)?,
            ..Default::default()
        })
    }

    async fn generate(
        &self,
        prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        self.reply(prompt)
    }

    async fn generate_streaming(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Ok(Box::new(futures::stream::empty()))
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
    assert_eq!(COMPACTION_DEADLINE, Duration::from_secs(300));
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
    assert_eq!(config.deadline, COMPACTION_DEADLINE);
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
        "Return only JSON",
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
    for required in [
        "attempts that failed",
        "stored separately",
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
            deadline: Duration::ZERO,
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
            (Some("none".into()), Some(8_192)),
            (Some("none".into()), Some(8_192)),
            (Some("none".into()), Some(4_096)),
            (Some("none".into()), Some(8_192)),
        ],
        "every structured utility request must disable reasoning, and review gets the tighter cap"
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

#[tokio::test]
async fn an_oversized_summary_is_regenerated_once_then_rejected_with_no_partial_commit() {
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

    let error = job
        .run(CONVERSATION, CompactionRequest::default())
        .await
        .expect_err("an oversized summary cannot be committed");
    assert!(error.to_string().contains("budget"), "{error}");

    assert_eq!(llm.count(Task::Summarize), 1);
    assert_eq!(llm.count(Task::Shrink), 1, "exactly one regeneration");
    assert_eq!(port.commits(), 0, "no partial commit");
    assert_eq!(port.watermark(), 0);
    assert_eq!(port.errors(), vec!["summary_too_large".to_string()]);
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
