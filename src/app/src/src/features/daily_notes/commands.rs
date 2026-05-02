use crate::features::vault::writeback;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
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

#[derive(Debug, FromRow)]
struct WorkspaceNoteRow {
    id: String,
    title: String,
    content: String,
    linked_document_ids: String,
    linked_conversation_ids: String,
    highlights_json: String,
    sticky_notes_json: String,
    conversation_snapshots_json: String,
    created_at: String,
    updated_at: String,
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

fn row_to_dto(row: WorkspaceNoteRow) -> Result<WorkspaceNoteDto> {
    Ok(WorkspaceNoteDto {
        id: row.id,
        title: row.title,
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

async fn get_note_by_id(pool: &SqlitePool, note_id: &str) -> Result<WorkspaceNoteDto> {
    let row = sqlx::query_as::<_, WorkspaceNoteRow>(
        r#"
        SELECT
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        FROM daily_notes_workspace
        WHERE id = ?
        "#,
    )
    .bind(note_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        AppError::Database(format!("Failed to fetch workspace note {}: {}", note_id, e))
    })?;

    row_to_dto(row)
}

pub async fn list_workspace_notes_impl(container: &Container) -> Result<Vec<WorkspaceNoteDto>> {
    let rows = sqlx::query_as::<_, WorkspaceNoteRow>(
        r#"
        SELECT
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        FROM daily_notes_workspace
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to list workspace notes: {}", e)))?;

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
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO daily_notes_workspace (
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        )
        VALUES (?, ?, '', '[]', '[]', '[]', '[]', '[]', ?, ?)
        "#,
    )
    .bind(&note_id)
    .bind(title)
    .bind(&now)
    .bind(&now)
    .execute(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to create workspace note: {}", e)))?;

    let note = get_note_by_id(container.db_pool(), &note_id).await?;
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
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE daily_notes_workspace
        SET
            title = ?,
            content = ?,
            linked_document_ids = ?,
            linked_conversation_ids = ?,
            highlights_json = ?,
            sticky_notes_json = ?,
            conversation_snapshots_json = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(note.title)
    .bind(note.content)
    .bind(to_json(&note.linked_document_ids)?)
    .bind(to_json(&note.linked_conversation_ids)?)
    .bind(to_json(&note.highlights)?)
    .bind(to_json(&note.sticky_notes)?)
    .bind(to_json(&note.conversation_snapshots)?)
    .bind(&now)
    .bind(&note_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to update workspace note {}: {}",
            note_id, e
        ))
    })?;

    let updated = get_note_by_id(container.db_pool(), &note_id).await?;
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
    sqlx::query("DELETE FROM daily_notes_workspace WHERE id = ?")
        .bind(&request.note_id)
        .execute(container.db_pool())
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to delete workspace note {}: {}",
                request.note_id, e
            ))
        })?;
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

async fn most_recent_workspace_note(pool: &SqlitePool) -> Result<Option<WorkspaceNoteDto>> {
    let row = sqlx::query_as::<_, WorkspaceNoteRow>(
        r#"
        SELECT
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        FROM daily_notes_workspace
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch latest workspace note: {}", e)))?;

    row.map(row_to_dto).transpose()
}

pub async fn get_today_note_impl(container: &Container) -> Result<DailyNoteCompatDto> {
    let pool = container.db_pool();
    if let Some(note) = most_recent_workspace_note(pool).await? {
        return Ok(workspace_note_to_compat(note));
    }

    let created = create_workspace_note_impl(
        container,
        CreateWorkspaceNoteRequestDto {
            title: Some(today_daily_title()),
        },
    )
    .await?;
    Ok(workspace_note_to_compat(created))
}

pub async fn quick_capture_impl(container: &Container, content: String) -> Result<()> {
    let pool = container.db_pool();
    let existing = most_recent_workspace_note(pool).await?;
    let mut note = if let Some(note) = existing {
        note
    } else {
        create_workspace_note_impl(
            container,
            CreateWorkspaceNoteRequestDto {
                title: Some(today_daily_title()),
            },
        )
        .await?
    };

    let snippet = content.trim();
    if snippet.is_empty() {
        return Ok(());
    }
    note.content = if note.content.trim().is_empty() {
        snippet.to_string()
    } else {
        format!("{}\n\n{}", note.content, snippet)
    };

    let _ = update_workspace_note_impl(container, note).await?;
    Ok(())
}

pub async fn get_daily_notes_range_impl(
    container: &Container,
    request: DailyNotesRangeRequestDto,
) -> Result<Vec<DailyNoteCompatDto>> {
    let rows = sqlx::query_as::<_, WorkspaceNoteRow>(
        r#"
        SELECT
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        FROM daily_notes_workspace
        WHERE date(updated_at) BETWEEN date(?) AND date(?)
        ORDER BY updated_at DESC
        "#,
    )
    .bind(request.start_date)
    .bind(request.end_date)
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch notes by range: {}", e)))?;

    rows.into_iter()
        .map(row_to_dto)
        .map(|note| note.map(workspace_note_to_compat))
        .collect()
}

pub async fn get_previous_daily_note_impl(
    container: &Container,
    request: DailyNoteCursorRequestDto,
) -> Result<Option<DailyNoteCompatDto>> {
    let row = sqlx::query_as::<_, WorkspaceNoteRow>(
        r#"
        SELECT
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        FROM daily_notes_workspace
        WHERE date(updated_at) < date(?)
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(request.current_date)
    .fetch_optional(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch previous note: {}", e)))?;

    row.map(row_to_dto)
        .transpose()
        .map(|note| note.map(workspace_note_to_compat))
}

pub async fn get_next_daily_note_impl(
    container: &Container,
    request: DailyNoteCursorRequestDto,
) -> Result<Option<DailyNoteCompatDto>> {
    let row = sqlx::query_as::<_, WorkspaceNoteRow>(
        r#"
        SELECT
            id,
            title,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            created_at,
            updated_at
        FROM daily_notes_workspace
        WHERE date(updated_at) > date(?)
        ORDER BY updated_at ASC
        LIMIT 1
        "#,
    )
    .bind(request.current_date)
    .fetch_optional(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch next note: {}", e)))?;

    row.map(row_to_dto)
        .transpose()
        .map(|note| note.map(workspace_note_to_compat))
}

pub async fn update_daily_note_content_impl(
    container: &Container,
    request: UpdateDailyNoteContentRequestDto,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let note_id = request.note_id.clone();
    sqlx::query(
        r#"
        UPDATE daily_notes_workspace
        SET content = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(request.content)
    .bind(&now)
    .bind(&note_id)
    .execute(container.db_pool())
    .await
    .map_err(|e| AppError::Database(format!("Failed to update daily note content: {}", e)))?;

    // Daily notes share the same `daily_notes_workspace` table as
    // workspace notes — backend-side they're the same entity. Reuse the
    // workspace writeback path so vault export is consistent.
    if let Ok(note) = get_note_by_id(container.db_pool(), &note_id).await {
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
