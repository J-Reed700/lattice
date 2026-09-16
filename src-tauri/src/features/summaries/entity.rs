//! Summary values: what a summary is, and how it is keyed in the vector index.

use chrono::{DateTime, Utc};

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

/// One generated summary, as persisted and as embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSummary {
    pub id: String,
    pub document_id: String,
    /// The heading this summary covers. Always `None` at document level.
    pub section: Option<String>,
    pub level: SummaryLevel,
    pub summary_text: String,
    /// Embedding model identity the summary vector was produced under.
    pub model_identity: String,
    pub created_at: DateTime<Utc>,
}

impl DocumentSummary {
    pub fn new(
        document_id: impl Into<String>,
        section: Option<String>,
        level: SummaryLevel,
        summary_text: impl Into<String>,
        model_identity: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: document_id.into(),
            section: section.filter(|s| !s.trim().is_empty()),
            level,
            summary_text: summary_text.into(),
            model_identity: model_identity.into(),
            created_at: Utc::now(),
        }
    }

    /// Key in the summary vector index.
    ///
    /// Prefixed so a summary key can never collide with the `emb_<chunk id>`
    /// keys of the chunk index even if the two files are ever confused for one
    /// another; the two indexes are separate files regardless.
    pub fn vector_id(&self) -> String {
        vector_id(&self.id)
    }
}

/// Vector-index key for a summary id. See [`DocumentSummary::vector_id`].
pub fn vector_id(summary_id: &str) -> String {
    format!("summary_{summary_id}")
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
