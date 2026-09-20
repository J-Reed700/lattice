//! Extraction and compaction orchestration for bounded conversation memory.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §6.3, §7, §8, §12, §15.
//!
//! ## What this module owns
//!
//! The model-facing half of memory maintenance: the prompts, the paging and
//! segmentation of source messages, the two model calls (extract, review), the
//! working-summary fold, the deadlines, and the single commit at the end.
//!
//! ## What it deliberately does not own
//!
//! Validation. Every rule about whether a quote is real, whether a source
//! belongs to this conversation, and what a transition may do lives in
//! [`crate::domain::conversation_memory`], and the atomic write lives behind
//! [`ConversationMemoryPort`]. This module calls them; it never re-decides them.
//!
//! ## The shape of a run
//!
//! One snapshot in, one commit out. Between those, everything is a candidate
//! held in memory: a cancelled, timed-out, or rejected run leaves the previously
//! committed state exactly as it was, and leaves the watermark where it was, so
//! the same source is offered again next time (§12).
//!
//! A run is bounded twice over — a five-minute deadline shared by every call,
//! and a work cap of 256 messages or 1 MiB of source per attempt. Hitting the
//! work cap returns [`CompactionStatus::Partial`] with the sequence to resume
//! from. It never skips the remainder (§7.5).

pub mod extract;
pub mod prompts;
pub mod segment;
pub mod summarize;
pub mod verify;

mod job;
mod selection;

pub use job::{CompactionJob, CompactionSlots};

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::application::ports::conversation_memory::{MemoryCommitError, SourceReadLimits};
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest};
use crate::application::ports::LLMPort;
use crate::domain::conversation_memory::{MemoryCommit, MemorySnapshot, MemoryValidationError};
use crate::shared::error::AppError;

// ---------------------------------------------------------------------------
// Limits (§7.5). Named so tests assert the contract rather than a literal.
// ---------------------------------------------------------------------------

/// Wall-clock allowance for one whole compaction, shared across every batch,
/// repair and review call.
pub const COMPACTION_DEADLINE: Duration = Duration::from_secs(5 * 60);

/// Repairs allowed per rejected model response. Small local models sometimes
/// fix one invalid field while leaving a second one behind; a second bounded
/// repair lets them converge without weakening deterministic validation.
pub const MAX_REPAIR_ATTEMPTS: usize = 2;

/// Model calls allowed for one batch: the first attempt plus its repair.
pub const MAX_EXTRACTION_ATTEMPTS: usize = MAX_REPAIR_ATTEMPTS + 1;

/// Fresh-snapshot retries allowed after a commit conflict (§11.2).
pub const MAX_COMMIT_CONFLICT_RETRIES: usize = 1;

/// Source messages processed in one foreground attempt.
pub const MAX_SOURCE_MESSAGES_PER_ATTEMPT: usize = 256;

/// Source bytes processed in one foreground attempt.
pub const MAX_SOURCE_BYTES_PER_ATTEMPT: usize = 1024 * 1024;

/// Complete turns kept out of the compacted prefix (§7.2). The current pending
/// user message is retained on top of these.
pub const MIN_RETAINED_TURNS: usize = 2;

/// Messages kept raw when the caller asks for no particular number.
pub const DEFAULT_KEEP_RECENT_MESSAGES: usize = 6;

/// Do not start a model call that cannot plausibly finish in what is left.
const MIN_CALL_SLICE: Duration = Duration::from_millis(500);

/// Characters of an adjacent message shown beside a batch.
const MAX_CONTEXT_PASSAGE_CHARS: usize = 1_000;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a compaction did not complete.
///
/// Every variant is recoverable in the same sense: the previously committed
/// memory, the working summary and the original transcript are all still
/// exactly as they were. Nothing here can leave a half-written ledger, because
/// there is only ever one write.
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("compaction exceeded its {0:?} deadline; committed memory is unchanged")]
    DeadlineExceeded(Duration),
    #[error("compaction was cancelled; committed memory is unchanged")]
    Cancelled,
    #[error("the model's memory patch was rejected: {0}")]
    Rejected(#[source] MemoryValidationError),
    #[error("the utility model could not be used for memory extraction: {0}")]
    Model(String),
    #[error("memory could not be committed: {0}")]
    Commit(#[source] MemoryCommitError),
    #[error("the new working summary needs {tokens} tokens but its budget is {budget}")]
    SummaryTooLarge { tokens: usize, budget: usize },
    #[error("the summariser returned no working summary")]
    SummaryEmpty,
    #[error(
        "the reviewer could not support what was extracted from this source ({0}); \
         it stays unprocessed rather than recorded as covered"
    )]
    ReviewUnresolved(&'static str),
    #[error("{0}")]
    Port(#[source] AppError),
}

impl CompactionError {
    /// Stable short code for `last_error_code` and structured logs. Never
    /// contains transcript text.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DeadlineExceeded(_) => "deadline_exceeded",
            Self::Cancelled => "cancelled",
            Self::Rejected(error) => error.code(),
            Self::Model(_) => "utility_model_unavailable",
            Self::Commit(error) => error.code(),
            Self::SummaryTooLarge { .. } => "summary_too_large",
            Self::SummaryEmpty => "summary_empty",
            Self::ReviewUnresolved(code) => code,
            Self::Port(_) => "memory_port_error",
        }
    }
}

impl From<AppError> for CompactionError {
    fn from(error: AppError) -> Self {
        Self::Port(error)
    }
}

impl From<CompactionError> for AppError {
    fn from(error: CompactionError) -> Self {
        let message = error.to_string();
        match error {
            CompactionError::Port(inner) => inner,
            CompactionError::Model(_) => AppError::ServiceNotAvailable(message),
            CompactionError::Rejected(_) => AppError::InvalidData(message),
            CompactionError::Commit(inner) if inner.is_retryable() => {
                AppError::ConcurrentModification {
                    resource: "conversation memory".into(),
                    details: message,
                }
            }
            CompactionError::Commit(MemoryCommitError::Database(_)) => AppError::Database(message),
            _ => AppError::Other(message),
        }
    }
}

// ---------------------------------------------------------------------------
// Deadline
// ---------------------------------------------------------------------------

/// The one clock every call in a run shares (§7.5).
///
/// Per-call timeouts would let a job with enough batches run indefinitely. This
/// is checked before each call and used as that call's own time budget, so the
/// last batch cannot outlive the job.
pub struct Deadline {
    expires_at: Instant,
    total: Duration,
    cancellation: Option<CancellationToken>,
}

impl Deadline {
    pub fn new(total: Duration, cancellation: Option<CancellationToken>) -> Self {
        Self {
            expires_at: Instant::now() + total,
            total,
            cancellation,
        }
    }

    /// Whether the run may continue at all.
    ///
    /// # Errors
    ///
    /// [`CompactionError::Cancelled`] or [`CompactionError::DeadlineExceeded`].
    pub fn check(&self) -> std::result::Result<(), CompactionError> {
        if self.is_cancelled() {
            return Err(CompactionError::Cancelled);
        }
        if self.left().is_zero() {
            return Err(CompactionError::DeadlineExceeded(self.total));
        }
        Ok(())
    }

    /// Time to give one model call, or an error when there is not enough left to
    /// bother starting it.
    pub fn slice(&self) -> std::result::Result<Duration, CompactionError> {
        self.check()?;
        let left = self.left();
        if left < MIN_CALL_SLICE {
            return Err(CompactionError::DeadlineExceeded(self.total));
        }
        Ok(left)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    }

    fn left(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }
}

/// Run one prompt against the utility model inside the shared deadline.
///
/// Typed completions carry the JSON schema, which lets a provider reject a
/// malformed response one round trip before the parser would. Providers without
/// them fall back to the text API, where the schema only exists as prose in the
/// prompt — the deterministic validator is what actually enforces it either way.
async fn complete_json(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    system: &str,
    prompt: String,
    schema: Option<serde_json::Value>,
) -> std::result::Result<String, CompactionError> {
    let remaining = deadline.slice()?;
    let call = async move {
        if llm.supports_typed_completions() {
            let request = CompletionRequest {
                input: vec![
                    CompletionInput::Message {
                        role: "system".into(),
                        content: system.to_string(),
                    },
                    CompletionInput::Message {
                        role: "user".into(),
                        content: prompt,
                    },
                ],
                json_schema: schema,
                // Extraction, transition review, and summary synthesis are
                // bounded utility operations. Reasoning-capable local models
                // otherwise may spend the entire compaction deadline on a
                // hidden chain of thought before returning any JSON.
                reasoning_effort: Some("none".into()),
                // The deterministic parser applies the tighter byte and
                // operation bounds after generation. This provider-side cap
                // prevents a malformed or uncooperative response from using
                // the model's much larger configured chat-output allowance.
                max_output_tokens: Some(if system == prompts::VERIFIER_SYSTEM {
                    4_096
                } else {
                    8_192
                }),
                time_budget: Some(remaining),
                ..Default::default()
            };
            llm.complete(&request).await.map(|response| response.text)
        } else {
            llm.generate(&prompt, &[format!("System: {system}")], None)
                .await
        }
    };

    let outcome = match deadline.cancellation.clone() {
        Some(token) => tokio::select! {
            biased;
            _ = token.cancelled() => return Err(CompactionError::Cancelled),
            outcome = tokio::time::timeout(remaining, call) => outcome,
        },
        None => tokio::time::timeout(remaining, call).await,
    };
    match outcome {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(error)) => Err(CompactionError::Model(error.to_string())),
        Err(_) => Err(CompactionError::DeadlineExceeded(deadline.total)),
    }
}

// ---------------------------------------------------------------------------
// Configuration, request, outcome
// ---------------------------------------------------------------------------

/// Tuning for one job instance. Every field has a §7.5 default; tests shrink
/// them to reach a limit without building a huge fixture.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Tokens the new working summary may occupy in the **continuation** model's
    /// pool. The job does not compute this; the budget owner supplies it.
    pub summary_token_budget: usize,
    pub deadline: Duration,
    /// Ceiling on one repository page read.
    pub source_read_limits: SourceReadLimits,
    /// Work cap for one attempt (§7.5).
    pub max_source_messages_per_attempt: usize,
    pub max_source_bytes_per_attempt: usize,
    pub segment_max_bytes: usize,
    pub segment_overlap_chars: usize,
    pub max_segments_per_batch: usize,
    pub max_batch_bytes: usize,
    /// Minimum messages kept raw when a request names no number (§7.2).
    pub keep_recent_messages: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            summary_token_budget: 1_024,
            deadline: COMPACTION_DEADLINE,
            source_read_limits: SourceReadLimits::DEFAULT,
            max_source_messages_per_attempt: MAX_SOURCE_MESSAGES_PER_ATTEMPT,
            max_source_bytes_per_attempt: MAX_SOURCE_BYTES_PER_ATTEMPT,
            segment_max_bytes: segment::SEGMENT_MAX_BYTES,
            segment_overlap_chars: segment::SEGMENT_OVERLAP_CHARS,
            max_segments_per_batch: segment::MAX_SEGMENTS_PER_BATCH,
            max_batch_bytes: segment::MAX_BATCH_BYTES,
            keep_recent_messages: DEFAULT_KEEP_RECENT_MESSAGES,
        }
    }
}

/// What asked for this compaction. Only affects the recorded operation label and
/// logging; the limits are the same either way, because an interactive `/compact`
/// that runs for five minutes is as bad as a background one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactionTrigger {
    /// The user ran `/compact`.
    #[default]
    Manual,
    /// Context assembly would otherwise have dropped unprocessed source.
    Automatic,
}

/// One compaction request.
#[derive(Debug, Clone, Default)]
pub struct CompactionRequest {
    pub trigger: CompactionTrigger,
    /// Idempotency key. Reuse it to retry the same user action safely: a commit
    /// that already landed is returned rather than applied twice.
    pub operation_id: Option<String>,
    /// Minimum raw messages to retain, adjusted outward to a turn boundary.
    pub keep_recent_messages: Option<usize>,
    pub cancellation: Option<CancellationToken>,
}

/// How far a run got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionStatus {
    /// Nothing eligible sat outside the retained tail.
    NothingToCompact,
    /// Everything selected was processed and committed.
    Committed,
    /// Committed what was accepted; source remains, and
    /// `resume_after_sequence` says where to continue.
    Partial,
}

/// The result of a run, including the diagnostics §13 asks for.
#[derive(Debug, Clone)]
pub struct CompactionOutcome {
    pub status: CompactionStatus,
    pub memory_revision: i64,
    pub transcript_revision: i64,
    pub processed_through_sequence: i64,
    /// Last message folded into the summary, when one was written.
    pub boundary_message_id: Option<String>,
    pub active_mandatory_count: usize,
    pub active_optional_count: usize,
    pub summary: Option<String>,
    pub summary_tokens: usize,
    pub summary_regenerated: bool,
    /// Exactly what was handed to the port. Kept so callers and tests can see
    /// the decisions a run made without a second read.
    pub commit: MemoryCommit,
    pub source_messages_read: usize,
    pub segments_submitted: usize,
    /// Segments whose batch was rejected twice. The watermark stops before them.
    pub segments_rejected: Vec<String>,
    pub rejection_code: Option<String>,
    pub extraction_calls: usize,
    pub repaired_batches: usize,
    pub review_calls: usize,
    /// True when a review could not be obtained and its subjects were recorded
    /// ambiguous rather than supported.
    pub review_degraded: bool,
    /// Host-owned `UnresolvedChange` items this run created.
    pub conflicts_recorded: usize,
    pub commit_conflict_retries: usize,
    /// Sequence to resume from when `status` is [`CompactionStatus::Partial`].
    pub resume_after_sequence: Option<i64>,
    pub was_already_committed: bool,
}

impl CompactionOutcome {
    /// The outcome for a conversation with nothing to do.
    fn nothing(snapshot: &MemorySnapshot) -> Self {
        Self {
            status: CompactionStatus::NothingToCompact,
            memory_revision: snapshot.state.memory_revision,
            transcript_revision: snapshot.transcript_revision,
            processed_through_sequence: snapshot.state.processed_through_sequence,
            boundary_message_id: None,
            active_mandatory_count: snapshot.mandatory_items().len(),
            active_optional_count: snapshot.optional_items().len(),
            summary: snapshot.summary.clone(),
            summary_tokens: 0,
            summary_regenerated: false,
            commit: MemoryCommit::default(),
            source_messages_read: 0,
            segments_submitted: 0,
            segments_rejected: Vec::new(),
            rejection_code: None,
            extraction_calls: 0,
            repaired_batches: 0,
            review_calls: 0,
            review_degraded: false,
            conflicts_recorded: 0,
            commit_conflict_retries: 0,
            resume_after_sequence: None,
            was_already_committed: false,
        }
    }
}

/// Counts tokens the way the *continuation* model does.
///
/// Injected because the budget owner is the context assembler, not this module:
/// measuring a summary against the utility model's tokenizer would authorise a
/// summary that does not fit the prompt it has to travel in (§7.3 step 9).
pub type TokenCounter = Arc<dyn Fn(&str) -> usize + Send + Sync>;
