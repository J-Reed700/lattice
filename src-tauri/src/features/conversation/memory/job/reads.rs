//! What an attempt reads: its source window, the refold range, the spans the
//! user asked to forget, and the evidence a review cites.
use tracing::warn;

use lattice_core::application::ports::conversation_memory::{SourceReadLimits, SourceSpanRef};
use lattice_core::domain::conversation::memory::{
    EvidenceSpan, MemoryItem, MemorySnapshot, SourceMessage, SuppressedSpan, ValidatedPatch,
    ValidatedTransitionTarget,
};

use super::super::verify::EvidenceText;
use super::super::CompactionError;
use super::{CompactionJob, RangeRead, Refold};

impl CompactionJob {
    /// Page the unprocessed range up to this attempt's work cap.
    ///
    /// Returns the messages read and whether they reach the end of the
    /// transcript. Stopping short is reported, never silently absorbed: the
    /// caller turns it into resumable progress (§7.5).
    pub(super) async fn read_window(
        &self,
        conversation_id: &str,
        start_after: i64,
        latest_sequence: i64,
    ) -> std::result::Result<(Vec<SourceMessage>, bool), CompactionError> {
        let read = self
            .read_range(
                conversation_id,
                start_after,
                latest_sequence,
                self.config.max_source_bytes_per_attempt,
            )
            .await?;
        Ok((read.messages, read.cursor >= latest_sequence))
    }

    /// Page the source a dropped summary covered, oldest first.
    ///
    /// Bounded by the work cap's message count and by the smaller refold byte
    /// budget. Progress needs no column of its own: the summary this run
    /// commits ends on the last message read, and the next run resumes there.
    ///
    /// The one read that could not record progress is a page with nothing to
    /// fold and no earlier summary to carry forward — an ineligible output
    /// larger than the whole budget. Retrying it would stall every later run,
    /// so the refold is given up for that range instead, with a warning.
    pub(super) async fn read_refold(
        &self,
        conversation_id: &str,
        refold_after: i64,
        start_after: i64,
        snapshot: &MemorySnapshot,
    ) -> std::result::Result<Refold, CompactionError> {
        let read = self
            .read_range(
                conversation_id,
                refold_after,
                start_after,
                self.config
                    .max_refold_bytes_per_attempt
                    .min(self.config.max_source_bytes_per_attempt),
            )
            .await?;
        let finished = !read.capped || read.cursor >= start_after;
        let foldable = snapshot.summary.is_some()
            || read.messages.iter().any(SourceMessage::is_eligible_source);
        if !finished && (read.messages.is_empty() || !foldable) {
            warn!(
                conversation_id,
                refold_after,
                start_after,
                "Summary refold cannot advance within one attempt; leaving that range to the ledger and recall"
            );
            return Ok(Refold::none());
        }
        Ok(Refold {
            read: read.messages,
            finished,
            foldable,
        })
    }

    /// The forgotten spans in `messages`, in one read, or none without one.
    pub(super) async fn suppressed_spans<'a>(
        &self,
        conversation_id: &str,
        messages: impl Iterator<Item = &'a SourceMessage>,
    ) -> std::result::Result<Vec<SuppressedSpan>, CompactionError> {
        let ids: Vec<String> = messages.map(|message| message.id.clone()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.memory.suppressed_spans(conversation_id, &ids).await?)
    }

    /// Page `(after_sequence, through_sequence]` oldest first until the range
    /// ends or the work cap binds.
    async fn read_range(
        &self,
        conversation_id: &str,
        after_sequence: i64,
        through_sequence: i64,
        max_bytes: usize,
    ) -> std::result::Result<RangeRead, CompactionError> {
        let mut messages: Vec<SourceMessage> = Vec::new();
        let mut bytes = 0usize;
        let mut cursor = after_sequence;
        let mut capped = false;

        while cursor < through_sequence {
            let message_room = self
                .config
                .max_source_messages_per_attempt
                .saturating_sub(messages.len());
            let byte_room = max_bytes.saturating_sub(bytes);
            if message_room == 0 || byte_room == 0 {
                capped = true;
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
                .page_source_messages(conversation_id, cursor, through_sequence, limits)
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
            messages.extend(page.messages);
            if !page.has_more {
                break;
            }
        }
        Ok(RangeRead {
            messages,
            cursor,
            capped,
        })
    }

    /// Exact text for every span a review will read.
    ///
    /// Spans inside this attempt's window resolve locally. An existing item's
    /// evidence usually does not — it quotes a message from before the watermark
    /// — so those go through the port, which resolves them against live content
    /// and returns absence rather than a cached quotation.
    pub(super) async fn resolve_review_evidence(
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
