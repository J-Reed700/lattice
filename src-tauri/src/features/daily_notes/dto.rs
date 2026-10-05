use super::repository::WorkspaceNoteRecord;
use crate::features::qa::dto::SourceDto;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceNoteRequestDto {
    pub title: Option<String>,
    /// The journal this page belongs to. Omitted for an unfiled page.
    pub journal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorkspaceNoteRequestDto {
    pub note_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DailyNoteCompatDto {
    pub id: String,
    pub date: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DailyNotesRangeRequestDto {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DailyNoteCursorRequestDto {
    pub current_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDailyNoteContentRequestDto {
    pub note_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspaceNotesResponseDto {
    pub notes: Vec<WorkspaceNoteDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspaceNotesRequestDto {
    /// List only the pages this journal owns. Omitted lists every page, which
    /// is what the cross-journal surfaces (reference inbox, weekly synthesis,
    /// vault sync) want.
    pub journal_id: Option<String>,
}

/// Where a quick capture landed, so the UI can name the destination
/// instead of saying "saved" and leaving the user to guess.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuickCaptureResultDto {
    pub note_id: String,
    pub note_title: String,
    /// True when today's page had to be created for this capture.
    pub created: bool,
}

pub(super) fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|e| AppError::Serialization(format!("Failed to serialize note field: {}", e)))
}

fn from_json<T>(value: &str, field_name: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if value.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(value).map_err(|e| {
        AppError::Deserialization(format!(
            "Failed to deserialize {} for workspace note: {}",
            field_name, e
        ))
    })
}

pub(super) fn row_to_dto(row: WorkspaceNoteRecord) -> Result<WorkspaceNoteDto> {
    Ok(WorkspaceNoteDto {
        revision: row.revision,
        id: row.id,
        title: row.title,
        journal_id: row.journal_id,
        content: row.content,
        linked_document_ids: from_json(&row.linked_document_ids, "linked_document_ids")?,
        linked_conversation_ids: from_json(
            &row.linked_conversation_ids,
            "linked_conversation_ids",
        )?,
        highlights: from_json(&row.highlights_json, "highlights_json")?,
        sticky_notes: from_json(&row.sticky_notes_json, "sticky_notes_json")?,
        conversation_snapshots: from_json(
            &row.conversation_snapshots_json,
            "conversation_snapshots_json",
        )?,
        sources: from_json(&row.sources_json, "sources_json")?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReferenceRequestDto {
    pub conversation_id: String,
    pub conversation_title: String,
    pub message_id: String,
    pub message_role: String,
    pub message_content: String,
    pub document_ids: Vec<String>,
    pub captured_at: String,
    /// Display title based on the user's local date.
    pub inbox_title: String,
    pub preferred_note_id: Option<String>,
    pub add_snapshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReferenceResultDto {
    pub note_id: String,
    pub note_title: String,
    pub linked_document_count: usize,
    pub snapshot_id: Option<String>,
}

pub(super) fn dto_to_row(note: &WorkspaceNoteDto) -> Result<WorkspaceNoteRecord> {
    Ok(WorkspaceNoteRecord {
        id: note.id.clone(),
        revision: note.revision,
        title: note.title.clone(),
        journal_id: note.journal_id.clone(),
        content: note.content.clone(),
        linked_document_ids: to_json(&note.linked_document_ids)?,
        linked_conversation_ids: to_json(&note.linked_conversation_ids)?,
        highlights_json: to_json(&note.highlights)?,
        sticky_notes_json: to_json(&note.sticky_notes)?,
        conversation_snapshots_json: to_json(&note.conversation_snapshots)?,
        sources_json: to_json(&note.sources)?,
        created_at: note.created_at.clone(),
        updated_at: note.updated_at.clone(),
    })
}
