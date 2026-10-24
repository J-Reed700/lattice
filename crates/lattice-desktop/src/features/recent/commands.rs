//! Recent documents tracking commands.

use crate::interfaces::di::Container;
use lattice_core::application::contracts::recent_documents::RecentDocument;
use lattice_core::application::ports::RecentDocumentsRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum RecentDocumentOperation {
    Track { document_id: String },
    GetRecent { limit: usize },
    Clear,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum RecentDocumentResponse {
    Tracked,
    Documents(Vec<RecentDocument>),
    Cleared,
}

pub async fn recent_document_operation(
    operation: RecentDocumentOperation,
    container: State<'_, Container>,
) -> Result<RecentDocumentResponse, AppError> {
    let repository = container.recent_documents_repository();
    match operation {
        RecentDocumentOperation::Track { document_id } => {
            track_document_access_internal(&document_id, repository.as_ref()).await?;
            Ok(RecentDocumentResponse::Tracked)
        }
        RecentDocumentOperation::GetRecent { limit } => {
            let docs = get_recent_documents_internal(limit, repository.as_ref()).await?;
            Ok(RecentDocumentResponse::Documents(docs))
        }
        RecentDocumentOperation::Clear => {
            clear_recent_documents_internal(repository.as_ref()).await?;
            Ok(RecentDocumentResponse::Cleared)
        }
    }
}

pub async fn track_document_access(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    let repository = container.recent_documents_repository();
    track_document_access_internal(&document_id, repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to track document access: {}", e)))
}

async fn track_document_access_internal(
    document_id: &str,
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<()> {
    repository.track_access(document_id).await
}

async fn get_recent_documents_internal(
    limit: usize,
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<Vec<RecentDocument>> {
    Ok(repository
        .get_recent_documents(limit)
        .await?
        .into_iter()
        .map(RecentDocument::from)
        .collect())
}

pub async fn clear_recent_documents(container: State<'_, Container>) -> Result<(), AppError> {
    let repository = container.recent_documents_repository();
    clear_recent_documents_internal(repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to clear recent documents: {}", e)))
}

async fn clear_recent_documents_internal(
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<()> {
    repository.clear_recent_history(None).await?;
    Ok(())
}

#[cfg(test)]
mod tests {}
