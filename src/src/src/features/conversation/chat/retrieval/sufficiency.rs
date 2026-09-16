//! Post-rerank sufficiency signal for corrective retrieval.
//!
//! Retrieval always returns *something*: RRF ranks every candidate it saw, so
//! an empty result list is the only failure the pipeline used to notice. This
//! module adds a cheap, purely local second opinion — no model call — that the
//! pipeline consults once, after reranking and before context assembly, to
//! decide whether one replanned retry is worth its latency.
//!
//! Three signals, deliberately weak on their own:
//!
//! * **Top blended score.** Only meaningful when the cross-encoder actually
//!   ran. `blend_rerank_scores` mixes `0.35 * normalized_first_stage + 0.65 *
//!   rerank`, so the first-stage winner alone is worth `0.35`. A top score
//!   below that floor means no passage earned any cross-encoder relevance.
//!   Without a reranker the scores are RRF ranks, which are not probabilities
//!   and must never be thresholded, so the score is reported and ignored.
//! * **Spread** between the first and fifth result. A reranker that separated
//!   nothing has not ranked; its top score is then no more trustworthy than an
//!   RRF rank, so we fall back to the no-reranker rule.
//! * **Lexical term coverage**: the fraction of the planner's informative query
//!   terms that appear anywhere in the top passages. This is the signal that
//!   catches the real failure mode — a confident ranking over passages that
//!   simply never mention what was asked about.

use std::collections::HashSet;

use crate::features::search::dto::SearchResultDto;
use crate::features::settings::dto::RetrievalTuningSettingsDto;

use super::corpus_plan::CorpusSearchPlan;
use super::{select_informative_terms, tokenize_keyword_terms, tokenize_overlap_terms};

/// How many ranked passages the coverage probe reads, and the window the
/// top1-minus-topN spread is measured over.
pub(super) const COVERAGE_TOP_N: usize = 5;

/// Planner queries are short by construction (at most three, 24 words each).
/// Coverage over more terms than this measures phrasing, not topicality.
const MAX_PLAN_TERMS: usize = 12;

/// Without a cross-encoder there is no calibrated score to read, so quantity is
/// the only other evidence that the branches found a seam of relevant text.
const MIN_RESULTS_WITHOUT_RERANK: usize = 3;

/// Below this, the reranker assigned the whole shortlist the same relevance.
const FLAT_SPREAD_EPSILON: f32 = 0.01;

/// Why a turn's retrieval was judged sufficient or not. Codes, not prose: they
/// go to the trace and to tests, and both want to match on them.
pub(super) mod reason {
    pub const NO_RESULTS: &str = "no_results";
    pub const LOW_TOP_SCORE: &str = "low_top_score";
    pub const FLAT_RERANK_SPREAD: &str = "flat_rerank_spread";
    pub const LOW_TERM_COVERAGE: &str = "low_term_coverage";
    pub const TOO_FEW_RESULTS: &str = "too_few_results";
    pub const NO_RERANKER: &str = "no_reranker";
    pub const ORDERED_READING: &str = "ordered_reading";
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::features::conversation::chat) struct SufficiencyVerdict {
    /// False only when a corrective retry could plausibly do better.
    pub sufficient: bool,
    /// Blended score of the best passage. Comparable across turns only when
    /// `reranked` was true for both.
    pub top_score: f32,
    /// `top_score` minus the score at rank `COVERAGE_TOP_N`, or minus the last
    /// result's score when fewer came back.
    pub spread: f32,
    /// Fraction of informative planner query terms found in the top passages.
    pub term_coverage: f32,
    pub reasons: Vec<&'static str>,
}

impl Default for SufficiencyVerdict {
    fn default() -> Self {
        Self {
            sufficient: true,
            top_score: 0.0,
            spread: 0.0,
            term_coverage: 0.0,
            reasons: Vec::new(),
        }
    }
}

/// Judge one pass of retrieval. Pure: no I/O, no clock, no model.
///
/// `reranked` must be true only when the cross-encoder actually rescored the
/// candidates. Passing true when it did not would threshold RRF ranks as if
/// they were relevance probabilities, which they are not.
pub(super) fn assess_sufficiency(
    results: &[SearchResultDto],
    plan: &CorpusSearchPlan,
    tuning: &RetrievalTuningSettingsDto,
    reranked: bool,
) -> SufficiencyVerdict {
    let Some(top) = results.first() else {
        return SufficiencyVerdict {
            sufficient: false,
            reasons: vec![reason::NO_RESULTS],
            ..Default::default()
        };
    };

    let top_score = top.score;
    let tail = results
        .get(COVERAGE_TOP_N - 1)
        .or_else(|| results.last())
        .map_or(0.0, |result| result.score);
    let spread = top_score - tail;
    let term_coverage = term_coverage(results, plan);

    // Ordered reading is deliberate evidence selection — the planner chose the
    // introduction and first chapter on purpose. Replanning it into a keyword
    // hunt would undo the choice, so such a plan is sufficient by construction.
    if plan.start_at_beginning || !plan.opening_document_ids.is_empty() {
        return SufficiencyVerdict {
            sufficient: true,
            top_score,
            spread,
            term_coverage,
            reasons: vec![reason::ORDERED_READING],
        };
    }

    let mut reasons = Vec::new();
    // A reranker that separated nothing has not ranked. Its top score carries
    // no more information than the RRF rank underneath it.
    let flat = reranked && results.len() >= COVERAGE_TOP_N && spread <= FLAT_SPREAD_EPSILON;
    if flat {
        reasons.push(reason::FLAT_RERANK_SPREAD);
    }
    let score_is_readable = reranked && !flat;
    if !score_is_readable && !flat {
        reasons.push(reason::NO_RERANKER);
    }

    let score_ok = !score_is_readable || top_score >= tuning.sufficiency_min_top_score;
    if !score_ok {
        reasons.push(reason::LOW_TOP_SCORE);
    }

    let coverage_ok = term_coverage >= tuning.sufficiency_min_term_coverage;
    if !coverage_ok {
        reasons.push(reason::LOW_TERM_COVERAGE);
    }

    // Quantity only substitutes for a score we cannot read.
    let count_ok = score_is_readable || results.len() >= MIN_RESULTS_WITHOUT_RERANK;
    if !count_ok {
        reasons.push(reason::TOO_FEW_RESULTS);
    }

    SufficiencyVerdict {
        sufficient: score_ok && coverage_ok && count_ok,
        top_score,
        spread,
        term_coverage,
        reasons,
    }
}

/// Fraction of the plan's informative terms that appear in the top passages.
///
/// A plan with no informative terms (a bare acronym, a section number) cannot
/// fail coverage — there is nothing to miss, and reporting 0.0 would send every
/// such turn into a pointless retry.
fn term_coverage(results: &[SearchResultDto], plan: &CorpusSearchPlan) -> f32 {
    let plan_terms = select_informative_terms(
        tokenize_keyword_terms(&plan.queries.join(" ")),
        MAX_PLAN_TERMS,
    );
    if plan_terms.is_empty() {
        return 1.0;
    }

    let mut passage_terms: HashSet<String> = HashSet::new();
    for result in results.iter().take(COVERAGE_TOP_N) {
        passage_terms.extend(tokenize_overlap_terms(&result.title));
        passage_terms.extend(tokenize_overlap_terms(&result.content));
    }

    let hits = plan_terms
        .iter()
        .filter(|term| passage_terms.contains(term.as_str()))
        .count();
    hits as f32 / plan_terms.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn result(id: &str, score: f32, content: &str) -> SearchResultDto {
        SearchResultDto {
            id: id.to_string(),
            title: "Guide".to_string(),
            content: content.to_string(),
            score,
            path: None,
            document_id: Some("doc".to_string()),
            position: None,
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    fn plan(query: &str) -> CorpusSearchPlan {
        CorpusSearchPlan {
            queries: vec![query.to_string()],
            opening_document_ids: Vec::new(),
            start_at_beginning: false,
        }
    }

    fn tuning() -> RetrievalTuningSettingsDto {
        RetrievalTuningSettingsDto::default()
    }

    #[test]
    fn empty_results_are_insufficient() {
        let verdict = assess_sufficiency(&[], &plan("noncompete clauses"), &tuning(), true);

        assert!(!verdict.sufficient);
        assert_eq!(verdict.reasons, [reason::NO_RESULTS]);
        assert_eq!(verdict.top_score, 0.0);
        assert_eq!(verdict.term_coverage, 0.0);
    }

    #[test]
    fn high_score_with_covered_terms_is_sufficient() {
        let results = vec![
            result(
                "a",
                0.82,
                "Noncompete clauses bind the employee after termination.",
            ),
            result("b", 0.41, "Termination notice periods."),
            result("c", 0.30, "Unrelated appendix."),
        ];

        let verdict = assess_sufficiency(&results, &plan("noncompete clauses"), &tuning(), true);

        assert!(verdict.sufficient, "reasons: {:?}", verdict.reasons);
        assert_eq!(verdict.top_score, 0.82);
        assert!((verdict.spread - 0.52).abs() < 1e-5);
        assert_eq!(verdict.term_coverage, 1.0);
    }

    #[test]
    fn confident_ranking_over_off_topic_passages_is_insufficient() {
        let results = vec![
            result("a", 0.91, "Quarterly revenue by region."),
            result("b", 0.55, "Headcount by department."),
            result("c", 0.40, "Office lease renewals."),
        ];

        let verdict = assess_sufficiency(
            &results,
            &plan("noncompete clauses garden leave"),
            &tuning(),
            true,
        );

        assert!(!verdict.sufficient);
        assert!(verdict.reasons.contains(&reason::LOW_TERM_COVERAGE));
        assert!(!verdict.reasons.contains(&reason::LOW_TOP_SCORE));
        assert_eq!(verdict.term_coverage, 0.0);
    }

    #[test]
    fn low_top_score_is_insufficient_only_when_a_reranker_ran() {
        let results = vec![
            result("a", 0.11, "Noncompete clauses bind the employee."),
            result("b", 0.09, "Noncompete review."),
            result("c", 0.07, "Clauses appendix."),
        ];

        let reranked = assess_sufficiency(&results, &plan("noncompete clauses"), &tuning(), true);
        assert!(!reranked.sufficient);
        assert!(reranked.reasons.contains(&reason::LOW_TOP_SCORE));

        // The same scores are RRF ranks when no cross-encoder ran, and ranks
        // are not probabilities. Coverage and count carry the verdict instead.
        let fused = assess_sufficiency(&results, &plan("noncompete clauses"), &tuning(), false);
        assert!(fused.sufficient, "reasons: {:?}", fused.reasons);
        assert!(fused.reasons.contains(&reason::NO_RERANKER));
    }

    #[test]
    fn a_thin_fused_shortlist_is_insufficient_without_a_reranker() {
        let results = vec![result(
            "a",
            0.02,
            "Noncompete clauses bind the employee after termination.",
        )];

        let verdict = assess_sufficiency(&results, &plan("noncompete clauses"), &tuning(), false);

        assert!(!verdict.sufficient);
        assert!(verdict.reasons.contains(&reason::TOO_FEW_RESULTS));
    }

    #[test]
    fn an_undifferentiated_reranker_falls_back_to_coverage_and_count() {
        let results: Vec<_> = (0..6)
            .map(|i| {
                result(
                    &format!("r{i}"),
                    0.2,
                    "Noncompete clauses bind the employee.",
                )
            })
            .collect();

        let verdict = assess_sufficiency(&results, &plan("noncompete clauses"), &tuning(), true);

        assert!(verdict.reasons.contains(&reason::FLAT_RERANK_SPREAD));
        assert!(!verdict.reasons.contains(&reason::LOW_TOP_SCORE));
        assert!(verdict.sufficient, "reasons: {:?}", verdict.reasons);
        assert_eq!(verdict.spread, 0.0);
    }

    #[test]
    fn ordered_reading_plans_are_never_corrected() {
        let results = vec![result("a", 0.0, "Front matter.")];
        let mut ordered = plan("introduction foundations overview");
        ordered.start_at_beginning = true;
        ordered.opening_document_ids = vec!["intro".to_string()];

        let verdict = assess_sufficiency(&results, &ordered, &tuning(), false);

        assert!(verdict.sufficient);
        assert_eq!(verdict.reasons, [reason::ORDERED_READING]);
    }

    #[test]
    fn a_plan_without_informative_terms_cannot_fail_coverage() {
        let results = vec![
            result("a", 0.9, "Unrelated text."),
            result("b", 0.5, "Also unrelated."),
            result("c", 0.4, "Still unrelated."),
        ];

        let verdict = assess_sufficiency(&results, &plan("706.07(a)"), &tuning(), true);

        assert_eq!(verdict.term_coverage, 1.0);
        assert!(verdict.sufficient, "reasons: {:?}", verdict.reasons);
    }
}
