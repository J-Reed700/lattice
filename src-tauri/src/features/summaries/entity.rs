//! Summary values: what a summary is, and how it is keyed in the vector index.

use chrono::{DateTime, Utc};

use lattice_core::application::ports::summary_search::SummaryLevel;

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
