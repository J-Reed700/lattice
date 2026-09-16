//! Turning summary hits into a reading order.
//!
//! The planner's own `opening_document_ids` are a judgement made from titles
//! and first lines. When summaries exist they describe the whole document, so
//! for "where do I start" style requests they are the better evidence — but
//! only about *which* documents, never about how many or in what order the
//! chat is allowed to read them, which stays with the planner's validation.

use std::collections::HashSet;

use crate::features::summaries::entity::SummaryHit;

/// Distinct documents behind `hits`, best-scoring first.
///
/// `hits` is expected best-first; a document's best hit fixes its position, so
/// a document with three good sections does not outrank one with a single
/// better whole-document match.
pub fn select_opening_documents(
    hits: &[SummaryHit],
    allowed: &HashSet<String>,
    limit: usize,
) -> Vec<String> {
    let mut seen = HashSet::new();
    hits.iter()
        .filter(|hit| allowed.contains(&hit.document_id))
        .filter(|hit| seen.insert(hit.document_id.clone()))
        .take(limit)
        .map(|hit| hit.document_id.clone())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::features::summaries::entity::SummaryLevel;

    fn hit(document_id: &str, score: f32) -> SummaryHit {
        SummaryHit {
            document_id: document_id.into(),
            summary_id: format!("{document_id}-{score}"),
            level: SummaryLevel::Document,
            section: None,
            summary_text: "text".into(),
            score,
        }
    }

    #[test]
    fn keeps_first_appearance_per_document_and_honours_scope_and_limit() {
        let hits = vec![
            hit("a", 0.9),
            hit("b", 0.8),
            hit("a", 0.7),
            hit("gone", 0.6),
            hit("c", 0.5),
        ];
        let allowed: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            select_opening_documents(&hits, &allowed, 4),
            vec!["a", "b", "c"]
        );
        assert_eq!(select_opening_documents(&hits, &allowed, 2), vec!["a", "b"]);
        assert!(select_opening_documents(&hits, &HashSet::new(), 4).is_empty());
    }
}
