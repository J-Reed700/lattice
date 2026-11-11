//! Tests for rank fusion.

use super::*;

#[test]
fn test_rerank_result_ordering() {
    // Sanity: sort descending by score.
    let mut results = [
        RerankResult {
            index: 0,
            score: 0.5,
        },
        RerankResult {
            index: 1,
            score: 0.9,
        },
        RerankResult {
            index: 2,
            score: 0.3,
        },
    ];
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assert_eq!(results[0].index, 1);
    assert_eq!(results[1].index, 0);
    assert_eq!(results[2].index, 2);
}

fn scored(scores: &[f32]) -> Vec<RerankResult> {
    scores
        .iter()
        .enumerate()
        .map(|(index, score)| RerankResult {
            index,
            score: *score,
        })
        .collect()
}

fn order(fused: &[FusedCandidate]) -> Vec<usize> {
    fused.iter().map(|candidate| candidate.index).collect()
}

#[test]
fn fusion_maps_relevance_back_to_original_candidates() {
    let reranked = vec![
        RerankResult {
            index: 1,
            score: 0.9,
        },
        RerankResult {
            index: 0,
            score: 0.2,
        },
        RerankResult {
            index: 2,
            score: 0.4,
        },
    ];

    let fused = fuse_rerank_order(3, &reranked).unwrap();

    // 1/61 + 1/63 for the first, 1/62 + 1/61 for the second: the second
    // leads, and each keeps its own relevance.
    assert_eq!(order(&fused), [1, 0, 2]);
    assert_eq!(fused[0].relevance, 0.9);
    assert_eq!(fused[1].relevance, 0.2);
}

/// A passage the cross-encoder ranks first climbs only as far as the
/// fusion lets it: it does not leap a head the first stage agreed on.
#[test]
fn fusion_weighs_both_orders_by_rank_at_k_sixty() {
    let mut scores = vec![0.5_f32; 30];
    scores[29] = 1.0;
    let fused = fuse_rerank_order(30, &scored(&scores)).unwrap();

    // The last candidate's 1/90 + 1/61 = 0.02750 falls between the twelfth
    // (1/72 + 1/73 = 0.02759) and the thirteenth (1/73 + 1/74 = 0.02721).
    let place = order(&fused).iter().position(|&index| index == 29);
    assert_eq!(place, Some(12));
    assert_eq!(order(&fused)[0], 0);
}

#[test]
fn fusion_ties_keep_the_fused_order_and_scores_are_clamped() {
    // First by one order and second by the other is a tie.
    let swapped = fuse_rerank_order(2, &scored(&[0.0, 1.0])).unwrap();
    assert_eq!(order(&swapped), [0, 1]);

    let fused = fuse_rerank_order(3, &scored(&[1.4, 0.5, -0.3])).unwrap();
    assert_eq!(order(&fused), [0, 1, 2]);
    let relevance: Vec<f32> = fused.iter().map(|candidate| candidate.relevance).collect();
    assert_eq!(relevance, [1.0, 0.5, 0.0]);
}

#[test]
fn fusion_of_nothing_is_nothing() {
    assert!(fuse_rerank_order(0, &[]).unwrap().is_empty());
}

#[test]
fn fusion_rejects_duplicate_indices() {
    let reranked = vec![
        RerankResult {
            index: 0,
            score: 0.9,
        },
        RerankResult {
            index: 0,
            score: 0.2,
        },
    ];

    assert!(fuse_rerank_order(2, &reranked).is_err());
}

#[test]
fn fusion_rejects_incomplete_out_of_range_or_non_finite_output() {
    let incomplete = vec![RerankResult {
        index: 0,
        score: 0.9,
    }];
    assert!(fuse_rerank_order(2, &incomplete).is_err());

    let out_of_range = vec![RerankResult {
        index: 1,
        score: 0.9,
    }];
    assert!(fuse_rerank_order(1, &out_of_range).is_err());

    let non_finite = vec![RerankResult {
        index: 0,
        score: f32::NAN,
    }];
    assert!(fuse_rerank_order(1, &non_finite).is_err());
}
