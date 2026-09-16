//! Favorites Repository Implementation
//!
//! Infrastructure implementation for favorites persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the FavoritesRepositoryPort trait,
//! providing operations for managing document favorites.

use crate::application::ports::FavoritesRepositoryPort;
use crate::features::favorites::dto::FavoriteDto;
use crate::infrastructure::persistence::database::query_with_timeout;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite implementation of favorites repository.
///
/// Handles persistence of favorite documents in SQLite database.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Debug, Clone)]
pub struct FavoritesRepository {
    pool: SqlitePool,
}

impl FavoritesRepository {
    /// Create a new favorites repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FavoritesRepositoryPort for FavoritesRepository {
    async fn add_favorite(&self, document_id: &str) -> Result<FavoriteDto> {
        let pool = self.pool.clone();
        let document_id = document_id.to_string();

        let favorite_id = Uuid::new_v4().to_string();
        let added_at = Utc::now().to_rfc3339();

        query_with_timeout(|| async {
            // Insert favorite
            sqlx::query!(
                r#"
                INSERT INTO favorites (id, document_id, added_at)
                VALUES (?, ?, ?)
                ON CONFLICT(document_id) DO NOTHING
                "#,
                favorite_id,
                document_id,
                added_at
            )
            .execute(&pool)
            .await?;

            // Retrieve favorite with document details
            let result = sqlx::query(
                r#"
                SELECT
                    f.id as favorite_id,
                    f.document_id,
                    f.added_at,
                    COALESCE(files.name, d.title) as file_name,
                    COALESCE(files.path, d.source_url, 'unknown') as file_path,
                    COALESCE(files.mime_type, d.content_type) as file_type
                FROM favorites f
                INNER JOIN documents d ON f.document_id = d.id
                LEFT JOIN files ON d.file_id = files.id
                WHERE f.document_id = ?
                "#,
            )
            .bind(&document_id)
            .fetch_one(&pool)
            .await?;

            Ok(FavoriteDto {
                id: result.try_get("favorite_id")?,
                document_id: result.try_get("document_id")?,
                document_name: result.try_get("file_name")?,
                document_path: result.try_get("file_path")?,
                file_type: result.try_get("file_type")?,
                added_at: result.try_get("added_at")?,
            })
        })
        .await
    }

    async fn remove_favorite(&self, document_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let document_id = document_id.to_string();

        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let result = sqlx::query!(
                r#"
                    DELETE FROM favorites
                    WHERE document_id = ?
                    "#,
                document_id
            )
            .execute(&pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to remove favorite: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(AppError::NotFound(format!(
                    "Favorite not found for document_id: {}",
                    document_id
                )));
            }

            Ok(())
        })
        .await
        .map_err(|_| AppError::InternalError("Delete favorite operation timed out".to_string()))?
    }

    async fn list_favorites(&self) -> Result<Vec<FavoriteDto>> {
        let pool = self.pool.clone();

        query_with_timeout(|| async {
            let results = sqlx::query(
                r#"
                SELECT
                    f.id as favorite_id,
                    f.document_id,
                    f.added_at,
                    COALESCE(files.name, d.title) as file_name,
                    COALESCE(files.path, d.source_url, 'unknown') as file_path,
                    COALESCE(files.mime_type, d.content_type) as file_type
                FROM favorites f
                INNER JOIN documents d ON f.document_id = d.id
                LEFT JOIN files ON d.file_id = files.id
                ORDER BY f.added_at DESC
                "#,
            )
            .fetch_all(&pool)
            .await?;

            results
                .into_iter()
                .map(|row| {
                    Ok(FavoriteDto {
                        id: row.try_get("favorite_id")?,
                        document_id: row.try_get("document_id")?,
                        document_name: row.try_get("file_name")?,
                        document_path: row.try_get("file_path")?,
                        file_type: row.try_get("file_type")?,
                        added_at: row.try_get("added_at")?,
                    })
                })
                .collect()
        })
        .await
    }

    async fn is_favorite(&self, document_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let document_id = document_id.to_string();

        query_with_timeout(|| async {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM favorites WHERE document_id = ?)",
            )
            .bind(document_id)
            .fetch_one(&pool)
            .await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new().connect(":memory:").await.unwrap()
    }

    async fn setup_schema(pool: &SqlitePool) {
        // Create files table
        sqlx::query(
            r#"
            CREATE TABLE files (
                id TEXT PRIMARY KEY NOT NULL,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                mime_type TEXT,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                checksum_sha256 TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Create documents table
        sqlx::query(
            r#"
            CREATE TABLE documents (
                id TEXT PRIMARY KEY NOT NULL,
                file_id TEXT,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                source_url TEXT,
                source_type TEXT NOT NULL CHECK (source_type IN ('web', 'file', 'manual', 'api')),
                content_type TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'indexed', 'failed')) DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                indexed_at TEXT,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Create favorites table
        sqlx::query(
            r#"
            CREATE TABLE favorites (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL UNIQUE,
                added_at TEXT NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Insert test file
        sqlx::query(
            r#"
            INSERT INTO files (id, path, name, mime_type, size_bytes, checksum_sha256)
            VALUES ('file1', '/test/doc.pdf', 'doc.pdf', 'application/pdf', 1024, 'abc123')
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Insert test document (file-based)
        sqlx::query(
            r#"
            INSERT INTO documents (id, file_id, title, content, source_type, content_type, status)
            VALUES ('doc1', 'file1', 'Test Document', 'Content here', 'file', 'application/pdf', 'indexed')
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_add_favorite() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(pool);

        let favorite = repo.add_favorite("doc1").await.unwrap();

        assert_eq!(favorite.document_id, "doc1");
        assert_eq!(favorite.document_name, "doc.pdf");
        assert_eq!(favorite.document_path, "/test/doc.pdf");
        assert_eq!(favorite.file_type, Some("application/pdf".to_string()));
        assert!(!favorite.id.is_empty());
    }

    #[tokio::test]
    async fn test_add_favorite_duplicate() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(pool);

        // First add should succeed
        repo.add_favorite("doc1").await.unwrap();

        // Second add should not fail (ON CONFLICT DO NOTHING)
        let result = repo.add_favorite("doc1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_remove_favorite() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(pool.clone());

        // Add favorite
        repo.add_favorite("doc1").await.unwrap();

        // Verify it exists
        assert!(repo.is_favorite("doc1").await.unwrap());

        // Remove it
        repo.remove_favorite("doc1").await.unwrap();

        // Verify it's gone
        assert!(!repo.is_favorite("doc1").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove_favorite_not_found() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(pool);

        let result = repo.remove_favorite("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_list_favorites() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(pool.clone());

        // Initially empty
        let favorites = repo.list_favorites().await.unwrap();
        assert_eq!(favorites.len(), 0);

        // Add a favorite
        repo.add_favorite("doc1").await.unwrap();

        // Should have one favorite
        let favorites = repo.list_favorites().await.unwrap();
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].document_id, "doc1");
    }

    #[tokio::test]
    async fn test_is_favorite() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(pool);

        // Not a favorite initially
        assert!(!repo.is_favorite("doc1").await.unwrap());

        // Add as favorite
        repo.add_favorite("doc1").await.unwrap();

        // Now it is a favorite
        assert!(repo.is_favorite("doc1").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_favorites_ordered_by_added_at() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;

        // Add another test file
        sqlx::query(
            r#"
            INSERT INTO files (id, path, name, mime_type, size_bytes, checksum_sha256)
            VALUES ('file2', '/test/doc2.pdf', 'doc2.pdf', 'application/pdf', 2048, 'def456')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Add another test document
        sqlx::query(
            r#"
            INSERT INTO documents (id, file_id, title, content, source_type, content_type, status)
            VALUES ('doc2', 'file2', 'Test Document 2', 'Content here', 'file', 'application/pdf', 'indexed')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo = FavoritesRepository::new(pool);

        // Add favorites in sequence
        repo.add_favorite("doc1").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        repo.add_favorite("doc2").await.unwrap();

        // Should be ordered by added_at DESC (most recent first)
        let favorites = repo.list_favorites().await.unwrap();
        assert_eq!(favorites.len(), 2);
        assert_eq!(favorites[0].document_id, "doc2"); // Most recent
        assert_eq!(favorites[1].document_id, "doc1");
    }
}
