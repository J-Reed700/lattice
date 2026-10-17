//! LLM claim judge.
//!
//! The lexical pre-filter can only see shared vocabulary. This asks a model
//! whether the cited passages actually entail the claim, which is the only way
//! to catch a sentence that borrows a source's wording and inverts its meaning.
//!
//! One claim per request. The evidence goes first and the claim last, so
//! llama-server's prompt cache reuses a page's prefix across the claims that
//! cite it. The answer starts with a single label whose probability is read
//! from the first token's log-probabilities, followed by a bounded explanation
//! and an exact source quote. Failed or unreadable judgments return no
//! verdict; the verifier decides what an unsettled claim reads as.
//!
//! No clock bounds the judging. Every cited claim is judged, however slow the
//! model: the adapter's stall detection ends a call whose server went quiet,
//! the answer's output reservation ends a runaway one, and app shutdown
//! cancels the background check.

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use tracing::debug;

use crate::features::qa::dto::SourceDto;
use lattice_core::application::contracts::settings::LLMVerificationSettingsDto;
use lattice_core::application::ports::llm_port::{chars_within_tokens, SamplingOverride};
#[cfg(test)]
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest};
use lattice_core::application::ports::LLMPort;
#[cfg(test)]
use lattice_core::application::services::claim_verification::{
    parse_judge_explanation, parse_verdict_word, verdict_from_logprobs,
};
use lattice_core::application::services::claim_verification::{
    CheckPolicy, ClaimChecker, ClaimVerdict, JudgeOutcome,
};
pub(super) use lattice_core::application::services::claim_verification::{
    ClaimEvidence, ClaimJudgment, MIN_VERDICT_CONFIDENCE,
};
use lattice_core::application::services::evidence_retrieval::{
    join_windows, scored_windows, WINDOW_OVERLAP_CHARS,
};
#[cfg(test)]
use lattice_core::shared::error::Result;

use super::lexical::{best_sentence, best_windows, LexicalClaim, ScoredWindow};

/// Output room for the model's reasoning followed by the public verdict,
/// factual explanation, and exact source quote. Reasoning tokens count toward
/// the provider's output budget even though they are not shown in the verdict.
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

const MAX_QUOTE_CHARS: usize = 400;

/// Between two windows of one page: the text in between was left out.
const WINDOW_SEPARATOR: &str = "\n[…]\n";

/// Whether a claim has anything to be judged against.
#[derive(Debug, Clone)]
pub(super) enum Evidence {
    /// What the claim is judged against, in as many calls as it takes: each
    /// part fits one judge call, best evidence first, and the claim is
    /// supported when any part supports it.
    Found(Vec<ClaimEvidence>),
    /// Every citation resolves, but no cited source has any text: the page was
    /// never archived or came back empty. Nothing was checked.
    NoText,
    /// No citation, or one that points at nothing. A claim with no evidence is
    /// not an unchecked claim: it has nothing behind it.
    NoCitation,
}

pub(super) struct ClaimJudge {
    llm: Arc<dyn LLMPort>,
    /// How the verdict is decoded. Greedy unless the user says otherwise.
    sampling: SamplingOverride,
    max_output_tokens: u32,
}

impl ClaimJudge {
    pub(super) fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self {
            llm,
            // Deterministic by default, so a judge built anywhere in the code
            // cannot quietly inherit a creative model's sampling: measured on
            // the bundled 9B at the chat default of 0.7, one claim against one
            // passage came back supported, unsupported and contradicted across
            // twenty-one runs of the same request.
            sampling: SamplingOverride::deterministic(),
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
        }
    }

    /// Apply the user's sampling settings while retaining enough output room
    /// for reasoning, a verdict, and its supporting explanation.
    pub(super) fn with_tuning(mut self, tuning: &LLMVerificationSettingsDto) -> Self {
        self.sampling = SamplingOverride {
            temperature: Some(tuning.temperature),
            top_p: Some(tuning.top_p),
            top_k: Some(tuning.top_k),
            // The judge's prompt lists the verdict labels just before it
            // answers; a presence penalty would push it off the one it means.
            presence_penalty: Some(0.0),
        };
        self
    }

    pub(super) fn model_name(&self) -> &str {
        self.llm.model_name()
    }

    /// Characters of source text one judge call about `claim` can carry: the
    /// judge's window less both prompts and the answer's room. What does not
    /// fit is judged in a further part.
    pub(super) fn evidence_room_chars(&self, claim: &str) -> usize {
        chars_within_tokens(
            self.checker().evidence_room_tokens(claim),
            self.llm.chars_per_token(),
        )
    }

    fn checker(&self) -> ClaimChecker<'_> {
        ClaimChecker::new(
            self.llm.as_ref(),
            self.sampling,
            self.max_output_tokens,
            CheckPolicy::Chat,
        )
    }

    /// Judge each `(claim index, claim text, evidence)`, keyed back by index.
    ///
    /// Every request gets an entry, and every part of a claim still open is
    /// judged. Parts go round by round: every claim's best part first, then
    /// the second part of each claim still open, and so on, so a claim citing
    /// a long page does not hold back the first call of the claims after it.
    /// Within a round every call is submitted at once and the backend's
    /// scheduler decides how many run side by side, at verification priority,
    /// behind the user's next turn.
    pub(super) async fn judge_claims(
        &self,
        requests: &[(usize, &str, Vec<ClaimEvidence>)],
    ) -> HashMap<usize, ClaimJudgment> {
        let mut judgments = HashMap::new();
        if requests.is_empty() {
            return judgments;
        }

        let mut tallies: Vec<Tally> = requests.iter().map(|_| Tally::default()).collect();
        let rounds = requests
            .iter()
            .map(|(_, _, parts)| parts.len())
            .max()
            .unwrap_or(0);
        for round in 0..rounds {
            // Boxed: the borrowed `async fn` futures otherwise trip the
            // compiler's higher-ranked `Send` check once the whole turn is
            // spawned.
            let calls: Vec<BoxFuture<'_, (usize, ClaimJudgment)>> = requests
                .iter()
                .enumerate()
                .filter(|(slot, _)| tallies.get(*slot).is_some_and(Tally::is_open))
                .filter_map(|(slot, (_, claim, parts))| {
                    let part = parts.get(round)?;
                    Some(async move { (slot, self.checker().check(claim, part).await) }.boxed())
                })
                .collect();
            let mut calls: futures::stream::FuturesUnordered<_> = calls.into_iter().collect();
            while let Some((slot, judgment)) = calls.next().await {
                if let Some(tally) = tallies.get_mut(slot) {
                    tally.record(judgment);
                }
            }
        }
        for ((index, _, _), tally) in requests.iter().zip(tallies) {
            judgments.insert(*index, tally.finish());
        }

        let judged = judgments
            .values()
            .filter(|judgment| matches!(judgment, ClaimJudgment::Judged(_)))
            .count();
        debug!(
            judged,
            requested = requests.len(),
            model = self.llm.model_name(),
            "Claim judge finished"
        );
        judgments
    }
}

/// What the parts of one claim judged so far add up to.
///
/// The first part that supports the claim confidently settles it: one cited
/// source supporting a claim is enough. A claim no part supports takes the
/// strongest finding among them. A part that could not be judged (a
/// transport failure, a refused prompt, or a reply with no verdict) settles
/// the claim as unchecked and its later parts are not
/// asked: the part left over might have held the support, so it is never
/// called unsupported, and a failing backend is not asked again and again.
#[derive(Default)]
struct Tally {
    strongest: Option<JudgeOutcome>,
    settled: Option<ClaimJudgment>,
}

impl Tally {
    fn is_open(&self) -> bool {
        self.settled.is_none()
    }

    fn record(&mut self, judgment: ClaimJudgment) {
        match judgment {
            ClaimJudgment::Judged(outcome) => {
                if outcome.verdict == ClaimVerdict::Supported && confident(&outcome) {
                    self.settled = Some(ClaimJudgment::Judged(outcome));
                    return;
                }
                self.strongest = Some(match self.strongest.take() {
                    Some(held) if finding_rank(&held) >= finding_rank(&outcome) => held,
                    _ => outcome,
                });
            }
            unjudged => self.settled = Some(unjudged),
        }
    }

    fn finish(self) -> ClaimJudgment {
        if let Some(settled) = self.settled {
            return settled;
        }
        self.strongest
            .map_or(ClaimJudgment::Unusable, ClaimJudgment::Judged)
    }
}

fn confident(outcome: &JudgeOutcome) -> bool {
    outcome
        .confidence
        .is_none_or(|confidence| confidence >= MIN_VERDICT_CONFIDENCE)
}

/// How strong a finding is when no part supported a claim confidently: a
/// hesitant support first, since the reader is then told the check was unsure
/// rather than shown a negative it did not establish; then a contradiction,
/// which a passage stated outright; then a missing fact. Ties go to the
/// surer verdict.
fn finding_rank(outcome: &JudgeOutcome) -> (u8, u32) {
    let kind = match outcome.verdict {
        ClaimVerdict::Supported => 3,
        ClaimVerdict::Contradicted => 2,
        _ => 1,
    };
    let sureness = outcome
        .confidence
        .map(|confidence| (confidence.clamp(0.0, 1.0) * 1_000.0) as u32)
        .unwrap_or(1_000);
    (kind, sureness)
}

/// The label line a cited source's text sits under in a judge prompt.
fn source_label(citation_id: u32) -> String {
    format!("[{citation_id}]\n")
}

/// Between two sources' blocks in one judge prompt.
const BLOCK_SEPARATOR: &str = "\n\n";

/// The evidence one claim is judged against, in parts that each fit `room`
/// characters of a judge call.
///
/// Every cited source is read. When their whole texts fit one call, that call
/// gets them whole. Otherwise each source is cut into its overlapping windows,
/// those sharing the claim's vocabulary are ranked, and they are taken best of
/// every source before second-best of any, filling one call and then the next
/// until every such window has a place: support far into a long page is
/// judged in a later call instead of being left out.
///
/// Only the passages the claim cites. A sentence that cites nothing has no
/// evidence; offering it whichever sources ranked first would make the judge
/// rule on text the sentence never made a statement about.
pub(super) fn evidence_for(claim: &LexicalClaim, sources: &[SourceDto], room: usize) -> Evidence {
    if claim.citation_ids.is_empty() || claim.citation_ids.len() != claim.cited_source_indices.len()
    {
        return Evidence::NoCitation;
    }

    // (citation number, text) per cited source with any text.
    let mut cited: Vec<(u32, &str)> = Vec::new();
    for &idx in &claim.cited_source_indices {
        let Some(source) = sources.get(idx) else {
            return Evidence::NoCitation;
        };
        let body = if source.content.trim().is_empty() {
            source.excerpt.as_deref().unwrap_or_default()
        } else {
            source.content.as_str()
        };
        if body.trim().is_empty() {
            continue;
        }
        let citation_id = source
            .citation_id
            .unwrap_or_else(|| u32::try_from(idx + 1).unwrap_or(u32::MAX));
        cited.push((citation_id, body.trim()));
    }
    if cited.is_empty() {
        return Evidence::NoText;
    }

    let blocks: Vec<String> = cited
        .iter()
        .map(|(citation_id, body)| format!("{}{body}", source_label(*citation_id)))
        .collect();
    let whole_cost = blocks
        .iter()
        .map(|block| block.chars().count())
        .sum::<usize>()
        + BLOCK_SEPARATOR.len() * blocks.len().saturating_sub(1);
    if whole_cost <= room {
        let quote = cited
            .iter()
            .filter_map(|(_, body)| best_windows(&claim.claim_text, body, 1).into_iter().next())
            .max_by_key(|window| window.score)
            .and_then(|window| best_sentence(&claim.claim_text, &window.text))
            .map(|sentence| truncate_chars(&sentence, MAX_QUOTE_CHARS));
        return Evidence::Found(vec![ClaimEvidence {
            text: blocks.join(BLOCK_SEPARATOR),
            quote,
        }]);
    }

    // (citation number, windows best first) per source with any window that
    // shares the claim's vocabulary.
    let per_source: Vec<(u32, Vec<ScoredWindow>)> = cited
        .iter()
        .map(|(citation_id, body)| {
            // Every window, best first; stable, so equal scores keep page
            // order. A window sharing no words with the claim is kept, in
            // the last parts: a paraphrase shares none and is still support.
            let mut windows = scored_windows(&claim.claim_text, body);
            windows.sort_by_key(|window| std::cmp::Reverse(window.score));
            (*citation_id, windows)
        })
        .filter(|(_, windows)| !windows.is_empty())
        .collect();
    if per_source.is_empty() {
        return Evidence::NoText;
    }

    let deepest = per_source
        .iter()
        .map(|(_, windows)| windows.len())
        .max()
        .unwrap_or(0);
    let mut ranked: Vec<(usize, &ScoredWindow)> = Vec::new();
    for rank in 0..deepest {
        for (slot, (_, windows)) in per_source.iter().enumerate() {
            if let Some(window) = windows.get(rank) {
                ranked.push((slot, window));
            }
        }
    }

    let empty_part = || vec![Vec::<&ScoredWindow>::new(); per_source.len()];
    let mut parts: Vec<Vec<Vec<&ScoredWindow>>> = Vec::new();
    let mut current = empty_part();
    let mut used = 0usize;
    for (slot, window) in ranked {
        let Some(citation_id) = per_source.get(slot).map(|(id, _)| *id) else {
            continue;
        };
        let mut cost = rendered_cost(
            current.get(slot).map_or(&[][..], Vec::as_slice),
            window,
            citation_id,
        );
        // A part always takes at least one window: a claim is never judged
        // against nothing because one window was long.
        if used > 0 && used + cost > room {
            parts.push(std::mem::replace(&mut current, empty_part()));
            used = 0;
            cost = window.text.chars().count()
                + source_label(citation_id).chars().count()
                + BLOCK_SEPARATOR.len();
        }
        if let Some(list) = current.get_mut(slot) {
            list.push(window);
        }
        used += cost;
    }
    if used > 0 {
        parts.push(current);
    }

    Evidence::Found(
        parts
            .into_iter()
            .map(|part| render_part(&claim.claim_text, &per_source, part))
            .collect(),
    )
}

/// What adding `window` to the windows `taken` from its source in one part
/// adds to the rendered part. Windows overlap their neighbours, and joined
/// neighbours show the shared text once, so a window next to one already
/// taken costs only what it adds; costing every window whole split a page
/// into about half as many parts again as it needed.
fn rendered_cost(taken: &[&ScoredWindow], window: &ScoredWindow, citation_id: u32) -> usize {
    let length = window.text.chars().count();
    if taken.is_empty() {
        return length + source_label(citation_id).chars().count() + BLOCK_SEPARATOR.len();
    }
    let beside =
        |index: Option<usize>| index.is_some_and(|index| taken.iter().any(|t| t.index == index));
    let left = beside(window.index.checked_sub(1));
    let right = beside(window.index.checked_add(1));
    match (left, right) {
        (true, true) => length
            .saturating_sub(2 * WINDOW_OVERLAP_CHARS)
            .saturating_sub(WINDOW_SEPARATOR.chars().count()),
        (true, false) | (false, true) => length.saturating_sub(WINDOW_OVERLAP_CHARS),
        (false, false) => length + WINDOW_SEPARATOR.chars().count(),
    }
}

/// One judge call's evidence: each source's windows in reading order under
/// its citation number, and the best window's closest sentence as the quote.
fn render_part(
    claim: &str,
    per_source: &[(u32, Vec<ScoredWindow>)],
    part: Vec<Vec<&ScoredWindow>>,
) -> ClaimEvidence {
    let mut blocks = Vec::new();
    for ((citation_id, _), windows) in per_source.iter().zip(&part) {
        if windows.is_empty() {
            continue;
        }
        blocks.push(format!(
            "{}{}",
            source_label(*citation_id),
            join_windows(windows, WINDOW_SEPARATOR)
        ));
    }
    let quote = part
        .iter()
        .flatten()
        .max_by_key(|window| window.score)
        .and_then(|window| best_sentence(claim, &window.text))
        .map(|sentence| truncate_chars(&sentence, MAX_QUOTE_CHARS));
    ClaimEvidence {
        text: blocks.join(BLOCK_SEPARATOR),
        quote,
    }
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests;
