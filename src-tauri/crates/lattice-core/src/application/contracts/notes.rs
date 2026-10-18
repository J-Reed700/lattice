//! Journal pages as the frontend, chat and learning exchange them, and the
//! `daily_notes_workspace` rows they are stored as.

use serde::{Deserialize, Serialize};

use super::sources::SourceDto;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteHighlightDto {
    pub id: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StickyItemDto {
    pub id: String,
    pub text: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMessageDto {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[cfg(test)]
mod snapshot_contract_tests {
    use super::SnapshotMessageDto;

    #[test]
    fn preserves_citation_metadata_and_reads_older_snapshots() {
        let mut value = serde_json::json!({
            "id": "message-1", "role": "assistant", "content": "See [1]",
            "createdAt": "2026-09-15T00:00:00Z"
        });
        let older: SnapshotMessageDto = serde_json::from_value(value.clone()).unwrap();
        assert!(older.metadata.is_none());
        value["metadata"] =
            serde_json::Value::String(r#"{"sources":[{"documentId":"document-1"}]}"#.into());
        let snapshot: SnapshotMessageDto = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSnapshotDto {
    pub id: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub captured_at: String,
    pub message_count: usize,
    pub messages: Vec<SnapshotMessageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceNoteDto {
    /// Revision of the persisted snapshot; full edits must match it.
    pub revision: i64,
    pub id: String,
    pub title: String,
    /// Owning journal, or `None` for an unfiled page.
    pub journal_id: Option<String>,
    pub content: String,
    pub linked_document_ids: Vec<String>,
    pub linked_conversation_ids: Vec<String>,
    pub highlights: Vec<NoteHighlightDto>,
    pub sticky_notes: Vec<StickyItemDto>,
    pub conversation_snapshots: Vec<ConversationSnapshotDto>,
    /// Source metadata backing inline citations in this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceDto>,
    pub created_at: String,
    pub updated_at: String,
}

/// One `daily_notes_workspace` row as stored: list columns are JSON text.
/// No `sqlx::FromRow` here, because core does not depend on the database
/// driver; the repository decodes rows with `try_map`.
#[derive(Debug, Clone)]
pub struct WorkspaceNoteRecord {
    pub revision: i64,
    pub id: String,
    pub title: String,
    /// Owning journal, or `None` for an unfiled page (quick capture, vault import).
    pub journal_id: Option<String>,
    pub content: String,
    pub linked_document_ids: String,
    pub linked_conversation_ids: String,
    pub highlights_json: String,
    pub sticky_notes_json: String,
    pub conversation_snapshots_json: String,
    pub sources_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The fields of a note that round-trip through a vault markdown file.
#[derive(Debug, Clone)]
pub struct VaultNoteUpsert<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub content: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

/// A note's identity and timestamps, for vault rescans.
#[derive(Debug, Clone)]
pub struct NoteTimestampRecord {
    pub id: String,
    pub updated_at: String,
    pub created_at: String,
}
