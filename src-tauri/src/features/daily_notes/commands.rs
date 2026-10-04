use super::repository::{DailyNotesRepository, WorkspaceNoteRecord};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::qa::dto::SourceDto;
use crate::features::vault::writeback;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::timestamps::now_db_timestamp;
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

pub(super) fn row_to_dto(row: WorkspaceNoteRecord) -> Result<WorkspaceNoteDto> {
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
        sources: from_json(&row.sources_json, "sources_json")?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

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
            sources_json: to_json(&note.sources)?,
            created_at: note.created_at,
            updated_at: now_db_timestamp(),
        })
        .await?;

    let updated = get_note_by_id(container, &repository, &note_id).await?;
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

/// Remap citation IDs in an appended synthesis when they overlap IDs already
/// used by the note. This keeps existing links stable while retaining all
/// metadata from the incoming source objects.
pub(super) fn append_sources_and_remap_citations(
    existing_sources: &[SourceDto],
    snippet: &str,
    incoming_sources: Vec<SourceDto>,
) -> (String, Vec<SourceDto>) {
    let mut used: std::collections::HashSet<u32> = existing_sources
        .iter()
        .filter_map(|source| source.citation_id)
        .collect();
    // Keep every incoming ID reserved too, so assigning a replacement for a
    // collision cannot steal an ID that another incoming source already uses.
    used.extend(
        incoming_sources
            .iter()
            .filter_map(|source| source.citation_id),
    );
    let mut next_id = used
        .iter()
        .copied()
        .max()
        .and_then(|id| id.checked_add(1))
        .unwrap_or(1);
    let mut remapped = std::collections::HashMap::new();
    let existing_ids: std::collections::HashSet<u32> = existing_sources
        .iter()
        .filter_map(|source| source.citation_id)
        .collect();

    for source in &incoming_sources {
        let Some(citation_id) = source.citation_id else {
            continue;
        };
        if !existing_ids.contains(&citation_id) {
            continue;
        }
        if remapped.contains_key(&citation_id) {
            continue;
        }
        while used.contains(&next_id) {
            next_id = next_id.checked_add(1).unwrap_or(1);
        }
        remapped.insert(citation_id, next_id);
        used.insert(next_id);
        next_id = next_id.checked_add(1).unwrap_or(1);
    }

    let content = if remapped.is_empty() {
        snippet.to_string()
    } else {
        // A single pass prevents replacement chains such as [1] -> [3] then
        // [3] -> [4] from changing the new citation a second time.
        if let Ok(pattern) = regex::Regex::new(r"\[(\d+)\]") {
            pattern
                .replace_all(snippet, |captures: &regex::Captures<'_>| {
                    captures[1]
                        .parse::<u32>()
                        .ok()
                        .and_then(|id| remapped.get(&id).copied())
                        .map(|id| format!("[{id}]"))
                        .unwrap_or_else(|| captures[0].to_string())
                })
                .into_owned()
        } else {
            snippet.to_string()
        }
    };

    let incoming_sources = incoming_sources
        .into_iter()
        .map(|source| {
            let mut source = source;
            if let Some(old_id) = source.citation_id {
                if let Some(new_id) = remapped.get(&old_id) {
                    source.citation_id = Some(*new_id);
                }
            }
            source
        })
        .collect();
    (content, incoming_sources)
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
    sources: Option<Vec<SourceDto>>,
    conversation_ids: Option<Vec<String>>,
) -> Result<QuickCaptureResultDto> {
    let snippet = capture_snippet(&content)?.to_string();

    let repository = DailyNotesRepository::new(container.db_pool().clone());
    let (mut note, created) = resolve_capture_target(container, &repository).await?;

    let (snippet, incoming_sources) =
        append_sources_and_remap_citations(&note.sources, &snippet, sources.unwrap_or_default());
    note.sources.extend(incoming_sources);
    note.content = appended_capture(&note.content, &snippet);
    for conversation_id in conversation_ids.unwrap_or_default() {
        let conversation_id = conversation_id.trim();
        if !conversation_id.is_empty()
            && !note
                .linked_conversation_ids
                .iter()
                .any(|id| id == conversation_id)
        {
            note.linked_conversation_ids
                .push(conversation_id.to_string());
        }
    }

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
    writeback::spawn_sync_workspace_note(container);
    Ok(())
}
