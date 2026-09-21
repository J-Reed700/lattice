use super::repository::{DailyNotesRepository, WorkspaceNoteRecord};
use crate::features::vault::writeback;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::time::now_db_timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

fn to_json<T: Serialize>(value: &T) -> Result<String> {
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

fn row_to_dto(row: WorkspaceNoteRecord) -> Result<WorkspaceNoteDto> {
    Ok(WorkspaceNoteDto {
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
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn get_note_by_id(
    repository: &DailyNotesRepository,
    note_id: &str,
) -> Result<WorkspaceNoteDto> {
    row_to_dto(repository.get(note_id).await?)
}

pub async fn list_workspace_notes_impl(
    container: &Container,
    journal_id: Option<&str>,
) -> Result<Vec<WorkspaceNoteDto>> {
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let rows = match journal_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(journal_id) => repository.list_for_journal(journal_id).await?,
        None => repository.list().await?,
    };

    rows.into_iter().map(row_to_dto).collect()
}

pub async fn create_workspace_note_impl(
    container: &Container,
    request: CreateWorkspaceNoteRequestDto,
) -> Result<WorkspaceNoteDto> {
    let note_id = Uuid::new_v4().to_string();
    let title = request
        .title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Untitled note".to_string());
    let now = now_db_timestamp();
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    repository
        .insert(&WorkspaceNoteRecord {
            id: note_id.clone(),
            title,
            journal_id: request
                .journal_id
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty()),
            content: String::new(),
            linked_document_ids: "[]".to_string(),
            linked_conversation_ids: "[]".to_string(),
            highlights_json: "[]".to_string(),
            sticky_notes_json: "[]".to_string(),
            conversation_snapshots_json: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
        .await?;

    let note = get_note_by_id(&repository, &note_id).await?;
    // Vault writeback: fire-and-forget. Failures log but never block
    // the SQL commit; the database is still the canonical write.
    writeback::spawn_sync_workspace_note(
        container,
        note.id.clone(),
        note.title.clone(),
        note.content.clone(),
        note.created_at.clone(),
        note.updated_at.clone(),
        Vec::new(),
    );
    Ok(note)
}

pub async fn update_workspace_note_impl(
    container: &Container,
    note: WorkspaceNoteDto,
) -> Result<WorkspaceNoteDto> {
    let note_id = note.id.clone();
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    repository
        .update(&WorkspaceNoteRecord {
            id: note_id.clone(),
            title: note.title,
            // `update` does not write this column: a page cannot change owner by
            // being saved, and a stale client copy must not be able to reassign it.
            journal_id: note.journal_id,
            content: note.content,
            linked_document_ids: to_json(&note.linked_document_ids)?,
            linked_conversation_ids: to_json(&note.linked_conversation_ids)?,
            highlights_json: to_json(&note.highlights)?,
            sticky_notes_json: to_json(&note.sticky_notes)?,
            conversation_snapshots_json: to_json(&note.conversation_snapshots)?,
            created_at: note.created_at,
            updated_at: now_db_timestamp(),
        })
        .await?;

    let updated = get_note_by_id(&repository, &note_id).await?;
    writeback::spawn_sync_workspace_note(
        container,
        updated.id.clone(),
        updated.title.clone(),
        updated.content.clone(),
        updated.created_at.clone(),
        updated.updated_at.clone(),
        Vec::new(),
    );
    Ok(updated)
}

pub async fn delete_workspace_note_impl(
    container: &Container,
    request: DeleteWorkspaceNoteRequestDto,
) -> Result<()> {
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    repository.delete(&request.note_id).await?;

    // Remove the mirrored markdown too. Without this the file outlives its
    // row, and the next vault rescan sees a file with no matching row, treats
    // it as an external addition, and re-imports the note the user deleted.
    writeback::spawn_delete_workspace_note(container, request.note_id.clone());

    Ok(())
}

fn today_daily_title() -> String {
    let today = chrono::Local::now().date_naive();
    format!("Daily Notes · {}", today.format("%A, %B %-d"))
}

fn workspace_note_to_compat(note: WorkspaceNoteDto) -> DailyNoteCompatDto {
    DailyNoteCompatDto {
        id: note.id,
        date: note
            .updated_at
            .get(0..10)
            .unwrap_or("1970-01-01")
            .to_string(),
        content: note.content,
        created_at: note.created_at,
        updated_at: note.updated_at,
    }
}

async fn most_recent_workspace_note(
    repository: &DailyNotesRepository,
) -> Result<Option<WorkspaceNoteDto>> {
    repository.most_recent().await?.map(row_to_dto).transpose()
}

pub async fn get_today_note_impl(container: &Container) -> Result<DailyNoteCompatDto> {
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    if let Some(note) = most_recent_workspace_note(&repository).await? {
        return Ok(workspace_note_to_compat(note));
    }

    let created = create_workspace_note_impl(
        container,
        CreateWorkspaceNoteRequestDto {
            title: Some(today_daily_title()),
            // The daily note is not a journal's page.
            journal_id: None,
        },
    )
    .await?;
    Ok(workspace_note_to_compat(created))
}

/// The text a capture will append, or `InvalidInput` when there is nothing.
///
/// Pure, so the empty-input rule is testable without a `Container`.
pub(super) fn capture_snippet(content: &str) -> Result<&str> {
    let snippet = content.trim();
    if snippet.is_empty() {
        return Err(AppError::InvalidInput("Nothing to capture".to_string()));
    }
    Ok(snippet)
}

/// A capture appended to a page keeps one blank line between the old text and
/// the new; a capture onto an empty page is the page.
pub(super) fn appended_capture(existing: &str, snippet: &str) -> String {
    if existing.trim().is_empty() {
        snippet.to_string()
    } else {
        format!("{}\n\n{}", existing, snippet)
    }
}

/// Today's page in the journal captures belong to, created when it does not
/// exist yet.
///
/// Captures used to append to whichever page was written to last, which put a
/// thought captured today onto a page about something else entirely (a week
/// synthesis, say). Asking for today's page by name keeps the destination
/// predictable and nameable in the toast.
async fn resolve_capture_target(
    container: &Container,
    repository: &DailyNotesRepository,
) -> Result<(WorkspaceNoteDto, bool)> {
    let title = today_daily_title();
    let journal_id = repository.capture_journal_id().await?;
    if let Some(row) = repository
        .find_by_title(&title, journal_id.as_deref())
        .await?
    {
        return Ok((row_to_dto(row)?, false));
    }
    let created = create_workspace_note_impl(
        container,
        CreateWorkspaceNoteRequestDto {
            title: Some(title),
            journal_id,
        },
    )
    .await?;
    Ok((created, true))
}

pub async fn quick_capture_impl(
    container: &Container,
    content: String,
) -> Result<QuickCaptureResultDto> {
    let snippet = capture_snippet(&content)?.to_string();

    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let (mut note, created) = resolve_capture_target(container, &repository).await?;

    note.content = appended_capture(&note.content, &snippet);

    let note_id = note.id.clone();
    let note_title = note.title.clone();
    let _ = update_workspace_note_impl(container, note).await?;
    Ok(QuickCaptureResultDto {
        note_id,
        note_title,
        created,
    })
}

pub async fn get_daily_notes_range_impl(
    container: &Container,
    request: DailyNotesRangeRequestDto,
) -> Result<Vec<DailyNoteCompatDto>> {
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let rows = repository
        .list_in_date_range(&request.start_date, &request.end_date)
        .await?;

    rows.into_iter()
        .map(row_to_dto)
        .map(|note| note.map(workspace_note_to_compat))
        .collect()
}

pub async fn get_previous_daily_note_impl(
    container: &Container,
    request: DailyNoteCursorRequestDto,
) -> Result<Option<DailyNoteCompatDto>> {
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let row = repository.previous_before(&request.current_date).await?;

    row.map(row_to_dto)
        .transpose()
        .map(|note| note.map(workspace_note_to_compat))
}

pub async fn get_next_daily_note_impl(
    container: &Container,
    request: DailyNoteCursorRequestDto,
) -> Result<Option<DailyNoteCompatDto>> {
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let row = repository.next_after(&request.current_date).await?;

    row.map(row_to_dto)
        .transpose()
        .map(|note| note.map(workspace_note_to_compat))
}

pub async fn update_daily_note_content_impl(
    container: &Container,
    request: UpdateDailyNoteContentRequestDto,
) -> Result<()> {
    let note_id = request.note_id.clone();
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    repository
        .update_content(&note_id, &request.content, &now_db_timestamp())
        .await?;

    // Daily notes share the same `daily_notes_workspace` table as
    // workspace notes — backend-side they're the same entity. Reuse the
    // workspace writeback path so vault export is consistent.
    if let Ok(note) = get_note_by_id(&repository, &note_id).await {
        writeback::spawn_sync_workspace_note(
            container,
            note.id.clone(),
            note.title.clone(),
            note.content.clone(),
            note.created_at.clone(),
            note.updated_at.clone(),
            Vec::new(),
        );
    }
    Ok(())
}
