//! Extraction and compaction orchestration for bounded conversation memory
//! (design: `docs/design/2026-09-19-conversation-memory.md` §6.3, §7, §8, §12,
//! §15): the prompts, paging and segmentation, the extract and review calls,
//! the working-summary fold and the single commit. Validation lives in
//! [`lattice_core::domain::conversation::memory`] and the atomic write behind
//! [`ConversationMemoryPort`]; this module never re-decides them.
//!
//! One snapshot in, one commit out: a cancelled or rejected run leaves the
//! committed state and the watermark as they were. A run has no clock of its
//! own; the per-attempt work cap returns [`CompactionStatus::Partial`] with a
//! resume point. Extraction advances the watermark on every run, but the
//! summary is refolded only when asked ([`CompactionRequest::summary_refold`]),
//! because the prompt opens its window where the summary ends.

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

use lattice_core::application::ports::conversation_memory::{MemoryCommitError, SourceReadLimits};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, InferencePriority, SamplingOverride,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::memory::{
    MemoryCommit, MemorySnapshot, MemoryValidationError, SummaryRefold,
};
use lattice_core::shared::error::AppError;

// ---------------------------------------------------------------------------
// Limits (§7.5). Named so tests assert the contract rather than a literal.
// ---------------------------------------------------------------------------

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

/// Source bytes one attempt folds back into a dropped summary, so one page adds
/// a bounded amount of summariser work to the attempt it rides on. A page, not
/// a cut: consolidation folds page after page until the summary is caught up
/// or it is stopped, and the next run resumes where this one stopped.
pub const MAX_REFOLD_BYTES_PER_ATTEMPT: usize = 32 * 1024;

/// Complete turns kept out of the compacted prefix (§7.2). The current pending
/// user message is retained on top of these.
pub const MIN_RETAINED_TURNS: usize = 2;

/// Messages kept raw when the caller asks for no particular number.
pub const DEFAULT_KEEP_RECENT_MESSAGES: usize = 6;

/// Do not start a model call that cannot plausibly finish in what is left.
const MIN_CALL_SLICE: Duration = Duration::from_millis(500);

/// Provider-side answer caps for the utility calls. They stop a runaway
/// response; the deterministic parsers apply the real bounds after it.
const EXTRACTOR_OUTPUT_TOKENS: usize = 8_192;
const VERIFIER_OUTPUT_TOKENS: usize = 4_096;

/// Characters a utility call has for its material in the utility model's own
/// window: what is left after the answer room (`output_tokens`, never more
/// than a quarter of the window), a 5% margin, the system prompt and
/// `framing`, the request text the material is set into.
fn call_room_chars(llm: &dyn LLMPort, system: &str, output_tokens: usize, framing: &str) -> usize {
    let window = llm.max_context_tokens();
    let room = window
        .saturating_sub(output_tokens.min(window / 4))
        .saturating_sub(window / 20)
        .saturating_sub(llm.count_tokens(system))
        .saturating_sub(llm.count_tokens(framing));
    lattice_core::application::ports::llm_port::chars_within_tokens(room, llm.chars_per_token())
}

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

/// The cancellation, and the one clock if the run has one, that every call
/// in a run shares (§7.5).
///
/// With a clock, it is checked before each call and used as that call's own
/// time budget, so the last batch cannot outlive the job. Without one, calls
/// end only by cancellation.
pub struct Deadline {
    expires_at: Option<Instant>,
    total: Option<Duration>,
    cancellation: Option<CancellationToken>,
    /// How the run's model calls queue at the backend.
    priority: InferencePriority,
    /// The conversation's memory key, so its calls share one slot and keep
    /// clear of the chat's own.
    cache_key: Option<String>,
}

impl Deadline {
    pub fn new(total: Option<Duration>, cancellation: Option<CancellationToken>) -> Self {
        Self {
            expires_at: total.map(|total| Instant::now() + total),
            total,
            cancellation,
            priority: InferencePriority::Maintenance,
            cache_key: None,
        }
    }

    /// Queue this run's model calls at `priority`, keyed to `conversation_id`'s
    /// memory. Not the conversation's own key: consolidation runs between
    /// turns, and on the chat's slot it would overwrite the cached prefix the
    /// next turn reuses.
    pub fn for_conversation(mut self, priority: InferencePriority, conversation_id: &str) -> Self {
        self.priority = priority;
        self.cache_key = Some(format!("memory:{conversation_id}"));
        self
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
        if self.left().is_some_and(|left| left.is_zero()) {
            return Err(self.exceeded());
        }
        Ok(())
    }

    /// Time to give one model call (`None`: no limit), or an error when there
    /// is not enough left to bother starting it.
    pub fn slice(&self) -> std::result::Result<Option<Duration>, CompactionError> {
        self.check()?;
        match self.left() {
            Some(left) if left < MIN_CALL_SLICE => Err(self.exceeded()),
            left => Ok(left),
        }
    }

    fn exceeded(&self) -> CompactionError {
        CompactionError::DeadlineExceeded(self.total.unwrap_or_default())
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    }

    /// What is left of the clock; `None` when the run has none.
    fn left(&self) -> Option<Duration> {
        self.expires_at
            .map(|expires_at| expires_at.saturating_duration_since(Instant::now()))
    }
}

/// Run one prompt against the utility model inside the shared deadline.
///
/// The request carries the JSON schema, which lets a provider reject a
/// malformed response one round trip before the parser would; the
/// deterministic validator is what actually enforces it.
async fn complete_json(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    system: &str,
    prompt: String,
    schema: Option<serde_json::Value>,
) -> std::result::Result<String, CompactionError> {
    complete_json_within(llm, deadline, system, prompt, schema, 0).await
}

/// [`complete_json`] with room for at least `min_output_tokens` of answer:
/// a summary sized from a large window must not be cut off by the provider
/// cap that stops a runaway response.
async fn complete_json_within(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    system: &str,
    prompt: String,
    schema: Option<serde_json::Value>,
    min_output_tokens: usize,
) -> std::result::Result<String, CompactionError> {
    let remaining = deadline.slice()?;
    let max_output_tokens = if system == prompts::VERIFIER_SYSTEM {
        VERIFIER_OUTPUT_TOKENS
    } else {
        EXTRACTOR_OUTPUT_TOKENS
    }
    .max(min_output_tokens);
    let phase = if system == prompts::EXTRACTOR_SYSTEM {
        "extractor_model_call"
    } else if system == prompts::VERIFIER_SYSTEM {
        "reviewer_model_call"
    } else if system == prompts::SUMMARIZER_SYSTEM {
        "summarizer_model_call"
    } else {
        "other_model_call"
    };
    let started = std::time::Instant::now();
    tracing::info!(
        phase,
        provider = llm.provider_name(),
        timeout_ms = remaining.map(|remaining| remaining.as_millis() as u64),
        max_output_tokens,
        "Memory utility model call started"
    );
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
        // Greedy decoding, so the same passages always yield the
        // same patch rather than depending on the chat temperature.
        sampling: Some(SamplingOverride::deterministic()),
        // The deterministic parser applies the tighter byte and
        // operation bounds after generation. This provider-side cap
        // prevents a malformed or uncooperative response from using
        // the model's much larger configured chat-output allowance.
        max_output_tokens: Some(u32::try_from(max_output_tokens).unwrap_or(u32::MAX)),
        time_budget: remaining,
        no_time_limit: remaining.is_none(),
        priority: deadline.priority,
        cancel: deadline.cancellation.clone(),
        cache_key: deadline.cache_key.clone(),
        ..Default::default()
    };
    let call = llm.complete(&request);

    let bounded = async {
        match remaining {
            Some(remaining) => tokio::time::timeout(remaining, call).await,
            None => Ok(call.await),
        }
    };
    let outcome = match deadline.cancellation.clone() {
        Some(token) => tokio::select! {
            biased;
            _ = token.cancelled() => return Err(CompactionError::Cancelled),
            outcome = bounded => outcome,
        },
        None => bounded.await,
    };
    match outcome {
        Ok(Ok(response)) => {
            tracing::info!(
                phase,
                outcome = "completed",
                provider = llm.provider_name(),
                finish_reason = %response.finish_reason,
                output_tokens = response.output_tokens,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Memory utility model call finished"
            );
            Ok(response.text)
        }
        Ok(Err(error)) => {
            tracing::warn!(
                phase,
                outcome = "provider_error",
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Memory utility model call failed"
            );
            Err(CompactionError::Model(error.to_string()))
        }
        Err(_) => {
            tracing::warn!(
                phase,
                outcome = "deadline_timeout",
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Memory utility model call timed out"
            );
            Err(deadline.exceeded())
        }
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
    /// A wall clock for the whole run, for a caller that has one to lend;
    /// `None`, the default, ends the run only by cancellation.
    pub deadline: Option<Duration>,
    /// Ceiling on one repository page read.
    pub source_read_limits: SourceReadLimits,
    /// Work cap for one attempt (§7.5).
    pub max_source_messages_per_attempt: usize,
    pub max_source_bytes_per_attempt: usize,
    /// Source bytes one attempt may fold back into a summary that a delete or a
    /// user correction dropped. Smaller than the work cap on purpose: each page
    /// commits on its own, so consolidation can stop between pages when it is
    /// stopped, and a later run picks up where it stopped.
    pub max_refold_bytes_per_attempt: usize,
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
            deadline: None,
            source_read_limits: SourceReadLimits::DEFAULT,
            max_source_messages_per_attempt: MAX_SOURCE_MESSAGES_PER_ATTEMPT,
            max_source_bytes_per_attempt: MAX_SOURCE_BYTES_PER_ATTEMPT,
            max_refold_bytes_per_attempt: MAX_REFOLD_BYTES_PER_ATTEMPT,
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
    /// Consolidate completed turns even before the context window fills.
    Maintenance,
}

impl CompactionTrigger {
    /// How the run's model calls queue. A manual `/compact` and the compaction
    /// a turn waits on before it can generate both have someone watching;
    /// consolidation after a saved answer does not.
    pub fn priority(self) -> InferencePriority {
        match self {
            Self::Manual | Self::Automatic => InferencePriority::Interactive,
            Self::Maintenance => InferencePriority::Maintenance,
        }
    }
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
    /// The refold a turn's prompt asked for, when its window had to drop
    /// turns the working summary does not cover. Consolidation refolds only
    /// on this; `/compact` and the compaction a turn waits on fold everything
    /// they extract without it.
    pub summary_refold: Option<SummaryRefold>,
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
    /// A fold over source below the watermark stopped at the work cap: this
    /// run folded a page of it, and more remain.
    pub refold_pending: bool,
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
            refold_pending: false,
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
