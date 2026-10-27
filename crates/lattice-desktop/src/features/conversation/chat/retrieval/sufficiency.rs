//! Post-rerank sufficiency signal for corrective retrieval: a cheap, local
//! second opinion, consulted once after reranking, on whether one replanned
//! retry is worth its latency. Three weak signals: the top blended score (only
//! when the cross-encoder ran; RRF ranks are never thresholded), the spread
//! between the first and fifth results, and how many of the plan's query
//! terms appear in the passages that will reach the answer.

use std::collections::HashSet;

use lattice_core::application::contracts::search::SearchResultDto;
use lattice_core::application::contracts::settings::RetrievalTuningSettingsDto;

use super::corpus_plan::CorpusSearchPlan;
use super::{select_informative_terms, tokenize_keyword_terms, tokenize_overlap_terms};

/// The window the top1-minus-topN spread is measured over.
pub(super) const COVERAGE_TOP_N: usize = 5;

/// The shortest shared stem that counts as one word in two forms. Shorter,
/// and a prefix is a coincidence of spelling rather than a common root.
const MIN_STEM_CHARS: usize = 4;

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
pub(in crate::features::conversation::chat) mod reason {
    pub const NO_RESULTS: &str = "no_results";
    pub const LOW_TOP_SCORE: &str = "low_top_score";
    pub const FLAT_RERANK_SPREAD: &str = "flat_rerank_spread";
    pub const LOW_TERM_COVERAGE: &str = "low_term_coverage";
    pub const TOO_FEW_RESULTS: &str = "too_few_results";
    pub const NO_RERANKER: &str = "no_reranker";
    pub const ORDERED_READING: &str = "ordered_reading";

    /// A code as the reader of a turn record meets it. The codes stay what
    /// tests and the trace match on; this is only what gets shown.
    pub fn readable(code: &str) -> &str {
        match code {
            NO_RESULTS => "nothing came back",
            LOW_TOP_SCORE => "nothing scored well",
            FLAT_RERANK_SPREAD => "no passage stood out from the rest",
            LOW_TERM_COVERAGE => "the passages missed most of the question's terms",
            TOO_FEW_RESULTS => "too few passages came back",
            NO_RERANKER => "no reranker to tell the passages apart",
            ORDERED_READING => "the question wants the document read in order",
            other => other,
        }
    }
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
/// they were relevance probabilities, which they are not. `evidence_depth` is
/// how many of `results` reach the answer; coverage reads exactly those.
pub(super) fn assess_sufficiency(
    results: &[SearchResultDto],
    plan: &CorpusSearchPlan,
    tuning: &RetrievalTuningSettingsDto,
    reranked: bool,
    evidence_depth: usize,
) -> SufficiencyVerdict {
    if results.is_empty() {
        return SufficiencyVerdict {
            sufficient: false,
            reasons: vec![reason::NO_RESULTS],
            ..Default::default()
        };
    }

    let (top_score, spread) = score_summary(results);
    let term_coverage = term_coverage(results, plan, evidence_depth);

    let mut reasons = Vec::new();
    // Reading in order is recorded, not obeyed: coverage judges this plan like
    // any other, and a corrective plan never reads in order, so a misread request
    // is still searched for what it asked.
    if plan.start_at_beginning {
        reasons.push(reason::ORDERED_READING);
    }
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

/// Structural evidence can be pinned ahead of score order. The sufficiency
/// signal still describes the best scores available, not the presentation
/// order chosen for the answer prompt.
fn score_summary(results: &[SearchResultDto]) -> (f32, f32) {
    let mut scores: Vec<f32> = results.iter().map(|result| result.score).collect();
    scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let top = scores.first().copied().unwrap_or(0.0);
    let tail = scores
        .get(COVERAGE_TOP_N - 1)
        .or_else(|| scores.last())
        .copied()
        .unwrap_or(0.0);
    (top, top - tail)
}

fn term_matches_set(term: &str, set: &HashSet<String>) -> bool {
    set.iter()
        .any(|candidate| terms_share_a_stem(term, candidate))
}

/// One word in two forms: equal, or one a prefix of the other with the shorter
/// at least `MIN_STEM_CHARS` long. That covers suffixing in any script
/// (`monitor`/`monitoring`, `vertrag`/`vertrages`, `célula`/`células`) and
/// rejects a shared prefix that is not a whole word (`contract`/`contrary`).
/// A stem that changes its own spelling (`policy`/`policies`) does not match;
/// that can only cost one corrective pass, never a wrong answer.
fn terms_share_a_stem(left: &str, right: &str) -> bool {
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    shorter == longer || (shorter.chars().count() >= MIN_STEM_CHARS && longer.starts_with(shorter))
}

/// Fraction of the plan's informative terms that appear in the first
/// `evidence_depth` passages, for the query that covers the most.
///
/// A plan with no informative terms (a bare acronym, a section number) cannot
/// fail coverage — there is nothing to miss, and reporting 0.0 would send every
/// such turn into a pointless retry.
fn term_coverage(
    results: &[SearchResultDto],
    plan: &CorpusSearchPlan,
    evidence_depth: usize,
) -> f32 {
    let passage_terms: HashSet<String> = results
        .iter()
        .take(evidence_depth)
        .flat_map(|result| {
            let section = result
                .metadata
                .get("section")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            tokenize_overlap_terms(&result.content)
                .into_iter()
                .chain(tokenize_overlap_terms(&result.title))
                .chain(tokenize_overlap_terms(section))
        })
        .collect();

    let mut best_coverage = None::<f32>;
    for query in &plan.queries {
        let terms = select_informative_terms(tokenize_keyword_terms(query), MAX_PLAN_TERMS);
        if terms.is_empty() {
            continue;
        }
        let hits = terms
            .iter()
            .filter(|term| term_matches_set(term.as_str(), &passage_terms))
            .count();
        let coverage = hits as f32 / terms.len() as f32;
        best_coverage = Some(best_coverage.map_or(coverage, |b| b.max(coverage)));
    }
    best_coverage.unwrap_or(1.0)
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
        let verdict = assess_sufficiency(&[], &plan("noncompete clauses"), &tuning(), true, 0);

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

        let verdict = assess_sufficiency(
            &results,
            &plan("noncompete clauses"),
            &tuning(),
            true,
            results.len(),
        );

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
            results.len(),
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

        let reranked = assess_sufficiency(
            &results,
            &plan("noncompete clauses"),
            &tuning(),
            true,
            results.len(),
        );
        assert!(!reranked.sufficient);
        assert!(reranked.reasons.contains(&reason::LOW_TOP_SCORE));

        // The same scores are RRF ranks when no cross-encoder ran, and ranks
        // are not probabilities. Coverage and count carry the verdict instead.
        let fused = assess_sufficiency(
            &results,
            &plan("noncompete clauses"),
            &tuning(),
            false,
            results.len(),
        );
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

        let verdict = assess_sufficiency(
            &results,
            &plan("noncompete clauses"),
            &tuning(),
            false,
            results.len(),
        );

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

        let verdict = assess_sufficiency(
            &results,
            &plan("noncompete clauses"),
            &tuning(),
            true,
            results.len(),
        );

        assert!(verdict.reasons.contains(&reason::FLAT_RERANK_SPREAD));
        assert!(!verdict.reasons.contains(&reason::LOW_TOP_SCORE));
        assert!(verdict.sufficient, "reasons: {:?}", verdict.reasons);
        assert_eq!(verdict.spread, 0.0);
    }

    #[test]
    fn pinned_structural_order_does_not_hide_the_best_reranker_score() {
        let results = vec![
            result("opening", 0.14, "Front matter."),
            result("section", 0.31, "Named section."),
            result("body", 0.96, "Body evidence answers the question."),
            result("tail", 0.18, "Appendix."),
            result("other", 0.15, "Other material."),
        ];

        let verdict = assess_sufficiency(
            &results,
            &plan("body evidence"),
            &tuning(),
            true,
            results.len(),
        );

        assert_eq!(verdict.top_score, 0.96);
        assert!((verdict.spread - 0.82).abs() < 1e-5);
        assert!(verdict.sufficient, "reasons: {:?}", verdict.reasons);
    }

    fn ordered(query: &str) -> CorpusSearchPlan {
        CorpusSearchPlan {
            opening_document_ids: vec!["doc".to_string()],
            start_at_beginning: true,
            ..plan(query)
        }
    }

    fn passages(contents: &[&str]) -> Vec<SearchResultDto> {
        contents
            .iter()
            .enumerate()
            .map(|(i, content)| result(&format!("r{i}"), 0.5, content))
            .collect()
    }

    /// Reading in order is recorded, never a verdict: openings that never
    /// mention what was asked send the turn to the corrective pass, whose plan
    /// searches instead of reading.
    #[test]
    fn an_ordered_reading_plan_that_misses_the_question_is_corrected() {
        for (query, openings) in [
            (
                "división celular mitosis",
                [
                    "Capítulo uno: la célula y sus orgánulos.",
                    "La membrana plasmática regula el transporte.",
                    "El núcleo guarda el material genético.",
                ],
            ),
            (
                "Lacrimosa Tonart",
                [
                    "Einleitung: die Entstehung des Requiems.",
                    "Mozart erhielt den Auftrag anonym.",
                    "Süßmayr vollendete die Partitur.",
                ],
            ),
        ] {
            let results = passages(&openings);

            let verdict =
                assess_sufficiency(&results, &ordered(query), &tuning(), false, results.len());

            assert!(!verdict.sufficient, "{query}: {:?}", verdict.reasons);
            assert!(verdict.reasons.contains(&reason::LOW_TERM_COVERAGE));
            assert!(verdict.reasons.contains(&reason::ORDERED_READING));
        }
    }

    #[test]
    fn an_ordered_reading_plan_that_covers_the_question_stands() {
        for (query, openings) in [
            (
                "división celular mitosis",
                [
                    "Capítulo uno: la célula y sus orgánulos.",
                    "La membrana plasmática regula el transporte.",
                    "La división celular por mitosis produce dos células hijas.",
                ],
            ),
            (
                "Lacrimosa Tonart",
                [
                    "Einleitung: die Entstehung des Requiems.",
                    "Mozart erhielt den Auftrag anonym.",
                    "Das Lacrimosa steht in d-Moll, der Tonart des ganzen Requiems.",
                ],
            ),
        ] {
            let results = passages(&openings);

            let verdict =
                assess_sufficiency(&results, &ordered(query), &tuning(), false, results.len());

            assert!(verdict.sufficient, "{query}: {:?}", verdict.reasons);
            assert!(verdict.reasons.contains(&reason::ORDERED_READING));
        }
    }

    #[test]
    fn a_plan_without_informative_terms_cannot_fail_coverage() {
        let results = vec![
            result("a", 0.9, "Unrelated text."),
            result("b", 0.5, "Also unrelated."),
            result("c", 0.4, "Still unrelated."),
        ];

        let verdict =
            assess_sufficiency(&results, &plan("706.07(a)"), &tuning(), true, results.len());

        assert_eq!(verdict.term_coverage, 1.0);
        assert!(verdict.sufficient, "reasons: {:?}", verdict.reasons);
    }

    #[test]
    fn coverage_matches_a_suffixed_form_in_any_script_but_not_a_shared_prefix() {
        let terms = HashSet::from([
            "monitoring".to_string(),
            "vertrages".to_string(),
            "células".to_string(),
            "policies".to_string(),
            "contrary".to_string(),
        ]);

        assert!(term_matches_set("monitor", &terms));
        assert!(term_matches_set("vertrag", &terms));
        assert!(term_matches_set("célula", &terms));
        assert!(!term_matches_set("contract", &terms));
        // A stem that changes its own spelling is a known miss: at worst one
        // corrective pass, never a wrong answer.
        assert!(!term_matches_set("policy", &terms));
    }

    #[test]
    fn coverage_reads_exactly_the_passages_that_reach_the_answer() {
        let mut results: Vec<_> = (0..5)
            .map(|i| {
                result(
                    &format!("r{i}"),
                    0.5,
                    "La fotosíntesis ocurre en el cloroplasto.",
                )
            })
            .collect();
        results.push(result(
            "r5",
            0.4,
            "La división celular por mitosis produce dos células hijas.",
        ));
        let question = plan("división celular mitosis");

        let shallow = assess_sufficiency(&results, &question, &tuning(), true, 3);
        let deep = assess_sufficiency(&results, &question, &tuning(), true, 6);

        assert_eq!(shallow.term_coverage, 0.0);
        assert!(shallow.reasons.contains(&reason::LOW_TERM_COVERAGE));
        assert_eq!(deep.term_coverage, 1.0);
        assert!(!deep.reasons.contains(&reason::LOW_TERM_COVERAGE));
    }
}
