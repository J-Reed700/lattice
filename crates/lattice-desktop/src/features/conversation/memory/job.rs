//! One compaction run, from snapshot to commit.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §7.
//!
//! Split out of the module façade because the run is the long part: boundary
//! selection, the batch loop, the candidate ledger a run builds before it is
//! allowed to write anything, and the single commit that publishes it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex as SyncMutex;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};

use lattice_core::application::ports::conversation_memory::ConversationMemoryPort;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::memory::{
    MemoryCommit, MemoryItem, MemorySnapshot, MemoryState, SourceMessage, SuppressedSpan,
    SuppressionsByMessage,
};
use lattice_core::shared::error::Result;

use super::segment::SourceSegment;
use super::{
    CompactionConfig, CompactionError, CompactionOutcome, CompactionRequest, CompactionStatus,
    CompactionTrigger, Deadline, TokenCounter, MAX_COMMIT_CONFLICT_RETRIES,
};

mod attempt;
mod reads;

/// Extraction and compaction for one application.
///
/// Holds the per-conversation single-flight slots, so one instance is shared by
/// every caller. Two instances would each hold their own slots and the lock
/// would mean nothing.
pub struct CompactionJob {
    memory: Arc<dyn ConversationMemoryPort>,
    utility_llm: Arc<dyn LLMPort>,
    count_tokens: TokenCounter,
    config: CompactionConfig,
    slots: Arc<CompactionSlots>,
}

/// Privacy-safe timing for one compaction phase. A drop without `finish` means
/// the future was cancelled while the phase was running.
struct PhaseTiming {
    conversation_id: String,
    phase: &'static str,
    started: Instant,
    finished: bool,
}

impl PhaseTiming {
    fn start(conversation_id: &str, phase: &'static str) -> Self {
        info!(conversation_id, phase, "Memory compaction phase started");
        Self {
            conversation_id: conversation_id.to_string(),
            phase,
            started: Instant::now(),
            finished: false,
        }
    }

    fn finish(mut self, outcome: &'static str) {
        info!(
            conversation_id = %self.conversation_id,
            phase = self.phase,
            outcome,
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            "Memory compaction phase finished"
        );
        self.finished = true;
    }
}

impl Drop for PhaseTiming {
    fn drop(&mut self) {
        if !self.finished {
            warn!(
                conversation_id = %self.conversation_id,
                phase = self.phase,
                outcome = "cancelled_or_interrupted",
                elapsed_ms = self.started.elapsed().as_millis() as u64,
                "Memory compaction phase interrupted"
            );
        }
    }
}

/// The per-conversation single-flight slots.
///
/// Held separately from the job, and shared, because the two things have
/// different lifetimes. The slots must be process-wide or the mutual exclusion
/// means nothing; the utility model and the continuation model's token counter
/// are per-turn values that change when the user switches models, so a job
/// pinned to one of them at startup would be pinned to a stale budget.
///
/// Build [`CompactionSlots`] once, in DI, and hand the same `Arc` to every job.
#[derive(Default)]
pub struct CompactionSlots {
    /// One async mutex per conversation. Different conversations never wait on
    /// each other; the same conversation never compacts twice at once.
    slots: SyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl CompactionJob {
    /// A job with its own private slots.
    ///
    /// For tests and one-off use. Production goes through
    /// [`Self::with_slots`]: two instances with separate slots do not exclude
    /// each other, which is the whole point of holding one.
    pub fn new(
        memory: Arc<dyn ConversationMemoryPort>,
        utility_llm: Arc<dyn LLMPort>,
        count_tokens: TokenCounter,
        config: CompactionConfig,
    ) -> Self {
        Self::with_slots(
            memory,
            utility_llm,
            count_tokens,
            config,
            Arc::new(CompactionSlots::default()),
        )
    }

    /// A job sharing process-wide slots with every other job.
    pub fn with_slots(
        memory: Arc<dyn ConversationMemoryPort>,
        utility_llm: Arc<dyn LLMPort>,
        count_tokens: TokenCounter,
        config: CompactionConfig,
        slots: Arc<CompactionSlots>,
    ) -> Self {
        Self {
            memory,
            utility_llm,
            count_tokens,
            config,
            slots,
        }
    }

    pub fn config(&self) -> &CompactionConfig {
        &self.config
    }

    /// Extract memory from unprocessed source and publish one new revision.
    ///
    /// Takes the conversation's single-flight slot, reads one snapshot, pages
    /// the unprocessed range, extracts and reviews it batch by batch, folds the
    /// working summary, and commits once. On failure nothing is written except
    /// the error code on the state row.
    ///
    /// # Errors
    ///
    /// Any [`CompactionError`], converted to an [`AppError`]. All of them leave
    /// the previously committed memory usable and the watermark where it was.
    pub async fn run(
        &self,
        conversation_id: &str,
        request: CompactionRequest,
    ) -> Result<CompactionOutcome> {
        let slot = self.slot(conversation_id);
        let operation_id = request
            .operation_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let outcome = {
            // Cancellation while queued must release the slot rather than wait
            // for a run that is no longer wanted.
            let guard = match &request.cancellation {
                Some(token) => tokio::select! {
                    biased;
                    _ = token.cancelled() => None,
                    guard = slot.lock() => Some(guard),
                },
                None => Some(slot.lock().await),
            };
            match guard {
                None => Err(CompactionError::Cancelled),
                Some(_held) => {
                    // The deadline starts when the work does. Waiting behind
                    // another compaction of the same conversation is not this
                    // run's fault, and charging it would fail the queued request
                    // for a reason the user cannot see.
                    let deadline =
                        Deadline::new(self.config.deadline, request.cancellation.clone())
                            .for_conversation(request.trigger.priority(), conversation_id);
                    self.run_pages(conversation_id, &request, &operation_id, &deadline)
                        .await
                }
            }
        };
        self.release(conversation_id, slot);

        match outcome {
            Ok(outcome) => Ok(outcome),
            // A cancellation is a user decision, not a fault: recording it would
            // show the conversation as broken in the memory details view.
            Err(CompactionError::Cancelled) => Err(CompactionError::Cancelled.into()),
            Err(error) => {
                let code = error.code();
                if let Err(write) = self.memory.record_memory_error(conversation_id, code).await {
                    warn!(
                        conversation_id,
                        code,
                        error = %write,
                        "Could not record the memory error code"
                    );
                }
                Err(error.into())
            }
        }
    }

    /// The conversation's slot, created on first use.
    fn slot(&self, conversation_id: &str) -> Arc<AsyncMutex<()>> {
        self.slots
            .slots
            .lock()
            .entry(conversation_id.to_string())
            .or_default()
            .clone()
    }

    /// Drop the slot when this call was the last holder.
    ///
    /// A strong count of two means only the map and this local hold it, so no
    /// waiter can lose its place; anyone arriving later inserts a fresh slot.
    fn release(&self, conversation_id: &str, slot: Arc<AsyncMutex<()>>) {
        let mut slots = self.slots.slots.lock();
        if slots
            .get(conversation_id)
            .is_some_and(|existing| Arc::strong_count(existing) == 2)
        {
            slots.remove(conversation_id);
        }
        drop(slot);
    }

    /// [`Self::run_with_retry`], again for each further page of a fold over
    /// source below the watermark, while a run with a deadline has room for
    /// one.
    ///
    /// Only consolidation keeps going: it runs after an answer with no one
    /// waiting, while a `/compact` or the compaction a turn waits on takes one
    /// page as before. Each page commits on its own, so stopping early loses
    /// nothing and the next run resumes from the summary's boundary. A page
    /// needs room for twice what the last one took; one the deadline or a
    /// cancellation cuts short anyway commits nothing, and the run reports
    /// the pages before it rather than a failure.
    async fn run_pages(
        &self,
        conversation_id: &str,
        request: &CompactionRequest,
        operation_id: &str,
        deadline: &Deadline,
    ) -> std::result::Result<CompactionOutcome, CompactionError> {
        let started = Instant::now();
        let mut outcome = self
            .run_with_retry(conversation_id, request, operation_id, deadline)
            .await?;
        let mut last_page = started.elapsed();
        let mut page = 1usize;
        while request.trigger == CompactionTrigger::Maintenance
            && outcome.refold_pending
            && deadline
                .left()
                .is_none_or(|left| left >= last_page.saturating_mul(2))
        {
            page += 1;
            let started = Instant::now();
            // Its own idempotency key: under the run's, the commit would be
            // taken for a retry of the first page and not applied.
            let page_operation = format!("{operation_id}/refold-{page}");
            let next = match self
                .run_with_retry(conversation_id, request, &page_operation, deadline)
                .await
            {
                Ok(next) => next,
                Err(CompactionError::DeadlineExceeded(_) | CompactionError::Cancelled) => break,
                Err(other) => return Err(other),
            };
            // A page that ends where the last one did made no progress, and
            // asking again would only repeat it.
            if next.status == CompactionStatus::NothingToCompact
                || next.boundary_message_id == outcome.boundary_message_id
            {
                break;
            }
            outcome = next;
            last_page = started.elapsed();
        }
        Ok(outcome)
    }

    /// One fresh-snapshot retry after a commit conflict (§11.2).
    async fn run_with_retry(
        &self,
        conversation_id: &str,
        request: &CompactionRequest,
        operation_id: &str,
        deadline: &Deadline,
    ) -> std::result::Result<CompactionOutcome, CompactionError> {
        let mut retries = 0usize;
        loop {
            match self
                .attempt(conversation_id, request, operation_id, deadline, retries)
                .await
            {
                Err(CompactionError::Commit(error))
                    if error.is_retryable() && retries < MAX_COMMIT_CONFLICT_RETRIES =>
                {
                    retries += 1;
                    warn!(
                        conversation_id,
                        code = error.code(),
                        "Memory commit conflicted — one retry from a fresh snapshot"
                    );
                    deadline.check()?;
                }
                other => return other,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Summary refold
// ---------------------------------------------------------------------------

/// Where the working summary's coverage ends, never past the watermark.
///
/// Usually behind the watermark: extraction runs after every answer, and the
/// summary moves only when a run folds it ([`FoldTarget`]). A delete also drops
/// a summary whose boundary is at or after the deleted message (by trigger, or
/// by its `up_to_message_id` cascade), and a user correction clears it, while
/// the ledger stays usable and the watermark stays put. With no summary every
/// eligible message at or below the watermark is covered by extraction and by
/// no summary, so the next fold starts from the beginning. A summary whose
/// boundary cannot be placed is trusted to reach the watermark, as it always
/// was.
fn summary_resume_point(snapshot: &MemorySnapshot, start_after: i64) -> i64 {
    snapshot.summary_covers_through().min(start_after)
}

/// What a run does with the working summary.
///
/// The prompt opens its verbatim window where the summary ends, so a summary
/// that moved after every answer would change the head of every request.
/// Consolidation after an answer therefore refolds only when that turn's
/// prompt asked: its window had to drop turns the summary does not cover. A
/// request a refold since then already satisfied is not repeated. `/compact`
/// and the compaction a turn waits on fold everything they extract, as they
/// always have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FoldTarget {
    /// Leave the summary as it is; extraction moves on without it.
    Keep,
    /// Refold through this sequence.
    Through(i64),
    /// Fold everything this run extracts.
    Extracted,
}

impl FoldTarget {
    /// The fold `request` asks for, with the summary covering through `covered`.
    fn for_run(request: &CompactionRequest, covered: i64) -> Self {
        match (request.summary_refold, request.trigger) {
            (Some(refold), _) if refold.is_needed(covered) => {
                Self::Through(refold.through_sequence)
            }
            (Some(_), _) | (None, CompactionTrigger::Maintenance) => Self::Keep,
            (None, CompactionTrigger::Manual | CompactionTrigger::Automatic) => Self::Extracted,
        }
    }

    /// The end of the source below the watermark this fold reads back, after
    /// `covered` and through at most `start_after`, or `None` when it reads
    /// none. Earlier runs extracted it; a fold reads it again, never
    /// re-extracts it.
    fn below_watermark(self, covered: i64, start_after: i64) -> Option<i64> {
        let through = match self {
            Self::Keep => return None,
            Self::Through(through) => through.min(start_after),
            Self::Extracted => start_after,
        };
        Some(through).filter(|through| *through > covered)
    }

    /// Where the fold ends once extraction accepted through
    /// `accepted_through`, or `None` when the summary stays as it is. It never
    /// runs past the watermark: a message beyond it has no memory record, and
    /// the window has to keep carrying it verbatim.
    fn end(self, covered: i64, accepted_through: i64) -> Option<i64> {
        let end = match self {
            Self::Keep => return None,
            Self::Through(through) => through.min(accepted_through),
            Self::Extracted => accepted_through,
        };
        Some(end).filter(|end| *end > covered)
    }
}

/// `messages` with every span the user forgot replaced by a marker.
///
/// Only the copies handed to the summariser change; the transcript and the ids
/// recorded on the commit do not.
fn redact_forgotten(
    messages: Vec<SourceMessage>,
    suppressed: &[SuppressedSpan],
) -> Vec<SourceMessage> {
    if suppressed.is_empty() {
        return messages;
    }
    let forgotten = SuppressionsByMessage::new(suppressed);
    messages
        .into_iter()
        .map(|mut message| {
            if let Cow::Owned(redacted) = forgotten.redact(&message.id, &message.content) {
                message.content = redacted;
            }
            message
        })
        .collect()
}

/// One page of `(summary boundary, fold end]` below the watermark: source the
/// ledger covers and the summary is to cover. It is folded, never
/// re-extracted.
struct Refold {
    /// Everything read, eligible or not. The last message is where the new
    /// summary ends, so the next refold starts after it.
    read: Vec<SourceMessage>,
    /// The read reached the end it was asked for, so fresh source can follow
    /// it into the same fold without leaving a gap.
    finished: bool,
    /// There is something to write: eligible source, or an earlier summary to
    /// carry forward to a later boundary. Without either the range holds only
    /// failed outputs, and a conversation in that state does not refold.
    foldable: bool,
}

impl Refold {
    fn none() -> Self {
        Self {
            read: Vec::new(),
            finished: true,
            foldable: false,
        }
    }

    /// The message the refolded summary ends on, when there is one to write.
    fn boundary(&self) -> Option<&SourceMessage> {
        if self.foldable {
            self.read.last()
        } else {
            None
        }
    }

    fn eligible(&self) -> Vec<SourceMessage> {
        self.read
            .iter()
            .filter(|message| message.is_eligible_source())
            .cloned()
            .collect()
    }
}

/// What one paged read returned.
struct RangeRead {
    messages: Vec<SourceMessage>,
    /// Sequence the read got through.
    cursor: i64,
    /// The work cap stopped the read, rather than the range running out.
    capped: bool,
}

// ---------------------------------------------------------------------------
// Candidate ledger
// ---------------------------------------------------------------------------

/// Keep the watermark before everything in `batch`.
///
/// Sequences are never reused, so one below the batch's first sequence covers
/// every message ahead of it and nothing inside it.
fn hold_before(batch: &[SourceSegment], start_after: i64, current: i64) -> i64 {
    batch
        .first()
        .map(|segment| segment.sequence.saturating_sub(1))
        .unwrap_or(current)
        .max(start_after)
        .min(current)
}

/// The in-memory ledger a run builds before it commits anything.
///
/// It exists because a run can extract in several batches and a later batch must
/// see what an earlier one decided. It also keeps the one bookkeeping rule the
/// repository cannot: a transition against an item this same run added has no
/// row to update yet, so it rewrites the pending insert instead of emitting an
/// `UPDATE` that would quietly match nothing.
struct CandidateLedger {
    active: Vec<MemoryItem>,
    inserts: Vec<MemoryItem>,
    updates: Vec<MemoryItem>,
}

impl CandidateLedger {
    fn new(active: Vec<MemoryItem>) -> Self {
        Self {
            active,
            inserts: Vec::new(),
            updates: Vec::new(),
        }
    }

    fn active(&self) -> &[MemoryItem] {
        &self.active
    }

    fn absorb(&mut self, commit: MemoryCommit) {
        for item in commit.inserts {
            self.active.push(item.clone());
            self.inserts.push(item);
        }
        for item in commit.updates {
            if let Some(pending) = self
                .inserts
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                *pending = item.clone();
            } else if let Some(previous) = self
                .updates
                .iter_mut()
                .find(|existing| existing.id == item.id)
            {
                *previous = item.clone();
            } else {
                self.updates.push(item.clone());
            }
            self.active.retain(|existing| existing.id != item.id);
            if item.state == MemoryState::Active {
                self.active.push(item);
            }
        }
    }

    /// Labels of the items that will reach every prompt. The summariser is shown
    /// these so it does not re-author them as its own authoritative list (§15).
    fn mandatory_labels(&self) -> Vec<String> {
        self.active
            .iter()
            .filter(|item| item.is_mandatory_now())
            .map(|item| item.label.clone())
            .collect()
    }

    fn into_changes(self) -> (Vec<MemoryItem>, Vec<MemoryItem>) {
        (self.inserts, self.updates)
    }
}
