//! One claim judged alone, and which of its page's passages the judge was
//! shown when the whole page did not fit its window.
use super::*;
use lattice_core::application::services::claim_verification::ANSWER_ROOM_TOKENS;

/// The order in which a claim's check needs `page`'s passages, most first,
/// as positions in the page. Passages the app placed on the page lead: the
/// section's cited passages, what research found for the claim, a passage the
/// user picked, a passed example run. Their score is no rank. Retrieved
/// passages follow by retrieval score, ties in page order. A page is sorted
/// by source to key it (`canonicalize_evidence`), so its own order says
/// nothing about which passage matters: a page too long for the judge's
/// window loses passages from the end of this order instead, and the judge
/// reads the passages in an order built on it (`reading_order`).
pub(in crate::features::learning) fn ranking(page: &[EvidencePassage]) -> Vec<usize> {
    let placed = |passage: &EvidencePassage| match passage.retrieval_kind.as_str() {
        CITED => 0,
        evidence_selection::RESEARCHED => 1,
        "user_selected" => 2,
        "execution" => 3,
        _ => 4,
    };
    let mut order: Vec<(usize, &EvidencePassage)> = page.iter().enumerate().collect();
    // Stable: equal kinds and scores keep their page order.
    order.sort_by(|(_, a), (_, b)| {
        placed(a)
            .cmp(&placed(b))
            .then_with(|| b.score.total_cmp(&a.score))
    });
    order.into_iter().map(|(position, _)| position).collect()
}

/// The passages of `page` at `shown` positions, in page order.
pub(in crate::features::learning) fn shown_of<T: Clone>(page: &[T], shown: &[usize]) -> Vec<T> {
    page.iter()
        .enumerate()
        .filter(|(position, _)| shown.contains(position))
        .map(|(_, passage)| passage.clone())
        .collect()
}

/// One claim's strict judgment, as [`judge_for_publication`] gives it.
pub(in crate::features::learning) struct Judged {
    pub(in crate::features::learning) judgment: ClaimJudgment,
    /// The positions of the passages the judge was shown, in page order.
    pub(in crate::features::learning) shown: Vec<usize>,
    /// Output tokens the judge's replies took: what this claim's answer cost.
    pub(in crate::features::learning) output_tokens: u64,
}

impl Judged {
    /// A judgment no reply was written for.
    fn unanswered(judgment: ClaimJudgment, shown: Vec<usize>) -> Self {
        Self {
            judgment,
            shown,
            output_tokens: 0,
        }
    }
}

/// The strict judgment of one claim against `passages`, the positions of
/// the passages the judge was shown, in page order, and what its answer
/// cost. `ranking` orders the
/// passages by how much the check needs them, most first (see [`ranking`]);
/// `reading` is the order the judge reads them in, as positions in the page
/// (the page order itself, or a lesson check's `reading_order`). A page too
/// long for this judge's window loses passages from the end of the ranking:
/// those the calibrated estimate says do not fit, then, when the backend
/// counts the prompt past the window anyway, as many more as its refusal
/// says, and the judge is asked again. The judge reads the passages it is
/// shown in `reading` order, and the positions it returns are the page's,
/// whatever order it read. A passage it was not shown is no part of the
/// judgment, so the caller binds the finding to the shown ones alone. Only when not even the most needed passage fits is the claim left
/// unchecked. Whether a supported verdict's support is enough is the
/// provenance gate's decision, never a second model call (source provenance
/// §11).
pub(in crate::features::learning) async fn judge_for_publication(
    llm: &dyn LLMPort,
    claim: &str,
    passages: &[String],
    ranking: &[usize],
    reading: &[usize],
) -> Judged {
    const TOO_LONG: &str =
        "The claim and its first passage do not fit the claim check in this model's context window.";
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    );
    // The positions shown when the `kept` most needed passages are, in page
    // order, and those passages' texts in reading order.
    let shown = |kept: usize| {
        let mut positions: Vec<usize> = ranking.iter().copied().take(kept).collect();
        positions.sort_unstable();
        let texts: Vec<String> = reading
            .iter()
            .filter(|position| positions.contains(position))
            .filter_map(|position| passages.get(*position).cloned())
            .collect();
        (positions, texts)
    };
    // A page is sized for this judge; one sized for a batch call is cut to
    // what this judge's window holds, least needed first.
    let mut kept = ranking.len();
    while kept > 0 && !checker.located_fits(claim, &shown(kept).1) {
        kept -= 1;
    }
    if kept < passages.len() {
        tracing::warn!(
            shown = kept,
            page = passages.len(),
            "A claim's page holds more than the single-claim judge's window; checking the passages that fit"
        );
    }
    loop {
        if kept == 0 && !passages.is_empty() {
            return Judged::unanswered(unchecked(TOO_LONG), Vec::new());
        }
        let (positions, texts) = shown(kept);
        let refusal = {
            let _model_call = crate::features::learning::lesson_progress::model_call();
            match checker.check_passages_or_size_refusal(claim, &texts).await {
                Ok(answered) => {
                    return Judged {
                        judgment: answered.judgments,
                        shown: positions,
                        output_tokens: answered.output_tokens,
                    }
                }
                Err(refusal) => refusal,
            }
        };
        let AppError::PromptExceedsWindow {
            prompt_tokens,
            window,
            ..
        } = refusal
        else {
            return Judged::unanswered(ClaimJudgment::from_request_error(refusal), positions);
        };
        if kept == 0 {
            return Judged::unanswered(unchecked(TOO_LONG), Vec::new());
        }
        let fewer = kept_after_refusal(
            llm,
            &checker,
            claim,
            (passages, ranking),
            kept,
            (prompt_tokens, window),
        );
        tracing::info!(
            prompt_tokens,
            window,
            shown = fewer,
            sent = kept,
            "The backend counted a claim check past the context window; asking again with fewer passages"
        );
        kept = fewer;
    }
}

/// How many of the `kept` most needed passages (`ranking` into `passages`) a
/// claim check sends after the backend counted its prompt at
/// `prompt_tokens`, past `window`: the least needed go until those dropped
/// cover the overshoot and the answer's room. Each dropped passage's
/// estimate is scaled by how far the backend's count ran past the estimate
/// of the whole prompt, so one step usually lands; a step that does not is
/// refused again with the new numbers. At least one passage always goes, so
/// each request sends less than the last.
fn kept_after_refusal(
    llm: &dyn LLMPort,
    checker: &ClaimChecker<'_>,
    claim: &str,
    (passages, ranking): (&[String], &[usize]),
    kept: usize,
    (prompt_tokens, window): (usize, usize),
) -> usize {
    let mut sent: Vec<usize> = ranking.iter().copied().take(kept).collect();
    sent.sort_unstable();
    let estimate = checker
        .located_prompt_tokens(claim, &shown_of(passages, &sent))
        .max(1);
    let needed = prompt_tokens
        .saturating_sub(window)
        .saturating_add(ANSWER_ROOM_TOKENS);
    let mut kept = kept;
    let mut freed = 0usize;
    while kept > 0 && freed < needed {
        kept -= 1;
        let passage = ranking
            .get(kept)
            .and_then(|position| passages.get(*position))
            .map(String::as_str)
            .unwrap_or_default();
        let cost = llm.count_tokens(&format!("[passage-{kept}]\n{passage}\n\n"));
        freed = freed.saturating_add(cost.saturating_mul(prompt_tokens).div_ceil(estimate));
    }
    kept
}
