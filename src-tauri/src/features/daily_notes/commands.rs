use super::repository::{DailyNotesRepository, WorkspaceNoteRecord};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::qa::dto::SourceDto;
use crate::features::vault::writeback;
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use crate::shared::persistence::timestamps::now_db_timestamp;
use uuid::Uuid;

pub(super) use super::dto::row_to_dto;
use super::dto::to_json;
pub use super::dto::*;

async fn get_note_by_id(
    container: &Container,
    repository: &DailyNotesRepository,
    note_id: &str,
) -> Result<WorkspaceNoteDto> {
    hydrate_workspace_note(
        repository,
        &ConversationRepository::new(container.db_pool().clone()),
        repository.get(note_id).await?,
    )
    .await
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

    let conversations = ConversationRepository::new(container.db_pool().clone());
    let mut notes = Vec::with_capacity(rows.len());
    for row in rows {
        notes.push(hydrate_workspace_note(&repository, &conversations, row).await?);
    }
    Ok(notes)
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
            revision: 0,
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
            sources_json: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
        .await?;

    let note = get_note_by_id(container, &repository, &note_id).await?;
    // Vault writeback: fire-and-forget. Failures log but never block
    // the SQL commit; the database is still the canonical write.
    writeback::spawn_sync_workspace_note(container);
    Ok(note)
}

pub async fn update_workspace_note_impl(
    container: &Container,
    mut note: WorkspaceNoteDto,
) -> Result<WorkspaceNoteDto> {
    let note_id = note.id.clone();
    enrich_sources_from_linked_conversations(
        &ConversationRepository::new(container.db_pool().clone()),
        &note.linked_conversation_ids,
        &mut note.sources,
    )
    .await;
    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let saved = repository
        .update(&WorkspaceNoteRecord {
            id: note_id.clone(),
            revision: note.revision,
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
            sources_json: to_json(&note.sources)?,
            created_at: note.created_at,
            updated_at: now_db_timestamp(),
        })
        .await?;

    let updated = row_to_dto(saved)?;
    writeback::spawn_sync_workspace_note(container);
    Ok(updated)
}

/// A journal page owns its copied sources. Fill older citation records from
/// explicitly linked conversations while their archive is available, but do
/// not guess if linked conversations saved different versions of the URL.
async fn enrich_sources_from_linked_conversations(
    repository: &ConversationRepository,
    conversation_ids: &[String],
    sources: &mut [SourceDto],
) -> bool {
    let mut changed = false;
    for source in sources {
        if source.web_snapshot.is_some() {
            continue;
        }
        let url = source.file_path.trim();
        if !(url
            .get(..8)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
            || url
                .get(..7)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://")))
        {
            continue;
        }
        if conversation_ids.is_empty() {
            continue;
        }
        let mut snapshots = Vec::with_capacity(conversation_ids.len());
        let mut every_link_has_snapshot = true;
        for conversation_id in conversation_ids {
            match repository
                .conversation_web_source_snapshot(conversation_id.clone(), url.to_string())
                .await
            {
                Ok(Some(snapshot)) => snapshots.push(snapshot),
                // Without a snapshot in every linked chat, we cannot know
                // which archived version this old citation came from.
                Ok(None) | Err(_) => {
                    every_link_has_snapshot = false;
                    break;
                }
            }
        }
        if !every_link_has_snapshot {
            continue;
        }
        let Some(first) = snapshots.first() else {
            continue;
        };
        if snapshots.iter().skip(1).any(|other| {
            other.content != first.content
                || other.title != first.title
                || other.truncated != first.truncated
                || other.fetched_at != first.fetched_at
        }) {
            continue;
        }
        source.web_snapshot = Some(crate::features::qa::dto::WebSnapshotDto {
            url: url.to_string(),
            title: first.title.clone(),
            text: first.content.clone(),
            fetched_at: first.fetched_at.clone(),
            truncated: first.truncated,
        });
        changed = true;
    }
    changed
}

pub(crate) async fn hydrate_workspace_note(
    repository: &DailyNotesRepository,
    conversations: &ConversationRepository,
    mut row: WorkspaceNoteRecord,
) -> Result<WorkspaceNoteDto> {
    // Retry a few times if an edit races hydration. Each persistence attempt
    // is compare-and-swap against both the source data and note revision; a
    // retry starts from the winning row and never replaces its other fields.
    for _ in 0..3 {
        let mut note = row_to_dto(row.clone())?;
        let mut sources = note.sources.clone();
        if !enrich_sources_from_linked_conversations(
            conversations,
            &note.linked_conversation_ids,
            &mut sources,
        )
        .await
        {
            return Ok(note);
        }
        let sources_json = to_json(&sources)?;
        if repository
            .update_sources_if_unchanged(&row.id, &row.sources_json, &row.updated_at, &sources_json)
            .await?
        {
            note.sources = sources;
            note.revision += 1;
            return Ok(note);
        }
        row = repository.get(&row.id).await?;
    }
    row_to_dto(row)
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
    writeback::spawn_delete_workspace_note(container);

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

pub async fn quick_capture_impl(
    container: &Container,
    content: String,
    sources: Option<Vec<SourceDto>>,
    conversation_ids: Option<Vec<String>>,
) -> Result<QuickCaptureResultDto> {
    let result = DailyNotesRepository::new(container.db_pool().clone())
        .quick_capture(
            &today_daily_title(),
            &content,
            sources.unwrap_or_default(),
            conversation_ids.unwrap_or_default(),
        )
        .await?;
    writeback::spawn_sync_workspace_note(container);
    Ok(result)
}

pub async fn capture_reference_impl(
    container: &Container,
    request: CaptureReferenceRequestDto,
) -> Result<CaptureReferenceResultDto> {
    let result = DailyNotesRepository::new(container.db_pool().clone())
        .capture_reference(request)
        .await?;
    writeback::spawn_sync_workspace_note(container);
    Ok(result)
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
    writeback::spawn_sync_workspace_note(container);
    Ok(())
}

#[cfg(test)]
pub(super) use super::capture::{
    append_sources_and_remap_citations, appended_capture, capture_snippet,
};
