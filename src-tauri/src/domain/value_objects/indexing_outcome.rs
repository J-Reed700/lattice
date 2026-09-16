//! Indexing outcome value object.
//!
//! Represents the result of attempting to index a file, distinguishing between:
//! - Newly indexed files
//! - Already indexed files (skipped)
//! - Re-indexed files (content changed)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Outcome of a file indexing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IndexingOutcome {
    /// New document indexed successfully
    NewlyIndexed {
        document_id: String,
        chunks_created: usize,
    },

    /// File already indexed, content unchanged (skipped)
    AlreadyIndexed {
        document_id: String,
        #[serde(with = "chrono::serde::ts_seconds")]
        last_indexed: DateTime<Utc>,
    },

    /// File re-indexed due to content change
    ReIndexed {
        document_id: String,
        old_checksum: String,
        new_checksum: String,
        chunks_created: usize,
    },
}

impl IndexingOutcome {
    /// Get the document ID for any outcome
    pub fn document_id(&self) -> &str {
        match self {
            Self::NewlyIndexed { document_id, .. } => document_id,
            Self::AlreadyIndexed { document_id, .. } => document_id,
            Self::ReIndexed { document_id, .. } => document_id,
        }
    }

    /// Check if this represents a successful indexing operation
    pub fn is_success(&self) -> bool {
        matches!(self, Self::NewlyIndexed { .. } | Self::ReIndexed { .. })
    }

    /// Check if content was actually processed
    pub fn was_processed(&self) -> bool {
        !matches!(self, Self::AlreadyIndexed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_newly_indexed_outcome() {
        let outcome = IndexingOutcome::NewlyIndexed {
            document_id: "doc-123".to_string(),
            chunks_created: 10,
        };

        assert_eq!(outcome.document_id(), "doc-123");
        assert!(outcome.is_success());
        assert!(outcome.was_processed());
    }

    #[test]
    fn test_already_indexed_outcome() {
        let outcome = IndexingOutcome::AlreadyIndexed {
            document_id: "doc-456".to_string(),
            last_indexed: Utc::now(),
        };

        assert_eq!(outcome.document_id(), "doc-456");
        assert!(!outcome.is_success());
        assert!(!outcome.was_processed());
    }
}
