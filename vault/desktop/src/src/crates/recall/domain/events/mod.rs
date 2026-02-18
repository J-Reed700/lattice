//! Domain Events
//!
//! Domain events represent significant state changes in the domain model that
//! other parts of the system may need to react to.
//!
//! # Architecture Decision
//!
//! Using enum instead of trait for domain events because:
//! - Idiomatic Rust pattern for event sourcing
//! - Compile-time exhaustiveness checking
//! - Zero runtime cost (no vtables/downcasting)
//! - Allows pattern matching in event handlers

pub mod model_download_events;

use serde::{Deserialize, Serialize};

/// Domain event enum representing all possible events in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    DocumentIndexed(DocumentIndexedEvent),
    DocumentDeleted(DocumentDeletedEvent),
    TagAdded(TagAddedEvent),
}

impl DomainEvent {
    /// Event type identifier for debugging/logging
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::DocumentIndexed(_) => "document_indexed",
            Self::DocumentDeleted(_) => "document_deleted",
            Self::TagAdded(_) => "tag_added",
        }
    }

    /// Extract aggregate ID (document ID) from any event
    pub fn aggregate_id(&self) -> &str {
        match self {
            Self::DocumentIndexed(e) => &e.document_id,
            Self::DocumentDeleted(e) => &e.document_id,
            Self::TagAdded(e) => &e.document_id,
        }
    }
}

/// Event fired when a document is successfully indexed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentIndexedEvent {
    pub document_id: String,
    pub chunks_count: usize,
    pub timestamp: i64,
}

/// Event fired when a document is deleted
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDeletedEvent {
    pub document_id: String,
    pub timestamp: i64,
}

/// Event fired when a tag is added to a document
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagAddedEvent {
    pub document_id: String,
    pub tag_id: String,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization_camelcase() {
        let indexed_event = DocumentIndexedEvent {
            document_id: "doc-123".to_string(),
            chunks_count: 42,
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&indexed_event).unwrap();
        assert!(json.contains("\"documentId\":"));
        assert!(json.contains("\"chunksCount\":"));
        assert!(!json.contains("document_id"));
        assert!(!json.contains("chunks_count"));

        let deleted_event = DocumentDeletedEvent {
            document_id: "doc-456".to_string(),
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&deleted_event).unwrap();
        assert!(json.contains("\"documentId\":"));
        assert!(!json.contains("document_id"));

        let tag_event = TagAddedEvent {
            document_id: "doc-789".to_string(),
            tag_id: "tag-123".to_string(),
            timestamp: 1234567890,
        };
        let json = serde_json::to_string(&tag_event).unwrap();
        assert!(json.contains("\"documentId\":"));
        assert!(json.contains("\"tagId\":"));
        assert!(!json.contains("document_id"));
        assert!(!json.contains("tag_id"));
    }
}
