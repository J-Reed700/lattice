//! Atomic capture operations share the same revision and transaction boundary as editing.
use crate::features::daily_notes::capture::{
    append_sources_and_remap_citations, appended_capture, apply_reference, capture_snippet,
};
use crate::features::daily_notes::dto::{
    dto_to_row, row_to_dto, CaptureReferenceRequestDto, CaptureReferenceResultDto,
    QuickCaptureResultDto, WorkspaceNoteDto,
};
use crate::infrastructure::persistence::repositories::workspace_notes::{
    note_from_row, DailyNotesRepository, NOTE_COLUMNS,
};
use lattice_core::application::contracts::notes::WorkspaceNoteRecord;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::timestamps::now_db_timestamp;
use sqlx::{Sqlite, SqlitePool, Transaction};

async fn capture_target(
    tx: &mut Transaction<'_, Sqlite>,
    title: &str,
    journal_id: Option<&str>,
    preferred: Option<&str>,
) -> Result<(WorkspaceNoteDto, bool)> {
    if let Some(id) = preferred {
        let query = format!("SELECT {NOTE_COLUMNS} FROM daily_notes_workspace WHERE id = ?");
        let row = sqlx::query(&query)
            .bind(id)
            .try_map(note_from_row)
            .fetch_optional(&mut **tx)
            .await?;
        // A removed explicit destination must not silently send the capture elsewhere.
        return row
            .map(row_to_dto)
            .transpose()?
            .map(|note| (note, false))
            .ok_or_else(|| AppError::NotFound(format!("Workspace note not found: {id}")));
    }
    let query = format!("SELECT {NOTE_COLUMNS} FROM daily_notes_workspace WHERE title = ? AND journal_id IS ? ORDER BY updated_at DESC, created_at, id LIMIT 1");
    if let Some(row) = sqlx::query(&query)
        .bind(title)
        .bind(journal_id)
        .try_map(note_from_row)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok((row_to_dto(row)?, false));
    }
    let now = now_db_timestamp();
    let note = WorkspaceNoteRecord {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: title.to_string(),
        journal_id: journal_id.map(str::to_string),
        content: String::new(),
        linked_document_ids: "[]".into(),
        linked_conversation_ids: "[]".into(),
        highlights_json: "[]".into(),
        sticky_notes_json: "[]".into(),
        conversation_snapshots_json: "[]".into(),
        sources_json: "[]".into(),
        created_at: now.clone(),
        updated_at: now,
    };
    DailyNotesRepository::insert_in_transaction(tx, &note).await?;
    Ok((row_to_dto(note)?, true))
}

pub(super) async fn capture_reference(
    pool: &SqlitePool,
    request: CaptureReferenceRequestDto,
) -> Result<CaptureReferenceResultDto> {
    capture_snippet(&request.message_content)?;
    if request.conversation_id.trim().is_empty()
        || request.message_id.trim().is_empty()
        || request.inbox_title.trim().is_empty()
    {
        return Err(AppError::InvalidInput(
            "A capture requires a message, conversation, and destination title".into(),
        ));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (mut note, _) = capture_target(
        &mut tx,
        &request.inbox_title,
        None,
        request.preferred_note_id.as_deref(),
    )
    .await?;
    let snapshot_id = apply_reference(&mut note, &request)?;
    note.updated_at = now_db_timestamp();
    DailyNotesRepository::update_in_transaction(&mut tx, &dto_to_row(&note)?).await?;
    tx.commit().await?;
    Ok(CaptureReferenceResultDto {
        note_id: note.id,
        note_title: note.title,
        linked_document_count: request
            .document_ids
            .iter()
            .filter(|id| !id.trim().is_empty())
            .map(|id| id.trim())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        snapshot_id,
    })
}

pub(super) async fn quick_capture(
    pool: &SqlitePool,
    title: &str,
    content: &str,
    sources: Vec<SourceDto>,
    conversation_ids: Vec<String>,
) -> Result<QuickCaptureResultDto> {
    let snippet = capture_snippet(content)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let journal_id: Option<String> = sqlx::query_scalar("SELECT j.id FROM journals j LEFT JOIN daily_notes_workspace n ON n.journal_id = j.id WHERE j.is_archived = 0 GROUP BY j.id ORDER BY MAX(n.updated_at) DESC, j.sort_order, j.created_at, j.id LIMIT 1")
        .fetch_optional(&mut *tx).await?;
    let (mut note, created) = capture_target(&mut tx, title, journal_id.as_deref(), None).await?;
    let (snippet, incoming_sources) =
        append_sources_and_remap_citations(&note.sources, snippet, sources);
    note.sources.extend(incoming_sources);
    note.content = appended_capture(&note.content, &snippet);
    for id in conversation_ids {
        let id = id.trim();
        if !id.is_empty()
            && !note
                .linked_conversation_ids
                .iter()
                .any(|current| current == id)
        {
            note.linked_conversation_ids.push(id.to_string());
        }
    }
    note.updated_at = now_db_timestamp();
    DailyNotesRepository::update_in_transaction(&mut tx, &dto_to_row(&note)?).await?;
    tx.commit().await?;
    Ok(QuickCaptureResultDto {
        note_id: note.id,
        note_title: note.title,
        created,
    })
}
