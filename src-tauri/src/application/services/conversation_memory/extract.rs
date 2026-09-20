//! The extraction round trip: prompt, parse, validate, bounded repairs.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §6.2, §7.3.
//!
//! Everything the model returns is untrusted. This module does not decide
//! whether a quote is real, whether a source belongs to this conversation, or
//! whether a transition is ordered correctly — `domain::conversation_memory`
//! does, and it accepts or rejects a patch as a unit. What lives here is the
//! plumbing around that: what the model is shown, how a rejection is reported
//! back to it, and the hard limit on bounded repair attempts.

use tracing::{debug, warn};

use crate::application::ports::LLMPort;
use crate::domain::conversation_memory::{
    parse_patch, validate_patch, MemoryItem, SourceIndex, SourceMessage, ValidatedPatch,
};

use super::prompts::{
    patch_schema, render_extractor_prompt, render_repair_prompt, ContextPassage, EXTRACTOR_SYSTEM,
};
use super::segment::SourceSegment;
use super::{complete_json, CompactionError, Deadline, MAX_EXTRACTION_ATTEMPTS};

/// One accepted patch, with what it cost to get it.
#[derive(Debug, Clone)]
pub struct ExtractionOutcome {
    pub patch: ValidatedPatch,
    /// Model calls spent, including the repair.
    pub attempts: usize,
    /// True when the first response was rejected and the repair was accepted.
    pub repaired: bool,
}

/// Extract memory from one batch of segments.
///
/// `quotable` is every message the batch is allowed to cite: the segments' own
/// messages plus the bounded adjacent context. A quote naming anything else is
/// rejected, which is why the repair prompt restates the list.
///
/// # Errors
///
/// * [`CompactionError::Rejected`] when the repair attempt was also invalid.
///   The caller must not advance the watermark past these segments (§6.2).
/// * [`CompactionError::DeadlineExceeded`] / [`CompactionError::Cancelled`]
///   when the job's shared deadline ran out or the caller cancelled.
/// * [`CompactionError::Model`] when the utility model itself failed.
pub async fn extract_segments(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    segments: &[SourceSegment],
    quotable: &[SourceMessage],
    context: &[ContextPassage<'_>],
    existing: &[MemoryItem],
) -> std::result::Result<ExtractionOutcome, CompactionError> {
    let manifest: Vec<String> = segments.iter().map(|segment| segment.id.clone()).collect();
    let allowed: Vec<&str> = quotable.iter().map(|message| message.id.as_str()).collect();
    let index = SourceIndex::new(quotable);

    let original_request = render_extractor_prompt(segments, context, existing);
    let mut request = original_request.clone();
    let mut attempts = 0usize;

    loop {
        attempts += 1;
        let raw = complete_json(
            llm,
            deadline,
            EXTRACTOR_SYSTEM,
            request,
            Some(patch_schema()),
        )
        .await?;

        let validated =
            parse_patch(&raw).and_then(|patch| validate_patch(&patch, &index, existing, &manifest));

        match validated {
            Ok(patch) => {
                debug!(
                    segments = segments.len(),
                    additions = patch.additions.len(),
                    transitions = patch.transitions.len(),
                    attempts,
                    "Memory extraction accepted"
                );
                return Ok(ExtractionOutcome {
                    patch,
                    attempts,
                    repaired: attempts > 1,
                });
            }
            Err(error) if attempts < MAX_EXTRACTION_ATTEMPTS => {
                // The error text goes back to the model; its *code* is what gets
                // logged and persisted, because the text can name a source id.
                warn!(
                    code = error.code(),
                    segments = segments.len(),
                    "Memory extraction response rejected — requesting a bounded repair"
                );
                request = render_repair_prompt(
                    &error.to_string(),
                    error.code(),
                    &allowed,
                    &manifest,
                    &original_request,
                    &raw,
                );
            }
            Err(error) => {
                warn!(
                    code = error.code(),
                    segments = segments.len(),
                    "Memory extraction repair also rejected — these segments stay unprocessed"
                );
                return Err(CompactionError::Rejected(error));
            }
        }
    }
}

/// Segment ids in a batch, for reporting which source a rejection left behind.
pub fn segment_ids(segments: &[SourceSegment]) -> Vec<String> {
    segments.iter().map(|segment| segment.id.clone()).collect()
}
