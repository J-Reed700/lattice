//! One compaction run, from snapshot to commit.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §7.
//!
//! Split out of the module façade because the run is the long part: boundary
//! selection, the batch loop, the candidate ledger a run builds before it is
//! allowed to write anything, and the single commit that publishes it.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex as SyncMutex;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};

use crate::application::ports::conversation_memory::{
    ConversationMemoryPort, MemoryCommitCandidate, MemoryCommitPreconditions, SourceReadLimits,
    SourceSpanRef, SummaryUpdate,
};
use crate::application::ports::LLMPort;
use crate::domain::conversation_memory::{
    apply_patch, EvidenceSpan, MemoryCommit, MemoryItem, MemoryKind, MemoryState,
    MemoryValidationError, SemanticVerdict, SourceMessage, ValidatedPatch,
    ValidatedTransitionTarget,
};
use crate::shared::error::Result;

use super::prompts::{EXTRACTOR_PROMPT_VERSION, VALIDATOR_VERSION};
use super::segment::SourceSegment;
use super::selection::{adjacent_context, quotable_messages, render_context, select_boundary};
use super::verify::EvidenceText;
use super::{
    extract, segment, summarize, verify, CompactionConfig, CompactionError, CompactionOutcome,
    CompactionRequest, CompactionStatus, CompactionTrigger, Deadline, TokenCounter,
    MAX_COMMIT_CONFLICT_RETRIES,
};

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
                        Deadline::new(self.config.deadline, request.cancellation.clone());
                    self.run_with_retry(conversation_id, &request, &operation_id, &deadline)
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

    /// One whole attempt, from snapshot to commit.
    async fn attempt(
        &self,
        conversation_id: &str,
        request: &CompactionRequest,
        operation_id: &str,
        deadline: &Deadline,
        commit_conflict_retries: usize,
    ) -> std::result::Result<CompactionOutcome, CompactionError> {
        deadline.check()?;
        let snapshot = self.memory.load_snapshot(conversation_id).await?;

        // An unsupported schema or an invalidating edit means the stored ledger
        // cannot be trusted, so this run rebuilds from sequence zero rather than
        // appending to state it cannot read (invariant 14).
        let rebuild = !snapshot.is_usable();
        let start_after = if rebuild {
            0
        } else {
            snapshot.state.processed_through_sequence
        };
        let operation = match (rebuild, request.trigger) {
            (true, _) => "rebuild",
            (false, CompactionTrigger::Automatic) => "auto_compact",
            (false, CompactionTrigger::Manual) => "compact",
        };
        if snapshot.latest_sequence <= start_after {
            return Ok(CompactionOutcome::nothing(&snapshot));
        }

        let (window, reached_end) = self
            .read_window(conversation_id, start_after, snapshot.latest_sequence)
            .await?;
        let keep_recent = request
            .keep_recent_messages
            .unwrap_or(self.config.keep_recent_messages)
            .max(1);
        let Some(prefix_len) = select_boundary(&window, reached_end, keep_recent) else {
            return Ok(CompactionOutcome::nothing(&snapshot));
        };
        let prefix = window.get(..prefix_len).unwrap_or(&window);
        let Some(boundary) = prefix.last() else {
            return Ok(CompactionOutcome::nothing(&snapshot));
        };
        let boundary_sequence = boundary.sequence;

        // A failed assistant output is not shown at all. It cannot be evidence
        // (§6.2 rule 8), and offering it as context invites a quote that the
        // validator would then reject, taking the whole batch with it. The
        // watermark still covers it: it was considered, not skipped.
        let eligible: Vec<SourceMessage> = prefix
            .iter()
            .filter(|message| message.is_eligible_source())
            .cloned()
            .collect();
        let segments = segment::segment_messages(
            &eligible,
            self.config.segment_max_bytes,
            self.config.segment_overlap_chars,
        );
        let segments_submitted = segments.len();
        let batches = segment::plan_batches(
            segments,
            self.config.max_segments_per_batch,
            self.config.max_batch_bytes,
        );

        let mut ledger = CandidateLedger::new(snapshot.active_items.clone());
        let next_revision = snapshot.state.memory_revision + 1;
        let mut accepted_through = boundary_sequence;
        let mut segments_rejected: Vec<String> = Vec::new();
        let mut rejection: Option<MemoryValidationError> = None;
        let mut halt: Option<&'static str> = None;
        let mut extraction_calls = 0usize;
        let mut repaired_batches = 0usize;
        let mut review_calls = 0usize;
        let mut review_degraded = false;
        let mut conflicts_recorded = 0usize;

        for batch in &batches {
            let context = adjacent_context(&eligible, batch);
            let quotable = quotable_messages(&eligible, batch, &context);
            let rendered = render_context(&context);

            let outcome = extract::extract_segments(
                self.utility_llm.as_ref(),
                deadline,
                batch,
                &quotable,
                &rendered,
                ledger.active(),
            )
            .await;
            let outcome = match outcome {
                Ok(outcome) => outcome,
                Err(CompactionError::Rejected(error)) => {
                    // Everything from this batch onward stays unprocessed, so
                    // the same source is offered again next time (§6.2).
                    segments_rejected.extend(extract::segment_ids(batch));
                    accepted_through = hold_before(batch, start_after, accepted_through);
                    rejection = Some(error);
                    break;
                }
                Err(other) => return Err(other),
            };
            extraction_calls += outcome.attempts;
            if outcome.repaired {
                repaired_batches += 1;
            }

            let evidence = self
                .resolve_review_evidence(
                    conversation_id,
                    &outcome.patch,
                    &eligible,
                    ledger.active(),
                )
                .await?;
            let review = verify::review_patch(
                self.utility_llm.as_ref(),
                deadline,
                &outcome.patch,
                ledger.active(),
                &eligible,
                &evidence,
            )
            .await?;
            review_calls += review.calls;
            review_degraded |= review.degraded;

            // §6.3: confidently unsupported operations are filtered by
            // `apply_patch`. They must not poison supported operations from the
            // same batch or stall the watermark forever when an extractor keeps
            // proposing the same assistant-derived detail. The source is still
            // covered by the working summary. An *ambiguous* optional addition
            // remains different: dropping it after a reviewer outage or genuine
            // uncertainty could silently lose user memory, so that case retries.
            if outcome.patch.additions.iter().any(|addition| {
                !addition.kind.is_mandatory()
                    && !matches!(
                        review.addition_verdicts.get(&addition.candidate_id),
                        Some(SemanticVerdict::Supported | SemanticVerdict::Unsupported)
                    )
            }) {
                segments_rejected.extend(extract::segment_ids(batch));
                accepted_through = hold_before(batch, start_after, accepted_through);
                halt = Some("ambiguous_optional_addition");
                break;
            }
            let commit = apply_patch(
                conversation_id,
                &outcome.patch,
                ledger.active(),
                &review.addition_verdicts,
                &review.transition_verdicts,
                next_revision,
                accepted_through,
            );
            conflicts_recorded += commit
                .inserts
                .iter()
                .filter(|item| item.kind == MemoryKind::UnresolvedChange)
                .count();
            ledger.absorb(commit);
        }

        // Nothing was accepted: there is no watermark to advance and no summary
        // worth writing, so this attempt commits nothing at all.
        if accepted_through <= start_after {
            if let Some(error) = rejection {
                return Err(CompactionError::Rejected(error));
            }
            if let Some(code) = halt {
                return Err(CompactionError::ReviewUnresolved(code));
            }
            return Ok(CompactionOutcome::nothing(&snapshot));
        }

        // Only eligible text is folded: a failed assistant output is not part of
        // the conversation's progress. The *display* boundary below still points
        // at the last message of the compacted prefix, eligible or not, because
        // that is where the divider belongs.
        let folded: Vec<SourceMessage> = eligible
            .iter()
            .filter(|message| message.sequence <= accepted_through)
            .cloned()
            .collect();
        let display_boundary = prefix
            .iter()
            .rev()
            .find(|message| message.sequence <= accepted_through);
        // A rebuild must not fold an earlier summary back in: that summary was
        // written against a boundary this run is discarding (§7.2).
        let previous_summary = if rebuild {
            None
        } else {
            snapshot.summary.as_deref()
        };
        let summary = summarize::fold_summary(
            self.utility_llm.as_ref(),
            deadline,
            previous_summary,
            &folded,
            &ledger.mandatory_labels(),
            self.config.summary_token_budget,
            self.count_tokens.as_ref(),
        )
        .await?;

        let summary_text = Some(summary.text.clone()).filter(|text| !text.is_empty());
        let (inserts, updates) = ledger.into_changes();
        let commit = MemoryCommit {
            inserts,
            updates,
            summary: summary_text.clone(),
            processed_through_sequence: accepted_through,
        };
        let summary_update = match (summary_text, display_boundary) {
            (Some(text), Some(last)) => Some(SummaryUpdate {
                summary_text: text,
                up_to_message_id: last.id.clone(),
                original_message_count: folded.len() as i64,
                original_tokens: folded
                    .iter()
                    .map(|message| (self.count_tokens)(&message.content) as i64)
                    .sum(),
                summary_tokens: summary.tokens as i64,
            }),
            _ => None,
        };

        let candidate = MemoryCommitCandidate {
            commit: commit.clone(),
            summary: summary_update,
            source_message_ids: folded.iter().map(|message| message.id.clone()).collect(),
            transcript_revision: snapshot.transcript_revision,
            extractor_model_identity: Some(self.utility_llm.model_name().to_string()),
            extractor_prompt_version: Some(EXTRACTOR_PROMPT_VERSION.to_string()),
            validator_version: Some(VALIDATOR_VERSION.to_string()),
            operation: operation.to_string(),
        };
        let preconditions = MemoryCommitPreconditions {
            conversation_id: conversation_id.to_string(),
            expected_transcript_revision: snapshot.transcript_revision,
            expected_memory_revision: snapshot.state.memory_revision,
            operation_id: operation_id.to_string(),
        };
        let committed = self
            .memory
            .commit_memory(&preconditions, &candidate)
            .await
            .map_err(CompactionError::Commit)?;

        // Retaining the recent tail is the design, not unfinished work. This run
        // is incomplete only when the work cap stopped the read short of the
        // transcript, or a batch was rejected and the watermark stayed behind it.
        let incomplete = !reached_end || rejection.is_some() || halt.is_some();
        info!(
            conversation_id,
            operation,
            memory_revision = committed.memory_revision,
            processed_through = committed.processed_through_sequence,
            mandatory = committed.active_mandatory_count,
            conflicts = conflicts_recorded,
            rejected_segments = segments_rejected.len(),
            incomplete,
            "Conversation memory committed"
        );

        Ok(CompactionOutcome {
            status: if incomplete {
                CompactionStatus::Partial
            } else {
                CompactionStatus::Committed
            },
            memory_revision: committed.memory_revision,
            transcript_revision: committed.transcript_revision,
            processed_through_sequence: committed.processed_through_sequence,
            boundary_message_id: display_boundary.map(|message| message.id.clone()),
            active_mandatory_count: committed.active_mandatory_count,
            active_optional_count: committed.active_optional_count,
            summary: commit.summary.clone(),
            summary_tokens: summary.tokens,
            summary_regenerated: summary.regenerated,
            commit,
            source_messages_read: window.len(),
            segments_submitted,
            segments_rejected,
            rejection_code: rejection
                .map(|error| error.code().to_string())
                .or_else(|| halt.map(str::to_string)),
            extraction_calls,
            repaired_batches,
            review_calls,
            review_degraded,
            conflicts_recorded,
            commit_conflict_retries,
            resume_after_sequence: incomplete.then_some(accepted_through),
            was_already_committed: committed.was_already_committed,
        })
    }

    /// Page the unprocessed range up to this attempt's work cap.
    ///
    /// Returns the messages read and whether they reach the end of the
    /// transcript. Stopping short is reported, never silently absorbed: the
    /// caller turns it into resumable progress (§7.5).
    async fn read_window(
        &self,
        conversation_id: &str,
        start_after: i64,
        latest_sequence: i64,
    ) -> std::result::Result<(Vec<SourceMessage>, bool), CompactionError> {
        let mut window: Vec<SourceMessage> = Vec::new();
        let mut bytes = 0usize;
        let mut cursor = start_after;

        while cursor < latest_sequence {
            let message_room = self
                .config
                .max_source_messages_per_attempt
                .saturating_sub(window.len());
            let byte_room = self
                .config
                .max_source_bytes_per_attempt
                .saturating_sub(bytes);
            if message_room == 0 || byte_room == 0 {
                break;
            }
            let limits = SourceReadLimits::new(
                self.config
                    .source_read_limits
                    .max_messages
                    .min(message_room),
                self.config.source_read_limits.max_bytes.min(byte_room),
            );
            let page = self
                .memory
                .page_source_messages(conversation_id, cursor, latest_sequence, limits)
                .await?;
            if page.messages.is_empty() || page.next_after_sequence <= cursor {
                break;
            }
            bytes = bytes.saturating_add(
                page.messages
                    .iter()
                    .map(|message| message.content.len())
                    .sum(),
            );
            cursor = page.next_after_sequence;
            window.extend(page.messages);
            if !page.has_more {
                break;
            }
        }
        Ok((window, cursor >= latest_sequence))
    }

    /// Exact text for every span a review will read.
    ///
    /// Spans inside this attempt's window resolve locally. An existing item's
    /// evidence usually does not — it quotes a message from before the watermark
    /// — so those go through the port, which resolves them against live content
    /// and returns absence rather than a cached quotation.
    async fn resolve_review_evidence(
        &self,
        conversation_id: &str,
        patch: &ValidatedPatch,
        window: &[SourceMessage],
        active_items: &[MemoryItem],
    ) -> std::result::Result<EvidenceText, CompactionError> {
        let mut evidence = EvidenceText::new();
        evidence.learn_from(
            window,
            patch
                .additions
                .iter()
                .flat_map(|addition| addition.evidence.iter())
                .chain(
                    patch
                        .transitions
                        .iter()
                        .flat_map(|transition| transition.evidence.iter()),
                ),
        );

        let targeted: Vec<&MemoryItem> = patch
            .transitions
            .iter()
            .filter_map(|transition| {
                let ValidatedTransitionTarget::Existing(id) = &transition.target else {
                    return None;
                };
                active_items.iter().find(|item| item.id == *id)
            })
            .collect();
        evidence.learn_from(
            window,
            targeted.iter().flat_map(|item| item.evidence.iter()),
        );

        let outstanding: Vec<&EvidenceSpan> = targeted
            .iter()
            .flat_map(|item| item.evidence.iter())
            .filter(|span| evidence.text(span).is_none())
            .collect();
        if outstanding.is_empty() {
            return Ok(evidence);
        }
        let refs: Vec<SourceSpanRef> = outstanding
            .iter()
            .map(|span| SourceSpanRef {
                message_id: span.message_id.clone(),
                start_byte: span.start_byte,
                end_byte: span.end_byte,
            })
            .collect();
        let resolved = self
            .memory
            .read_source_spans(conversation_id, &refs, self.config.source_read_limits)
            .await?;
        // The port answers one resolved span per request, in request order.
        for (span, resolution) in outstanding.iter().zip(resolved) {
            if let Some(text) = resolution.text {
                evidence.insert(span, text);
            }
        }
        Ok(evidence)
    }
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
