//! The body of one compaction attempt.
use tracing::info;

use lattice_core::application::ports::conversation_memory::{
    MemoryCommitCandidate, MemoryCommitPreconditions, SummaryUpdate,
};
use lattice_core::domain::conversation::memory::{
    apply_patch, drop_forgotten_additions, MemoryCommit, MemoryKind, MemoryValidationError,
    SemanticVerdict, SourceMessage,
};

use super::super::prompts::{EXTRACTOR_PROMPT_VERSION, VALIDATOR_VERSION};
use super::super::selection::{
    adjacent_context, context_room_chars, quotable_messages, render_context, select_boundary,
};
use super::super::{
    extract, segment, summarize, verify, CompactionError, CompactionOutcome, CompactionRequest,
    CompactionStatus, CompactionTrigger, Deadline,
};
use super::{
    hold_before, redact_forgotten, summary_resume_point, CandidateLedger, CompactionJob,
    FoldTarget, PhaseTiming, Refold,
};

impl CompactionJob {
    /// One whole attempt, from snapshot to commit.
    pub(super) async fn attempt(
        &self,
        conversation_id: &str,
        request: &CompactionRequest,
        operation_id: &str,
        deadline: &Deadline,
        commit_conflict_retries: usize,
    ) -> std::result::Result<CompactionOutcome, CompactionError> {
        deadline.check()?;
        let snapshot_phase = PhaseTiming::start(conversation_id, "load_snapshot");
        let snapshot = self.memory.load_snapshot(conversation_id).await;
        snapshot_phase.finish(if snapshot.is_ok() {
            "completed"
        } else {
            "error"
        });
        let snapshot = snapshot?;

        // An unsupported schema or an invalidating edit means the stored ledger
        // cannot be trusted, so this run rebuilds from sequence zero rather than
        // appending to state it cannot read (invariant 14).
        let rebuild = !snapshot.is_usable();
        let start_after = if rebuild {
            0
        } else {
            snapshot.state.processed_through_sequence
        };
        // Where the working summary ends, and what this run does with it. The
        // summary usually trails the watermark: extraction runs after every
        // answer, the summary moves only when a fold is asked for. A rebuild
        // starts both from nothing.
        let refold_after = if rebuild {
            start_after
        } else {
            summary_resume_point(&snapshot, start_after)
        };
        let target = FoldTarget::for_run(request, refold_after);
        let refold_through = target.below_watermark(refold_after, start_after);
        let operation = match (rebuild, request.trigger) {
            (true, _) => "rebuild",
            (false, CompactionTrigger::Automatic) => "auto_compact",
            (false, CompactionTrigger::Manual) => "compact",
            (false, CompactionTrigger::Maintenance) => "consolidate",
        };
        if snapshot.latest_sequence <= start_after && refold_through.is_none() {
            return Ok(CompactionOutcome::nothing(&snapshot));
        }

        let refold = match refold_through {
            Some(through) => {
                let refold_phase = PhaseTiming::start(conversation_id, "read_refold_window");
                let read = self
                    .read_refold(conversation_id, refold_after, through, &snapshot)
                    .await;
                refold_phase.finish(if read.is_ok() { "completed" } else { "error" });
                read?
            }
            None => Refold::none(),
        };

        let (window, reached_end) = if snapshot.latest_sequence > start_after {
            let source_phase = PhaseTiming::start(conversation_id, "read_source_window");
            let source_window = self
                .read_window(conversation_id, start_after, snapshot.latest_sequence)
                .await;
            source_phase.finish(if source_window.is_ok() {
                "completed"
            } else {
                "error"
            });
            source_window?
        } else {
            (Vec::new(), true)
        };
        let keep_recent = request
            .keep_recent_messages
            .unwrap_or(self.config.keep_recent_messages)
            .max(1);
        let boundary = if request.trigger == CompactionTrigger::Maintenance {
            // Only completed turns: never extract a pending user turn while its
            // answer is being generated. Recent messages remain available raw.
            window
                .iter()
                .rposition(|m| {
                    m.role == lattice_core::domain::conversation::memory::SourceRole::Assistant
                        && m.status == "completed"
                })
                .map(|index| index + 1)
        } else {
            select_boundary(&window, reached_end, keep_recent)
        };
        let prefix: &[SourceMessage] = match boundary {
            Some(prefix_len) => window.get(..prefix_len).unwrap_or(&window),
            None => &[],
        };
        let refold_boundary = refold.boundary();
        let Some(boundary_sequence) = prefix
            .last()
            .map(|message| message.sequence)
            .or(refold_boundary.map(|_| start_after))
        else {
            return Ok(CompactionOutcome::nothing(&snapshot));
        };

        // A failed assistant output is not shown at all. It cannot be evidence
        // (§6.2 rule 8), and offering it as context invites a quote that the
        // validator would then reject, taking the whole batch with it. The
        // watermark still covers it: it was considered, not skipped.
        let eligible: Vec<SourceMessage> = prefix
            .iter()
            .filter(|message| message.is_eligible_source())
            .cloned()
            .collect();
        // The user's forgets, read once for everything this attempt may extract
        // from or fold: a rebuild extracts from the original messages again,
        // and every fold is made from them.
        let suppressions_phase = PhaseTiming::start(conversation_id, "read_suppressions");
        let suppressed = self
            .suppressed_spans(conversation_id, eligible.iter().chain(&refold.read))
            .await;
        suppressions_phase.finish(if suppressed.is_ok() {
            "completed"
        } else {
            "error"
        });
        let suppressed = suppressed?;
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
            let rendered = render_context(
                &context,
                context_room_chars(self.utility_llm.as_ref(), batch, context.len()),
            );

            let extraction_phase = PhaseTiming::start(conversation_id, "extract");
            let outcome = extract::extract_segments(
                self.utility_llm.as_ref(),
                deadline,
                batch,
                &quotable,
                &rendered,
                ledger.active(),
            )
            .await;
            extraction_phase.finish(if outcome.is_ok() {
                "completed"
            } else {
                "error"
            });
            let mut outcome = match outcome {
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
            let forgotten = drop_forgotten_additions(&mut outcome.patch, &suppressed);
            if forgotten > 0 {
                info!(
                    conversation_id,
                    forgotten, "Dropped extracted memory that quotes forgotten text"
                );
            }

            let evidence_phase = PhaseTiming::start(conversation_id, "resolve_review_evidence");
            let evidence = self
                .resolve_review_evidence(
                    conversation_id,
                    &outcome.patch,
                    &eligible,
                    ledger.active(),
                )
                .await;
            evidence_phase.finish(if evidence.is_ok() {
                "completed"
            } else {
                "error"
            });
            let evidence = evidence?;
            let review_phase = PhaseTiming::start(conversation_id, "review");
            let review = verify::review_patch(
                self.utility_llm.as_ref(),
                deadline,
                &outcome.patch,
                ledger.active(),
                &eligible,
                &evidence,
            )
            .await;
            review_phase.finish(if review.is_ok() { "completed" } else { "error" });
            let review = review?;
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

        // Nothing was accepted: there is no watermark to advance, and unless
        // source below it is being folded there is no summary worth writing,
        // so this attempt commits nothing at all. A refold does not wait on
        // extraction: it reads only source the ledger already covers.
        let refold_only = accepted_through <= start_after;
        if refold_only && refold_boundary.is_none() {
            if let Some(error) = rejection {
                return Err(CompactionError::Rejected(error));
            }
            if let Some(code) = halt {
                return Err(CompactionError::ReviewUnresolved(code));
            }
            return Ok(CompactionOutcome::nothing(&snapshot));
        }
        let operation = if refold_only {
            "refold_summary"
        } else {
            operation
        };

        // What this run processed, recorded on the commit's event.
        let fresh: Vec<SourceMessage> = eligible
            .iter()
            .filter(|message| message.sequence <= accepted_through)
            .cloned()
            .collect();
        let refolded = refold.eligible();
        // Where the summary ends after this run, or `None` to leave it.
        let fold_end = target.end(refold_after, accepted_through);
        // Only eligible text is folded: a failed assistant output is not part
        // of the conversation's progress. Refolded source goes first, in
        // conversation order. Fresh source joins it only once the refold has
        // reached its end: the summary's boundary is where the next fold
        // resumes, so nothing may sit unfolded behind it. Fresh source
        // extracted meanwhile is folded later, as part of the range the next
        // fold covers.
        let folded: Vec<SourceMessage> = match fold_end {
            None => Vec::new(),
            Some(end) if refold.finished => refolded
                .iter()
                .chain(fresh.iter().filter(|message| message.sequence <= end))
                .cloned()
                .collect(),
            Some(_) => refolded.clone(),
        };
        // The one place source text enters the summariser, for a normal fold,
        // a refold and a rebuild alike, so this is where a user's forget is
        // enforced: the summary is rebuilt from the originals, and the
        // forgotten value must not come back with it. Forgotten spans are cut
        // out; the forgotten facts are named, so a restatement elsewhere is
        // left out as well.
        let folded = redact_forgotten(folded, &suppressed);
        let forgotten_labels = if folded.is_empty() {
            Vec::new()
        } else {
            let labels_phase = PhaseTiming::start(conversation_id, "read_forgotten_labels");
            let labels = self.memory.forgotten_labels(conversation_id).await;
            labels_phase.finish(if labels.is_ok() { "completed" } else { "error" });
            labels?
        };
        // The *display* boundary points at the last message folded over,
        // eligible or not, because that is where the summary ends and where
        // the divider belongs.
        let display_boundary = match fold_end {
            None => None,
            Some(end) if refold.finished => prefix
                .iter()
                .rev()
                .find(|message| message.sequence <= end)
                .or(refold_boundary),
            Some(_) => refold_boundary,
        };
        // A rebuild must not fold an earlier summary back in: that summary was
        // written against a boundary this run is discarding (§7.2).
        let previous_summary = if rebuild {
            None
        } else {
            snapshot.summary.as_deref()
        };
        let summary = match display_boundary {
            Some(_) => {
                let summary_phase = PhaseTiming::start(conversation_id, "summary");
                let summary = summarize::fold_summary(
                    self.utility_llm.as_ref(),
                    deadline,
                    previous_summary,
                    &folded,
                    &ledger.mandatory_labels(),
                    &forgotten_labels,
                    self.config.summary_token_budget,
                    self.count_tokens.as_ref(),
                )
                .await;
                summary_phase.finish(if summary.is_ok() {
                    "completed"
                } else {
                    "error"
                });
                match summary {
                    Ok(summary) => Some(summary),
                    // What extraction accepted does not depend on the
                    // summary: a fold that does not fit or comes back
                    // empty would do so every time, so the accepted batches
                    // commit and the summary stays as it was.
                    Err(
                        error @ (CompactionError::SummaryTooLarge { .. }
                        | CompactionError::SummaryEmpty),
                    ) if !refold_only => {
                        tracing::warn!(
                            conversation_id,
                            %error,
                            "The working summary could not be folded; committing the extraction with the summary unchanged"
                        );
                        None
                    }
                    Err(error) => return Err(error),
                }
            }
            None => None,
        };

        let summary_text = summary
            .as_ref()
            .map(|summary| summary.text.clone())
            .filter(|text| !text.is_empty());
        let (inserts, updates) = ledger.into_changes();
        let commit = MemoryCommit {
            inserts,
            updates,
            summary: summary_text.clone(),
            processed_through_sequence: accepted_through,
        };
        let summary_update = match (summary_text, display_boundary, &summary) {
            (Some(text), Some(last), Some(summary)) => Some(SummaryUpdate {
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
            source_message_ids: refolded
                .iter()
                .chain(&fresh)
                .map(|message| message.id.clone())
                .collect(),
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
        let commit_phase = PhaseTiming::start(conversation_id, "commit");
        let committed = self.memory.commit_memory(&preconditions, &candidate).await;
        commit_phase.finish(if committed.is_ok() {
            "completed"
        } else {
            "error"
        });
        let committed = committed.map_err(CompactionError::Commit)?;

        // Retaining the recent tail is the design, not unfinished work. This run
        // is incomplete only when the work cap stopped the read short of the
        // transcript, a batch was rejected and the watermark stayed behind it,
        // or a fold still has source below the watermark left to read.
        let incomplete = !reached_end || rejection.is_some() || halt.is_some() || !refold.finished;
        info!(
            conversation_id,
            operation,
            memory_revision = committed.memory_revision,
            processed_through = committed.processed_through_sequence,
            mandatory = committed.active_mandatory_count,
            conflicts = conflicts_recorded,
            rejected_segments = segments_rejected.len(),
            refolded = refolded.len(),
            refold_finished = refold.finished,
            summary_through = ?display_boundary.map(|message| message.sequence),
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
            // The summary in force after this run: the refolded one, or the
            // one it left as it was.
            summary: commit
                .summary
                .clone()
                .or_else(|| previous_summary.map(str::to_owned)),
            summary_tokens: summary.as_ref().map_or(0, |summary| summary.tokens),
            summary_regenerated: summary.as_ref().is_some_and(|summary| summary.regenerated),
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
            refold_pending: !refold.finished,
            was_already_committed: committed.was_already_committed,
        })
    }
}
