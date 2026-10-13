//! Tests for recency boosting of fused search results.

use super::recency::boost_recent;
use super::*;

fn fused(id: &str, score: f32, updated_at: Option<&str>) -> SearchResultDto {
    SearchResultDto {
        id: id.into(),
        title: id.into(),
        content: String::new(),
        score,
        path: None,
        document_id: Some(id.into()),
        position: None,
        vector_score: None,
        bm25_score: None,
        vector_rank: None,
        bm25_rank: None,
        metadata: updated_at
            .map(|at| {
                std::collections::HashMap::from([("updated_at".to_owned(), serde_json::json!(at))])
            })
            .unwrap_or_default(),
    }
}

#[test]
fn a_recently_updated_document_is_lifted_above_an_older_one() {
    let today = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut results = vec![
        fused("old", 0.050, Some("2001-01-01 00:00:00")),
        fused("new", 0.049, Some(&today)),
    ];

    boost_recent(&mut results, 0.5, 730);

    assert_eq!(results[0].id, "new");
}

#[test]
fn a_zero_recency_weight_keeps_the_fused_order_and_scores() {
    let today = chrono::Utc::now().to_rfc3339();
    let mut results = vec![fused("old", 0.050, None), fused("new", 0.049, Some(&today))];

    boost_recent(&mut results, 0.0, 730);

    assert_eq!(results[0].id, "old");
    assert_eq!(results[1].score, 0.049);
}
