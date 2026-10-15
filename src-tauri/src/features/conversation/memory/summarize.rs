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

use lattice_core::application::ports::llm_port::chars_within_tokens;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::memory::SourceMessage;
use lattice_core::shared::text::split_into_parts;

use super::prompts::{
    render_summary_prompt, render_summary_shrink_prompt, SummaryPassage, SUMMARIZER_SYSTEM,
};
use super::{complete_json_within, CompactionError, Deadline};

/// Smallest fold step, in tokens, so a window too small to plan for still
/// makes progress; a request it cannot hold fails visibly at the provider.
const MIN_FOLD_TOKENS: usize = 256;

/// Tokens per passage for its JSON framing: sequence, role and the quotes.
const PASSAGE_FRAMING_TOKENS: usize = 16;

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
/// that fits `budget_tokens`, leaving out the facts in `forgotten_labels`.
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
    forgotten_labels: &[String],
    budget_tokens: usize,
    count_tokens: &(dyn Fn(&str) -> usize + Send + Sync),
) -> std::result::Result<FittedSummary, CompactionError> {
    let mut current = previous_summary.map(str::to_string);
    let mut calls = 0usize;
    let output_tokens = summary_output_tokens(budget_tokens);

    // Nothing new to fold. This happens when a compacted prefix held only
    // ineligible material; spending a call to rewrite an unchanged summary would
    // only risk losing something from it.
    let steps = if passages.is_empty() {
        Vec::new()
    } else {
        let room = fold_room_chars(llm, mandatory_labels, forgotten_labels, budget_tokens);
        plan_fold(passages, room)
    };

    for (index, step) in steps.iter().enumerate() {
        let rendered: Vec<SummaryPassage<'_>> = step
            .iter()
            .map(|piece| SummaryPassage {
                sequence: piece.sequence,
                role: piece.role,
                text: &piece.text,
            })
            .collect();
        let prompt = render_summary_prompt(
            current.as_deref(),
            &rendered,
            mandatory_labels,
            forgotten_labels,
            budget_tokens,
            index + 1 == steps.len(),
        );
        calls += 1;
        let text = complete_json_within(
            llm,
            deadline,
            SUMMARIZER_SYSTEM,
            prompt,
            None,
            output_tokens,
        )
        .await?;
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
        let text = complete_json_within(
            llm,
            deadline,
            SUMMARIZER_SYSTEM,
            prompt,
            None,
            output_tokens,
        )
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

/// Answer room a summariser call reserves: the summary's budget and half
/// again, so a candidate that overruns can still be measured and shrunk
/// rather than cut off mid-sentence by the provider.
fn summary_output_tokens(budget_tokens: usize) -> usize {
    budget_tokens
        .saturating_add(budget_tokens / 2)
        .max(MIN_FOLD_TOKENS)
}

/// Characters of new passages one fold step can carry in the summariser's
/// own window, after its answer room, a 5% margin, the system prompt, the
/// request's fixed fields and a running summary as long as an answer may be.
fn fold_room_chars(
    llm: &dyn LLMPort,
    mandatory_labels: &[String],
    forgotten_labels: &[String],
    budget_tokens: usize,
) -> usize {
    let window = llm.max_context_tokens();
    let output = summary_output_tokens(budget_tokens);
    let template = render_summary_prompt(
        None,
        &[],
        mandatory_labels,
        forgotten_labels,
        budget_tokens,
        false,
    );
    let fixed = llm
        .count_tokens(SUMMARIZER_SYSTEM)
        .saturating_add(llm.count_tokens(&template))
        // The running summary rides in every step after the first.
        .saturating_add(output);
    let room = window
        .saturating_sub(output)
        .saturating_sub(window / 20)
        .saturating_sub(fixed)
        .max(MIN_FOLD_TOKENS);
    chars_within_tokens(room, llm.chars_per_token())
}

/// A message, or one part of a message too long for one fold step.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FoldPiece<'a> {
    sequence: i64,
    role: &'a str,
    text: String,
}

/// Split the passages into fold steps that each fit `room_chars`. Never
/// called with none.
///
/// One step is the common case. A very long processed range becomes several,
/// each rewriting the running summary, which is lossier than a single pass and
/// is the reason the fold is paged rather than the input truncated. A message
/// longer than a whole step is split into marked parts, every part folded in
/// turn, so nothing past its opening is lost to the summary.
fn plan_fold(passages: &[SourceMessage], room_chars: usize) -> Vec<Vec<FoldPiece<'_>>> {
    let framing = chars_within_tokens(PASSAGE_FRAMING_TOKENS, 4.0);
    let room = room_chars.max(framing + 1);
    let mut steps: Vec<Vec<FoldPiece<'_>>> = Vec::new();
    let mut current: Vec<FoldPiece<'_>> = Vec::new();
    let mut used = 0usize;
    for message in passages {
        for text in split_message(&message.content, room - framing) {
            let size = text.chars().count() + framing;
            if !current.is_empty() && used.saturating_add(size) > room {
                steps.push(std::mem::take(&mut current));
                used = 0;
            }
            used = used.saturating_add(size);
            current.push(FoldPiece {
                sequence: message.sequence,
                role: message.role.as_str(),
                text,
            });
        }
    }
    if !current.is_empty() {
        steps.push(current);
    }
    steps
}

/// `text` whole when it fits `max_chars`, otherwise in parts that each do,
/// marked `[part i of n]`.
fn split_message(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    // Room for the mark itself.
    let parts = split_into_parts(text, max_chars.saturating_sub(24).max(1));
    let total = parts.len();
    parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| format!("[part {} of {total}] {part}", index + 1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::domain::conversation::memory::SourceRole;

    fn message(sequence: i64, content: &str) -> SourceMessage {
        SourceMessage {
            id: format!("m{sequence}"),
            conversation_id: "c".to_string(),
            sequence,
            role: SourceRole::User,
            content: content.to_string(),
            content_digest: String::new(),
            status: "completed".to_string(),
        }
    }

    /// A long message used to reach the summariser as its first 4,000
    /// characters. It is now folded in parts, and every character arrives.
    #[test]
    fn a_message_longer_than_a_step_is_folded_in_marked_parts_with_nothing_lost() {
        let long: String = (0..3_000).map(|index| format!("w{index} ")).collect();
        let passages = vec![message(1, "short"), message(2, &long)];
        let steps = plan_fold(&passages, 2_000);
        assert!(steps.len() > 1, "{} steps", steps.len());
        let pieces: Vec<&FoldPiece<'_>> = steps.iter().flatten().collect();
        assert_eq!(pieces[0].text, "short");
        let parts: Vec<&FoldPiece<'_>> =
            pieces.iter().copied().filter(|p| p.sequence == 2).collect();
        let total = parts.len();
        let mut rebuilt = String::new();
        for (index, part) in parts.iter().enumerate() {
            let mark = format!("[part {} of {total}] ", index + 1);
            assert!(part.text.starts_with(&mark), "{}", &part.text[..30]);
            assert!(part.text.chars().count() <= 2_000);
            rebuilt.push_str(&part.text[mark.len()..]);
        }
        assert_eq!(rebuilt, long);
        for step in &steps {
            let chars: usize = step.iter().map(|piece| piece.text.chars().count()).sum();
            assert!(chars <= 2_000, "{chars}");
        }
    }

    /// Passages that fit one step are folded in one call, unmarked.
    #[test]
    fn passages_that_fit_are_one_step_as_written() {
        let passages = vec![message(1, "first"), message(2, "second")];
        let steps = plan_fold(&passages, 10_000);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0][0].text, "first");
        assert_eq!(steps[0][1].text, "second");
    }
}
