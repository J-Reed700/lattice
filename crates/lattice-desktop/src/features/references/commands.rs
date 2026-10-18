//! Pure command implementations for passage references.
//!
//! Repository access is always on demand from `container.db_pool()` (the
//! `features::daily_notes` pattern). No filesystem access anywhere in this
//! slice — the repository is the only source of truth.

use super::dto::{
    CreatePassageReferenceRequestDto, PassageReferenceDto, UpdatePassageReferenceRequestDto,
};
use super::repository::PassageReferenceRepository;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::references::PassageReferenceRecord;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::timestamps::now_db_timestamp;
use uuid::Uuid;

const DEFAULT_LIST_LIMIT: i64 = 200;
const MAX_LIST_LIMIT: i64 = 1000;
/// Chunk ids are uuid-shaped; a longer one is not an id, and is refused
/// rather than shortened into a different one.
const MAX_ID_CHARS: usize = 200;

/// Trims and collapses empty to `None`. Never shortens: a title or locator is
/// kept as given, and a long one is clipped where it is displayed, not cut
/// where it is stored.
fn optional_field(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
}

/// An optional id, refused when it is longer than an id can be.
fn optional_id(value: Option<String>, field: &str) -> Result<Option<String>> {
    let id = optional_field(value);
    if id
        .as_ref()
        .is_some_and(|id| id.chars().count() > MAX_ID_CHARS)
    {
        return Err(AppError::InvalidInput(format!(
            "{field} is longer than {MAX_ID_CHARS} characters, which no id is"
        )));
    }
    Ok(id)
}

/// Trims and collapses empty to `None`; never shortens. A note is the user's
/// own writing and is kept as they wrote it.
fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
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

/// Validates and trims a create request into a storable record.
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
        chunk_id: optional_id(request.chunk_id, "chunkId")?,
        file_path,
        file_name,
        locator: optional_field(request.locator),
        // Stored whole: a saved passage is read back as the user saved it,
        // and week synthesis reads it in full.
        text,
        title: optional_field(request.title),
        note: optional_text(request.note),
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
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
    let records = repository(container).list(limit).await?;
    Ok(records.into_iter().map(record_to_dto).collect())
}

pub async fn update_passage_reference_impl(
    container: &Container,
    request: UpdatePassageReferenceRequestDto,
) -> Result<PassageReferenceDto> {
    let id = required_field(&request.id, "id")?;
    let title = optional_field(request.title);
    let note = optional_text(request.note);

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
