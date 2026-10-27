//! One claim judged alone, and which of its page's passages the judge was
//! shown when the whole page did not fit its window.
use super::*;
use lattice_core::application::services::claim_verification::ANSWER_ROOM_TOKENS;

/// The order a claim's check needs `page`'s passages in, most first, as
/// positions: passages the app placed (cited, researched, user-picked, example
/// runs) lead, then retrieved ones by score. A page too long for the judge
/// loses passages from the end of this order.
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

/// The strict judgment of one claim against `passages`, the positions the
/// judge was shown (in page order), and what its answer cost. A page too long
/// for the window loses passages from the end of `ranking`, by the request's
/// count and then by any refusal; the claim is left unchecked only when not
/// even the most needed passage fits. Whether support is enough is the
/// provenance gate's decision.
pub(in crate::features::learning) async fn judge_for_publication(
    llm: &dyn LLMPort,
    claim: &str,
    passages: &[String],
    ranking: &[usize],
) -> Judged {
    let page_order: Vec<usize> = (0..passages.len()).collect();
    judge_in_order(llm, claim, passages, ranking, &page_order).await
}

/// [`judge_for_publication`], the judge reading the passages it is shown in
/// `reading` order (positions in the page; a pass B page's
/// `reading_order`). The positions it returns are the page's, whatever order
/// it read.
pub(in crate::features::learning) async fn judge_in_order(
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
        CHECKER_OUTPUT_TOKENS,
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
    let window = llm.max_context_tokens();
    // A page is sized for this judge; one sized for a batch call is cut to
    // what this judge's window holds, least needed first. Every request is
    // sized by the count of what it sends, the backend's own where it can
    // count: a page counted past the room loses as many of its least needed
    // passages as the count says and is counted again, before anything is
    // sent.
    let mut kept = ranking.len();
    let mut cut = false;
    loop {
        let (positions, texts) = shown(kept);
        let counted = checker.located_prompt_tokens_measured(claim, &texts).await;
        if window.saturating_sub(counted) < ANSWER_ROOM_TOKENS {
            if kept == 0 {
                return Judged::unanswered(unchecked(TOO_LONG), Vec::new());
            }
            kept = kept_after_count(
                llm,
                &checker,
                claim,
                (passages, ranking),
                kept,
                (counted, window),
            );
            cut = true;
            continue;
        }
        if std::mem::take(&mut cut) {
            tracing::warn!(
                shown = kept,
                page = passages.len(),
                prompt_tokens = counted,
                "A claim's page holds more than the single-claim judge's window; checking the passages that fit"
            );
        }
        if kept == 0 && !passages.is_empty() {
            return Judged::unanswered(unchecked(TOO_LONG), Vec::new());
        }
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
        let fewer = kept_after_count(
            llm,
            &checker,
            claim,
            (passages, ranking),
            kept,
            (prompt_tokens, window),
        );
        tracing::info!(
            prompt_tokens,
            counted,
            window,
            shown = fewer,
            sent = kept,
            "The backend counted a claim check past the context window; asking again with fewer passages"
        );
        kept = fewer;
    }
}

/// How many of the `kept` most needed passages a claim check keeps once its
/// prompt counted `prompt_tokens`: the least needed go until the overshoot and
/// the answer's room are covered, scaled by how far the count ran from the
/// estimate. At least one goes, so each count is of less than the last.
fn kept_after_count(
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
