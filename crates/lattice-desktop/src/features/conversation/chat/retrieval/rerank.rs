use std::collections::HashSet;

use crate::features::conversation::chat::ports::ChatRetrieval;
use lattice_core::application::contracts::search::{
    RerankOptions, Reranked, SearchResponseDto, SearchResultDto,
};
use lattice_core::application::contracts::settings::RetrievalTuningSettingsDto;

/// Rerank the shortlist through the search orchestrator's cross-encoder
/// stage, reporting whether cross-encoder scores were actually applied.
///
/// The caller needs that second value, not just the reordered list: a
/// cross-encoder relevance can be thresholded, but the RRF weights left behind
/// when the reranker is unavailable or fails are ranks, and thresholding ranks
/// would invent confidence the pipeline does not have.
pub(super) async fn apply_rerank_stage(
    container: &dyn ChatRetrieval,
    validated_message: &str,
    interpretation: &lattice_core::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> (SearchResponseDto, bool) {
    let options = RerankOptions {
        query: build_rerank_query(validated_message, interpretation, followup_anchor_terms),
        max_candidates: tuning.rerank_max_candidates as usize,
        query_max_chars: tuning.rerank_query_max_chars as usize,
    };
    let original_response = search_response.clone();
    let reranked = container
        .library_search()
        .rerank(search_response, &options)
        .await;
    finalize_rerank(original_response, reranked)
}

/// Keep structural passages at the front without exempting them from scoring.
///
/// An unapplied rerank may contain only fused scores, so the exact original
/// response wins in that case. Once the cross-encoder has scored the full
/// response, only the leading structural passages are moved back to their
/// original leading order; every score remains the one produced by reranking.
fn finalize_rerank(
    original_response: SearchResponseDto,
    mut reranked: Reranked,
) -> (SearchResponseDto, bool) {
    if !reranked.applied {
        return (original_response, false);
    }

    reranked.response.results =
        restore_leading_structural_results(&original_response.results, reranked.response.results);
    reranked.response.total = reranked.response.results.len();
    (reranked.response, true)
}

fn restore_leading_structural_results(
    original_results: &[SearchResultDto],
    mut reranked_results: Vec<SearchResultDto>,
) -> Vec<SearchResultDto> {
    let leading_ids = original_results
        .iter()
        .take_while(|result| is_structural_result(result))
        .map(|result| result.id.as_str());
    let mut leading = Vec::new();

    for id in leading_ids {
        if let Some(index) = reranked_results.iter().position(|result| result.id == id) {
            leading.push(reranked_results.remove(index));
        }
    }

    leading.extend(reranked_results);
    leading
}

pub(super) fn is_structural_result(result: &SearchResultDto) -> bool {
    matches!(
        result
            .metadata
            .get("retrievalMethod")
            .and_then(|value| value.as_str()),
        Some("section_lookup") | Some("document_opening")
    )
}

/// What the cross-encoder judges passages against: the message, the HyDE
/// passage when there is one, and up to ten follow-up anchor terms.
fn build_rerank_query(
    validated_message: &str,
    interpretation: &lattice_core::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
) -> String {
    let mut query = validated_message.trim().to_string();

    if let Some(hyde_text) = interpretation.hyde_text.as_deref() {
        if !hyde_text.trim().is_empty() {
            query = format!("{}\n{}", query, hyde_text.trim());
        }
    }

    if let Some(anchors) = followup_anchor_terms {
        if !anchors.is_empty() {
            let mut ordered_anchors: Vec<String> = anchors.iter().cloned().collect();
            ordered_anchors.sort();
            let anchor_text = ordered_anchors
                .into_iter()
                .take(10)
                .collect::<Vec<_>>()
                .join(" ");
            if !anchor_text.is_empty() {
                query = format!("{}\n{}", query, anchor_text);
            }
        }
    }

    query
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn result(id: &str, score: f32, retrieval_method: Option<&str>) -> SearchResultDto {
        let mut metadata = HashMap::new();
        if let Some(method) = retrieval_method {
            metadata.insert("retrievalMethod".to_string(), serde_json::json!(method));
        }
        SearchResultDto {
            id: id.to_string(),
            title: id.to_string(),
            content: format!("{id} content"),
            score,
            path: None,
            document_id: None,
            position: None,
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata,
        }
    }

    fn response(results: Vec<SearchResultDto>) -> SearchResponseDto {
        SearchResponseDto {
            total: results.len(),
            results,
            query_time_ms: 0,
        }
    }

    fn ids(results: &[SearchResultDto]) -> Vec<&str> {
        results.iter().map(|result| result.id.as_str()).collect()
    }

    #[test]
    fn applied_rerank_restores_the_original_leading_structural_order() {
        let original = response(vec![
            result("opening", 0.03, Some("document_opening")),
            result("section", 0.02, Some("section_lookup")),
            result("body", 0.01, None),
            result("tail", 0.005, None),
        ]);
        let reranked = Reranked {
            response: response(vec![
                result("body", 0.96, None),
                result("section", 0.31, Some("section_lookup")),
                result("tail", 0.18, None),
                result("opening", 0.14, Some("document_opening")),
            ]),
            applied: true,
        };

        let (restored, applied) = finalize_rerank(original, reranked);

        assert!(applied);
        assert_eq!(
            ids(&restored.results),
            ["opening", "section", "body", "tail"]
        );
    }

    #[test]
    fn structural_passages_keep_their_actual_scores_without_a_floor() {
        let original = response(vec![
            result("opening", 0.03, Some("document_opening")),
            result("section", 0.02, Some("section_lookup")),
            result("body", 0.01, None),
        ]);
        let reranked = Reranked {
            response: response(vec![
                result("body", 0.96, None),
                result("section", 0.31, Some("section_lookup")),
                result("opening", 0.14, Some("document_opening")),
            ]),
            applied: true,
        };

        let (restored, _) = finalize_rerank(original, reranked);

        assert_eq!(restored.results[0].score, 0.14);
        assert_eq!(restored.results[1].score, 0.31);
        assert!(restored.results[..2]
            .iter()
            .all(|result| result.score < 0.85));
    }

    /// Ordered reading leads the pool with at most `share` openings, the
    /// planner's half of the even split. Only that leading run is pinned:
    /// every result behind it, an opening among them, keeps the
    /// cross-encoder's order.
    #[test]
    fn only_the_leading_openings_stay_pinned_ahead_of_the_reranked_results() {
        let cases: [(&[&str], &[&str], &str); 2] = [
            (
                &[
                    "有機化学ノート 第1章",
                    "有機化学ノート 第2章",
                    "有機化学ノート 第3章",
                ],
                &[
                    "酸化と還元",
                    "電気陰性度",
                    "共有結合",
                    "イオン結合",
                    "水素結合",
                ],
                "無機化学ノート 第1章",
            ),
            (
                &[
                    "Requiem programme notes: Introitus",
                    "Requiem programme notes: Kyrie",
                    "Requiem programme notes: Sequentia",
                ],
                &[
                    "Lacrimosa",
                    "Süssmayr's completion",
                    "Dies irae",
                    "Tuba mirum",
                    "Rex tremendae",
                ],
                "Requiem programme notes: Offertorium",
            ),
        ];

        for (openings, searches, late_opening) in cases {
            let method = |id: &str| {
                (openings.contains(&id) || id == late_opening).then_some("document_opening")
            };
            let pool = |ids: &[&str]| {
                response(
                    ids.iter()
                        .enumerate()
                        .map(|(rank, id)| result(id, 1.0 / (rank as f32 + 1.0), method(id)))
                        .collect(),
                )
            };
            let original: Vec<&str> = openings
                .iter()
                .chain(searches)
                .copied()
                .chain([late_opening])
                .collect();
            let share = original.len().div_ceil(2);
            let leading = original.iter().take_while(|id| method(id).is_some());
            assert!(leading.count() <= share);

            // The cross-encoder reverses the search results, ranks the late
            // opening second and the leading openings last.
            let reversed: Vec<&str> = searches.iter().rev().copied().collect();
            let (best, others) = reversed.split_first().unwrap();
            let ranked: Vec<&str> = [*best, late_opening]
                .into_iter()
                .chain(others.iter().copied())
                .collect();
            let reranked: Vec<&str> = ranked
                .iter()
                .copied()
                .chain(openings.iter().rev().copied())
                .collect();

            let (restored, applied) = finalize_rerank(
                pool(&original),
                Reranked {
                    response: pool(&reranked),
                    applied: true,
                },
            );

            assert!(applied);
            let expected: Vec<&str> = openings.iter().copied().chain(ranked).collect();
            assert_eq!(ids(&restored.results), expected);
        }
    }

    #[test]
    fn unapplied_rerank_preserves_the_original_response_order_and_scores() {
        let original = response(vec![
            result("opening", 0.03, Some("document_opening")),
            result("body", 0.01, None),
        ]);
        let reranked = Reranked {
            response: response(vec![
                result("body", 0.99, None),
                result("opening", 0.98, Some("document_opening")),
            ]),
            applied: false,
        };

        let (restored, applied) = finalize_rerank(original, reranked);

        assert!(!applied);
        assert_eq!(ids(&restored.results), ["opening", "body"]);
        assert_eq!(
            restored
                .results
                .iter()
                .map(|result| result.score)
                .collect::<Vec<_>>(),
            [0.03, 0.01]
        );
    }
}
