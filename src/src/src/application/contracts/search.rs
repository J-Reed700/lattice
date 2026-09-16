//! Search records exchanged through application ports.

use crate::domain::entities::search_result::SearchResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultRecord {
    pub doc_id: String,
    pub chunk_id: String,
    pub score: f32,
    pub content: String,
}

impl From<SearchResultRecord> for SearchResult {
    fn from(record: SearchResultRecord) -> Self {
        let document_id = (!record.chunk_id.is_empty()).then(|| record.doc_id.clone());
        let id = if record.chunk_id.is_empty() {
            record.doc_id
        } else {
            record.chunk_id
        };
        // Match the search mapper's fallback policy and uphold domain score invariants.
        let score = if record.score.is_finite() {
            record.score
        } else {
            0.0
        };
        Self::with_metadata(id, score, Some(record.content), document_id, None, None)
            .unwrap_or_else(|_| Self::default_invalid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_preserves_document_and_chunk_identity() {
        for chunk_id in ["", "chunk-1"] {
            let result = SearchResult::from(SearchResultRecord {
                doc_id: "doc-1".into(),
                chunk_id: chunk_id.into(),
                score: 1.5,
                content: "text".into(),
            });
            assert_eq!(
                result.id(),
                if chunk_id.is_empty() {
                    "doc-1"
                } else {
                    chunk_id
                }
            );
            assert_eq!(
                result.document_id(),
                if chunk_id.is_empty() {
                    None
                } else {
                    Some("doc-1")
                }
            );
            assert_eq!(result.score(), 1.0);
            assert_eq!(result.snippet(), Some("text"));
        }
    }

    #[test]
    fn conversion_does_not_admit_nonfinite_scores() {
        for score in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let result = SearchResult::from(SearchResultRecord {
                doc_id: "doc-1".into(),
                chunk_id: String::new(),
                score,
                content: String::new(),
            });
            assert_eq!(result.score(), 0.0);
        }
    }
}
