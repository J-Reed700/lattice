//! The working-summary fold and its budget gate (§7.3 steps 8-9, §15).
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §7.3, §15.
//!
//! The summary is generated, fallible assistant context. It is capped
//! independently of the ledger, and it is measured against the **continuation**
//! model's pool rather than the utility model's window: the utility model could
//! happily return four thousand tokens of prose that will not fit the prompt it
//! is meant to travel in. The caller injects the token counter for that reason
//! — this module must not invent its own accounting.
//!
//! An oversized or empty candidate gets exactly one bounded regeneration and is
//! then rejected. Persisting it and hoping the next turn shrinks it would leave
//! a prompt that cannot be built at all.

use tracing::{debug, warn};

use crate::application::ports::LLMPort;
use crate::domain::conversation_memory::SourceMessage;

use super::prompts::{
    render_summary_prompt, render_summary_shrink_prompt, SummaryPassage, SUMMARIZER_SYSTEM,
};
use super::segment::clip_chars;
use super::{complete_json, CompactionError, Deadline};

/// Source bytes folded into one summariser call. Beyond this the fold is paged,
/// carrying the running summary forward (§7.3 step 8).
pub const MAX_FOLD_BYTES: usize = 16 * 1024;

/// Characters of one message shown to the summariser. The summary is prose about
/// the work, not a second copy of the transcript; the transcript stays readable
/// through recall.
pub const MAX_PASSAGE_CHARS: usize = 4_000;

/// Regenerations allowed when the candidate does not fit its pool (§7.3 step 9).
pub const MAX_SUMMARY_REGENERATIONS: usize = 1;

/// A summary that fits the pool it was measured against.
#[derive(Debug, Clone)]
pub struct FittedSummary {
    pub text: String,
    /// Measured with the continuation model's counter, not the utility model's.
    pub tokens: usize,
    /// True when the first candidate overran and the shrink request was used.
    pub regenerated: bool,
    pub calls: usize,
}

/// Fold the previous summary and the newly processed passages into one summary
/// that fits `budget_tokens`.
///
/// # Errors
///
/// * [`CompactionError::SummaryTooLarge`] when the candidate still overruns
///   after its one regeneration. Nothing is committed: an oversized summary is
///   not a better outcome than an older accurate one.
/// * [`CompactionError::SummaryEmpty`] when the summariser returned nothing
///   twice. A blank summary would read as "nothing happened".
/// * [`CompactionError::DeadlineExceeded`] / [`CompactionError::Cancelled`] /
///   [`CompactionError::Model`] from the underlying calls.
pub async fn fold_summary(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    previous_summary: Option<&str>,
    passages: &[SourceMessage],
    mandatory_labels: &[String],
    budget_tokens: usize,
    count_tokens: &(dyn Fn(&str) -> usize + Send + Sync),
) -> std::result::Result<FittedSummary, CompactionError> {
    let mut current = previous_summary.map(str::to_string);
    let mut calls = 0usize;

    // Nothing new to fold. This happens when a compacted prefix held only
    // ineligible material; spending a call to rewrite an unchanged summary would
    // only risk losing something from it.
    let steps = if passages.is_empty() {
        Vec::new()
    } else {
        plan_fold(passages)
    };

    for (index, step) in steps.iter().enumerate() {
        let rendered: Vec<SummaryPassage<'_>> = step
            .iter()
            .map(|message| SummaryPassage {
                sequence: message.sequence,
                role: message.role.as_str(),
                text: clip_chars(&message.content, MAX_PASSAGE_CHARS).0,
            })
            .collect();
        let prompt = render_summary_prompt(
            current.as_deref(),
            &rendered,
            mandatory_labels,
            budget_tokens,
            index + 1 == steps.len(),
        );
        calls += 1;
        let text = complete_json(llm, deadline, SUMMARIZER_SYSTEM, prompt, None).await?;
        let text = text.trim().to_string();
        if text.is_empty() {
            // Keep what the previous step produced rather than folding forward
            // into an empty summary; the final emptiness check still applies.
            warn!(
                step = index + 1,
                "Summariser returned no text for a fold step"
            );
            continue;
        }
        current = Some(text);
    }

    let candidate = current.unwrap_or_default();
    let tokens = count_tokens(&candidate);
    if !candidate.is_empty() && tokens <= budget_tokens {
        return Ok(FittedSummary {
            text: candidate,
            tokens,
            regenerated: false,
            calls,
        });
    }

    if candidate.is_empty() && passages.is_empty() && previous_summary.is_none() {
        // Nothing was folded and there was nothing to carry forward. An empty
        // summary is the honest answer, and the caller commits no summary row.
        return Ok(FittedSummary {
            text: String::new(),
            tokens: 0,
            regenerated: false,
            calls,
        });
    }

    warn!(
        tokens,
        budget_tokens,
        empty = candidate.is_empty(),
        "Working summary does not fit its pool — one regeneration"
    );

    let mut attempt = candidate;
    let mut measured = tokens;
    for _ in 0..MAX_SUMMARY_REGENERATIONS {
        let prompt = render_summary_shrink_prompt(&attempt, measured, budget_tokens);
        calls += 1;
        let text = complete_json(llm, deadline, SUMMARIZER_SYSTEM, prompt, None)
            .await?
            .trim()
            .to_string();
        measured = count_tokens(&text);
        attempt = text;
        if !attempt.is_empty() && measured <= budget_tokens {
            debug!(
                tokens = measured,
                budget_tokens, "Working summary fits after one regeneration"
            );
            return Ok(FittedSummary {
                text: attempt,
                tokens: measured,
                regenerated: true,
                calls,
            });
        }
    }

    if attempt.is_empty() {
        return Err(CompactionError::SummaryEmpty);
    }
    Err(CompactionError::SummaryTooLarge {
        tokens: measured,
        budget: budget_tokens,
    })
}

/// Split the passages into fold steps by byte size. Never called with none.
///
/// One step is the common case. A very long processed range becomes several,
/// each rewriting the running summary, which is lossier than a single pass and
/// is the reason the fold is paged rather than the input truncated.
fn plan_fold(passages: &[SourceMessage]) -> Vec<Vec<SourceMessage>> {
    let mut steps: Vec<Vec<SourceMessage>> = Vec::new();
    let mut current: Vec<SourceMessage> = Vec::new();
    let mut bytes = 0usize;
    for message in passages {
        let size = message.content.len();
        if !current.is_empty() && bytes.saturating_add(size) > MAX_FOLD_BYTES {
            steps.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes = bytes.saturating_add(size);
        current.push(message.clone());
    }
    if !current.is_empty() {
        steps.push(current);
    }
    steps
}
