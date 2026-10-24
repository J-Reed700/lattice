//! Favorites/bookmarks management commands.

use crate::interfaces::di::Container;
use lattice_core::application::ports::FavoritesRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Favorite document with metadata
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct FavoriteDocument {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum FavoriteOperation {
    Add { document_id: String },
    Remove { document_id: String },
    GetAll,
    Check { document_id: String },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum FavoriteResponse {
    Modified,
    Favorites(Vec<FavoriteDocument>),
    Status(bool),
}

pub async fn favorite_operation(
    operation: FavoriteOperation,
    container: State<'_, Container>,
) -> Result<FavoriteResponse, AppError> {
    let repository = container.favorites_repository();
    match operation {
        FavoriteOperation::Add { document_id } => {
            add_favorite_internal(&document_id, repository.as_ref()).await?;
            Ok(FavoriteResponse::Modified)
        }
        FavoriteOperation::Remove { document_id } => {
            remove_favorite_internal(&document_id, repository.as_ref()).await?;
            Ok(FavoriteResponse::Modified)
        }
        FavoriteOperation::GetAll => {
            let favorites = get_favorites_internal(repository.as_ref()).await?;
            Ok(FavoriteResponse::Favorites(favorites))
        }
        FavoriteOperation::Check { document_id } => {
            let is_fav = is_favorite_internal(&document_id, repository.as_ref()).await?;
            Ok(FavoriteResponse::Status(is_fav))
        }
    }
}

pub async fn add_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    add_favorite_impl(container.inner(), document_id).await
}

pub async fn add_favorite_impl(container: &Container, document_id: String) -> Result<(), AppError> {
    let repository = container.favorites_repository();
    let result = add_favorite_internal(&document_id, repository.as_ref()).await;

    result.map_err(|e| AppError::Other(format!("Failed to add favorite: {}", e)))
}

async fn add_favorite_internal(
    document_id: &str,
    repository: &dyn FavoritesRepositoryPort,
) -> Result<()> {
    repository.add_favorite(document_id).await?;
    Ok(())
}

pub async fn remove_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    remove_favorite_impl(container.inner(), document_id).await
}

pub async fn remove_favorite_impl(
    container: &Container,
    document_id: String,
) -> Result<(), AppError> {
    let repository = container.favorites_repository();
    let result = remove_favorite_internal(&document_id, repository.as_ref()).await;

    result.map_err(|e| AppError::Other(format!("Failed to remove favorite: {}", e)))
}

async fn remove_favorite_internal(
    document_id: &str,
    repository: &dyn FavoritesRepositoryPort,
) -> Result<()> {
    match repository.remove_favorite(document_id).await {
        Err(AppError::NotFound(_)) => Ok(()), // Commands preserve idempotent removal.
        result => result,
    }
}

pub async fn get_favorites(
    container: State<'_, Container>,
) -> Result<Vec<FavoriteDocument>, AppError> {
    get_favorites_impl(container.inner()).await
}

pub async fn get_favorites_impl(container: &Container) -> Result<Vec<FavoriteDocument>, AppError> {
    let repository = container.favorites_repository();
    get_favorites_internal(repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to get favorites: {}", e)))
}

async fn get_favorites_internal(
    repository: &dyn FavoritesRepositoryPort,
) -> Result<Vec<FavoriteDocument>> {
    Ok(repository
        .list_favorites()
        .await?
        .into_iter()
        .map(|record| FavoriteDocument {
            id: record.id,
            document_id: record.document_id,
            document_name: record.document_name,
            document_path: record.document_path,
            file_type: record.file_type,
            added_at: record.added_at,
        })
        .collect())
}

pub async fn is_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<bool, AppError> {
    is_favorite_impl(container.inner(), document_id).await
}

pub async fn is_favorite_impl(
    container: &Container,
    document_id: String,
) -> Result<bool, AppError> {
    let repository = container.favorites_repository();
    is_favorite_internal(&document_id, repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to check favorite status: {}", e)))
}

async fn is_favorite_internal(
    document_id: &str,
    repository: &dyn FavoritesRepositoryPort,
) -> Result<bool> {
    repository.is_favorite(document_id).await
}

#[cfg(test)]
mod tests {}
