//! Favorites Repository Implementation
//!
//! Infrastructure implementation for favorites persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the FavoritesRepositoryPort trait,
//! providing operations for managing document favorites.

use async_trait::async_trait;
use chrono::Utc;
use lattice_core::application::contracts::favorites::FavoriteRecord as FavoriteDto;
use lattice_core::application::ports::FavoritesRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use sqlx::Row;
use uuid::Uuid;

/// SQLite implementation of favorites repository.
///
/// Handles persistence of favorite documents in SQLite database.
///
/// # Thread Safety
///
/// This repository is thread-safe through the Database's connection management.
#[derive(Debug, Clone)]
pub struct FavoritesRepository {
    db: Database,
}

impl FavoritesRepository {
    /// Create a new favorites repository.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FavoritesRepositoryPort for FavoritesRepository {
    async fn add_favorite(&self, document_id: &str) -> Result<FavoriteDto> {
        let favorite_id = Uuid::new_v4().to_string();
        let added_at = Utc::now().to_rfc3339();

        let mut tx = self.db.begin_write().await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?)")
                .bind(document_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound("Document not found".into()));
        }
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
        .execute(&mut *tx)
        .await?;

        // Retrieve favorite with document details
        let result = sqlx::query(
            r#"
            SELECT
                f.id as favorite_id,
                f.document_id,
                f.added_at,
                d.file_name,
                d.file_path,
                d.mime_type as file_type
            FROM favorites f
            INNER JOIN documents d ON f.document_id = d.id
            WHERE f.document_id = ?
            "#,
        )
        .bind(document_id)
        .fetch_one(&mut *tx)
        .await?;

        let favorite = FavoriteDto {
            id: result.try_get("favorite_id")?,
            document_id: result.try_get("document_id")?,
            document_name: result.try_get("file_name")?,
            document_path: result.try_get("file_path")?,
            file_type: result.try_get("file_type")?,
            added_at: result.try_get("added_at")?,
        };
        tx.commit().await?;
        Ok(favorite)
    }

    async fn remove_favorite(&self, document_id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        let result = sqlx::query!(
            r#"
                    DELETE FROM favorites
                    WHERE document_id = ?
                    "#,
            document_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to remove favorite: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Favorite not found for document_id: {}",
                document_id
            )));
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_favorites(&self) -> Result<Vec<FavoriteDto>> {
        let mut read = self.db.begin_read().await?;
        let results = sqlx::query(
            r#"
            SELECT
                f.id as favorite_id,
                f.document_id,
                f.added_at,
                d.file_name,
                d.file_path,
                d.mime_type as file_type
            FROM favorites f
            INNER JOIN documents d ON f.document_id = d.id
            ORDER BY f.added_at DESC
            "#,
        )
        .fetch_all(&mut *read)
        .await?;
        read.commit().await?;

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
    }

    async fn is_favorite(&self, document_id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let favorite = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM favorites WHERE document_id = ?)",
        )
        .bind(document_id)
        .fetch_one(&mut *read)
        .await?;
        read.commit().await?;
        Ok(favorite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new().connect(":memory:").await.unwrap()
    }

    async fn setup_schema(pool: &SqlitePool) {
        // Exercise the same schema as the application, including future migrations.
        sqlx::migrate!("./migrations").run(pool).await.unwrap();
        sqlx::query(
            r#"
            WITH seed(id, path, name, mime, size) AS (
                VALUES ('doc1', '/test/doc.pdf', 'doc.pdf', 'application/pdf', 1024)
            )
            INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes,
                modified_at, indexed_at, checksum, status)
            SELECT id, path, name, mime, size, '2026-09-15T00:00:00Z',
                '2026-09-15T00:00:00Z', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'indexed'
            FROM seed
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
        let repo = FavoritesRepository::new(Database::new(pool.clone()));

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
        let repo = FavoritesRepository::new(Database::new(pool.clone()));

        // First add should succeed
        repo.add_favorite("doc1").await.unwrap();

        // Second add should not fail (ON CONFLICT DO NOTHING)
        let result = repo.add_favorite("doc1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn failed_metadata_read_does_not_commit_favorite() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(Database::new(pool.clone()));
        assert!(matches!(
            repo.add_favorite("missing").await,
            Err(AppError::NotFound(_))
        ));
        // A schema-level read failure after the insert exercises rollback.
        sqlx::query("ALTER TABLE documents RENAME COLUMN file_name TO unavailable_name")
            .execute(&pool)
            .await
            .unwrap();
        assert!(repo.add_favorite("doc1").await.is_err());
        assert!(!repo.is_favorite("doc1").await.unwrap());
        sqlx::query("ALTER TABLE documents RENAME COLUMN unavailable_name TO file_name")
            .execute(&pool)
            .await
            .unwrap();
        repo.add_favorite("doc1").await.unwrap();
        assert!(repo.is_favorite("doc1").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove_favorite() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(Database::new(pool.clone()));

        repo.add_favorite("doc1").await.unwrap();

        assert!(repo.is_favorite("doc1").await.unwrap());

        repo.remove_favorite("doc1").await.unwrap();

        assert!(!repo.is_favorite("doc1").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove_favorite_not_found() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(Database::new(pool.clone()));

        let result = repo.remove_favorite("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_list_favorites() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(Database::new(pool.clone()));

        // Initially empty
        let favorites = repo.list_favorites().await.unwrap();
        assert_eq!(favorites.len(), 0);

        repo.add_favorite("doc1").await.unwrap();

        let favorites = repo.list_favorites().await.unwrap();
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].document_id, "doc1");
    }

    #[tokio::test]
    async fn test_is_favorite() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = FavoritesRepository::new(Database::new(pool.clone()));

        // Not a favorite initially
        assert!(!repo.is_favorite("doc1").await.unwrap());

        repo.add_favorite("doc1").await.unwrap();

        // Now it is a favorite
        assert!(repo.is_favorite("doc1").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_favorites_ordered_by_added_at() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;

        sqlx::query(
            r#"
            INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes,
                modified_at, checksum, status)
            VALUES ('doc2', '/test/doc2.pdf', 'doc2.pdf', 'application/pdf', 2048,
                '2026-09-15T00:00:00Z', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'indexed')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo = FavoritesRepository::new(Database::new(pool.clone()));

        repo.add_favorite("doc1").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        repo.add_favorite("doc2").await.unwrap();

        let favorites = repo.list_favorites().await.unwrap();
        assert_eq!(favorites.len(), 2);
        assert_eq!(favorites[0].document_id, "doc2"); // Most recent
        assert_eq!(favorites[1].document_id, "doc1");
    }
}
