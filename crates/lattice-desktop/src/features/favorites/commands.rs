//! Favorites/Bookmarks Management Commands
//!
//! Thin command controllers for managing user's favorite documents following modern patterns.
//! Provides persistent document bookmarking with idempotent operations,
//! and efficient enum-based dispatch pattern.
//!
//! # Commands (5 total)
//!
//! - `favorite_operation` - Unified dispatcher (add/remove/getAll/check)
//! - `add_favorite` - Add document to favorites
//! - `remove_favorite` - Remove document from favorites
//! - `get_favorites` - Retrieve all favorites with metadata
//! - `is_favorite` - Check if document is favorited
//!
//! # Features
//!
//! - **Persistent Storage**: SQLite-backed favorites table
//! - **Idempotent Operations**: Safe to add/remove multiple times
//! - **Metadata Enrichment**: Favorites include document name, path, type, timestamp
//! - **Order Preservation**: Favorites ordered by most recently added
//!
//! # Database Schema
//!
//! ```sql
//! CREATE TABLE favorites (
//!     id TEXT PRIMARY KEY,
//!     document_id TEXT NOT NULL REFERENCES documents(id),
//!     added_at TEXT NOT NULL DEFAULT (datetime('now')),
//!     UNIQUE(document_id)
//! );
//! ```
//!
//! # Architecture
//!
//! Commands delegate directly to SQLite with internal helper functions for
//! idempotent operations and data enrichment via document JOIN queries.

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

/// Unified favorite operation dispatcher using enum-based pattern
///
/// Single command handling multiple favorite operations (add, remove, getAll, check) via enum
/// dispatch pattern. Simplifies frontend API by consolidating related operations into a single
/// endpoint with type-safe operation selection.
///
/// # Operation Variants
///
/// - **Add**: Add document to favorites (idempotent)
/// - **Remove**: Remove document from favorites (idempotent)
/// - **GetAll**: Retrieve all favorites with metadata
/// - **Check**: Check if document is favorited
///
/// # Architecture
///
/// Enum dispatch pattern delegating to internal helper functions
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

/// Add a document to favorites with idempotent behavior
///
/// Adds the specified document to the user's favorites list. Operation is idempotent - calling
/// multiple times has no effect beyond the first call. Verifies document exists before adding.
///
/// # Behavior
///
/// 1. Verify document exists in database
/// 2. Check if already favorited (skip if true)
/// 3. Insert favorite record with UUID and timestamp
///
/// # Idempotency
///
/// Safe to call multiple times for the same document. If already favorited, operation
/// succeeds immediately without database modification.
///
/// # Security
///
/// - Action: `FavoriteAdded`
/// - Resource ID: `favorite:{document_id}`
/// - Metadata: document_id, operation name
/// - Result: success/failure with error details
///
/// # Architecture
///
/// Thin controller delegating to `add_favorite_internal` helper
pub async fn add_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    add_favorite_impl(container.inner(), document_id).await
}

/// Implementation layer (Pure Rust - No Tauri)
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

/// Remove a document from favorites with idempotent behavior
///
/// Removes the specified document from the user's favorites list. Operation is idempotent -
/// calling multiple times has no effect beyond the first call. Safe to call even if document
/// was never favorited.
///
/// # Behavior
///
/// 1. Execute DELETE query for document_id
/// 2. Check rows affected (0 = not favorited, >0 = removed)
/// 3. Return success regardless of initial state
///
/// # Idempotency
///
/// Safe to call multiple times for the same document. If not favorited, operation
/// succeeds immediately (0 rows affected, no error).
///
/// # Security
///
/// - Action: `FavoriteRemoved`
/// - Resource ID: `favorite:{document_id}`
/// - Metadata: document_id, operation name
/// - Result: success/failure with error details
///
/// # Architecture
///
/// Thin controller delegating to `remove_favorite_internal` helper
pub async fn remove_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    remove_favorite_impl(container.inner(), document_id).await
}

/// Implementation layer (Pure Rust - No Tauri)
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

/// Get all favorite documents with enriched metadata via JOIN query
///
/// Retrieves the complete list of favorited documents with metadata enrichment from the
/// documents table. Favorites are returned in reverse chronological order (most recently
/// added first) with document name, path, type, and timestamp information.
///
/// # Database Query
///
/// Executes JOIN query to enrich favorites with document metadata:
/// ```sql
/// SELECT
///     f.id,
///     f.document_id,
///     d.file_name,
///     d.file_path,
///     d.file_type,
///     f.added_at
/// FROM favorites f
/// INNER JOIN documents d ON f.document_id = d.id
/// ORDER BY f.added_at DESC
/// ```
///
/// **Note**: Favorites for deleted documents are automatically excluded (INNER JOIN).
///
/// # Sort Order
///
/// Favorites returned in **reverse chronological order** (most recently added first).
/// This matches typical UI patterns where recent items appear at the top.
///
/// # Architecture
///
/// Thin controller delegating to `get_favorites_internal` helper with JOIN query
pub async fn get_favorites(
    container: State<'_, Container>,
) -> Result<Vec<FavoriteDocument>, AppError> {
    get_favorites_impl(container.inner()).await
}

/// Implementation layer (Pure Rust - No Tauri)
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

/// Check if a document is favorited with fast boolean status query
///
/// Efficiently checks whether the specified document is in the user's favorites list.
/// Returns a simple boolean flag without fetching additional metadata. Useful for
/// updating UI favorite icons/buttons.
///
/// # Database Query
///
/// Executes lightweight COUNT query:
/// ```sql
/// SELECT COUNT(*) FROM favorites WHERE document_id = ?
/// ```
///
/// Returns `true` if count > 0, `false` otherwise.
///
/// # Architecture
///
/// Thin controller delegating to `is_favorite_internal` helper with COUNT query
pub async fn is_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<bool, AppError> {
    is_favorite_impl(container.inner(), document_id).await
}

/// Implementation layer (Pure Rust - No Tauri)
pub async fn is_favorite_impl(
    container: &Container,
    document_id: String,
) -> Result<bool, AppError> {
    let repository = container.favorites_repository();
    is_favorite_internal(&document_id, repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to check favorite status: {}", e)))
}

/// Internal implementation for checking favorite status
async fn is_favorite_internal(
    document_id: &str,
    repository: &dyn FavoritesRepositoryPort,
) -> Result<bool> {
    repository.is_favorite(document_id).await
}

#[cfg(test)]
mod tests {
    // Tests would go here - integration tests with test database
}
