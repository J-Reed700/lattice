//! Document listing and metadata queries.

use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result, ResultExt};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Document metadata DTO for display in UI.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetadataDto {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_type: String,
    pub category: String,
    pub language: String,
    pub modified_at: String,
    pub indexed_at: String,
    pub word_count: i32,
}

/// List all indexed documents with metadata.
pub async fn list_all_documents(
    container: State<'_, Container>,
    limit: usize,
) -> Result<Vec<DocumentMetadataDto>> {
    if limit == 0 || limit > 10000 {
        return Err(AppError::InvalidInput(
            "Limit must be between 1 and 10000".to_string(),
        ));
    }

    let repo = container.document_repository();

    let documents = repo
        .find_all_paginated(limit)
        .await
        .context("Failed to query documents table")?;

    let dtos: Vec<DocumentMetadataDto> = documents
        .into_iter()
        .map(|doc| DocumentMetadataDto {
            id: doc.id().as_str().to_string(),
            file_name: doc.file_name().to_string(),
            file_path: doc.file_path().display().to_string(),
            file_type: doc.file_type().map(|s| s.to_string()).unwrap_or_default(),
            category: doc.category().to_string(),
            language: doc.language().to_string(),
            modified_at: doc.modified_at().to_rfc3339(),
            indexed_at: doc.indexed_at().to_rfc3339(),
            word_count: doc.word_count(),
        })
        .collect();

    Ok(dtos)
}

/// Get document metadata by ID.
pub async fn get_document_impl(
    container: &Container,
    document_id: String,
) -> Result<DocumentMetadataDto> {
    use uuid::Uuid;
    let _uuid = Uuid::parse_str(&document_id).map_err(|_| {
        AppError::InvalidInput(format!("Invalid document ID format: {}", document_id))
    })?;

    let repo = container.document_repository();

    let doc = repo
        .find_by_id(&document_id)
        .await
        .context("Failed to query document by ID")?
        .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

    let dto = DocumentMetadataDto {
        id: doc.id().as_str().to_string(),
        file_name: doc.file_name().to_string(),
        file_path: doc.file_path().display().to_string(),
        file_type: doc.file_type().map(|s| s.to_string()).unwrap_or_default(),
        category: doc.category().to_string(),
        language: doc.language().to_string(),
        modified_at: doc.modified_at().to_rfc3339(),
        indexed_at: doc.indexed_at().to_rfc3339(),
        word_count: doc.word_count(),
    };

    Ok(dto)
}

#[tauri::command]
#[specta::specta]
pub async fn get_document(
    container: State<'_, Container>,
    document_id: String,
) -> Result<DocumentMetadataDto> {
    get_document_impl(&container, document_id).await
}
