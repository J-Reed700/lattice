//! Pure command implementations for passage references.
//!
//! Repository access is always on demand from `container.db_pool()` (the
//! `features::daily_notes` pattern). No filesystem access anywhere in this
//! slice — the repository is the only source of truth.

use super::dto::{
    CreatePassageReferenceRequestDto, PassageReferenceDto, UpdatePassageReferenceRequestDto,
};
use super::repository::{PassageReferenceRecord, PassageReferenceRepository};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::time::now_db_timestamp;
use uuid::Uuid;

const DEFAULT_LIST_LIMIT: i64 = 200;
const MAX_LIST_LIMIT: i64 = 1000;
pub(super) const MAX_TEXT_CHARS: usize = 20_000;
const MAX_TITLE_CHARS: usize = 200;
/// Chunk ids are uuid-shaped; this only guards against a hostile caller.
const MAX_ID_CHARS: usize = 200;
const MAX_NOTE_CHARS: usize = 4_000;
const MAX_LOCATOR_CHARS: usize = 120;

/// Char-safe truncation with a trailing ellipsis. Never slices bytes.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut output = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

/// Trims, collapses empty to `None`, and truncates to `max_chars`.
fn optional_field(value: Option<String>, max_chars: usize) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
        .map(|trimmed| truncate_chars(&trimmed, max_chars))
}

fn required_field(value: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(format!("{field} is required")));
    }
    Ok(trimmed.to_string())
}

fn record_to_dto(record: PassageReferenceRecord) -> PassageReferenceDto {
    PassageReferenceDto {
        id: record.id,
        document_id: record.document_id,
        chunk_id: record.chunk_id,
        file_path: record.file_path,
        file_name: record.file_name,
        locator: record.locator,
        text: record.text,
        title: record.title,
        note: record.note,
        created_at: record.created_at,
    }
}

fn repository(container: &Container) -> PassageReferenceRepository {
    PassageReferenceRepository::new(container.db_pool().clone())
}

/// Validates, trims and truncates a create request into a storable record.
///
/// Pure: this is where every input rule lives, so it can be tested without a
/// `Container` or a database.
pub(super) fn build_create_record(
    request: CreatePassageReferenceRequestDto,
) -> Result<PassageReferenceRecord> {
    let document_id = required_field(&request.document_id, "documentId")?;
    let file_path = required_field(&request.file_path, "filePath")?;
    let file_name = required_field(&request.file_name, "fileName")?;
    let text = required_field(&request.text, "text")?;

    Ok(PassageReferenceRecord {
        id: format!("pref_{}", Uuid::new_v4()),
        document_id,
        chunk_id: optional_field(request.chunk_id, MAX_ID_CHARS),
        file_path,
        file_name,
        locator: optional_field(request.locator, MAX_LOCATOR_CHARS),
        text: truncate_chars(&text, MAX_TEXT_CHARS),
        title: optional_field(request.title, MAX_TITLE_CHARS),
        note: optional_field(request.note, MAX_NOTE_CHARS),
        created_at: now_db_timestamp(),
    })
}

pub async fn create_passage_reference_impl(
    container: &Container,
    request: CreatePassageReferenceRequestDto,
) -> Result<PassageReferenceDto> {
    let record = build_create_record(request)?;
    repository(container).insert(&record).await?;
    Ok(record_to_dto(record))
}

pub async fn list_passage_references_impl(
    container: &Container,
    limit: Option<i64>,
) -> Result<Vec<PassageReferenceDto>> {
    let limit = limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let records = repository(container).list(limit).await?;
    Ok(records.into_iter().map(record_to_dto).collect())
}

pub async fn update_passage_reference_impl(
    container: &Container,
    request: UpdatePassageReferenceRequestDto,
) -> Result<PassageReferenceDto> {
    let id = required_field(&request.id, "id")?;
    let title = optional_field(request.title, MAX_TITLE_CHARS);
    let note = optional_field(request.note, MAX_NOTE_CHARS);

    let repo = repository(container);
    repo.update_annotations(&id, title.as_deref(), note.as_deref())
        .await?;
    let record = repo.get(&id).await?;
    Ok(record_to_dto(record))
}

pub async fn delete_passage_reference_impl(container: &Container, id: String) -> Result<()> {
    let id = required_field(&id, "id")?;
    let removed = repository(container).delete(&id).await?;
    if !removed {
        return Err(AppError::NotFound(format!(
            "Passage reference not found: {id}"
        )));
    }
    Ok(())
}
