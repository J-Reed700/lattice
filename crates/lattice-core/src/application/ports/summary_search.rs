//! First-stage, collection-level retrieval over summary vectors, and turning
//! its hits into a reading order.
//!
//! A summary can never be returned as a passage and quoted as evidence. What
//! it can do is name the *documents* worth reading, which is what "what is
//! this about" and "where do I start" need.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;

use crate::shared::error::Result;

/// Which slice of a document a summary describes.
///
/// Stored as the literal strings the migration's CHECK constraint allows, so a
/// typo is a database error rather than a silently unreadable row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SummaryLevel {
    /// The document as a whole.
    Document,
    /// One top-level section of the document.
    Section,
}

impl SummaryLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Section => "section",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "document" => Some(Self::Document),
            "section" => Some(Self::Section),
            _ => None,
        }
    }
}

/// A summary matched by similarity search.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryHit {
    pub document_id: String,
    pub summary_id: String,
    pub level: SummaryLevel,
    pub section: Option<String>,
    pub summary_text: String,
    pub score: f32,
}

/// Port for the summary tier, so callers can be tested against a stub.
#[async_trait]
pub trait SummarySearchPort: Send + Sync {
    /// Best summaries for `query`, restricted to `scope`, best first.
    async fn top_summaries(
        &self,
        query: &str,
        scope: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<SummaryHit>>;

    /// Document-level summary text for `scope`, keyed by document id.
    ///
    /// Only whole-document summaries: a planner catalog describes documents,
    /// and splicing a section summary in under a document's name would be
    /// telling the planner a chapter is the book.
    async fn document_summaries(&self, scope: &HashSet<String>) -> Result<HashMap<String, String>>;
}

/// Distinct documents behind `hits`, best-scoring first.
///
/// The planner's own `opening_document_ids` are a judgement made from titles
/// and first lines. When summaries exist they describe the whole document, so
/// for "where do I start" style requests they are the better evidence — but
/// only about *which* documents, never about how many or in what order the
/// chat is allowed to read them, which stays with the planner's validation.
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
