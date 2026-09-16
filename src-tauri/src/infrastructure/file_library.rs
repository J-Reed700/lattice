//! Document-repository schema persistence for the indexed-folder library.
use crate::application::contracts::file_library::{IndexedFolder, IndexingActivity};
use crate::application::ports::file_library::FileLibraryPort;
use crate::shared::{error::Result, sql_like::directory_prefix_pattern};
use sqlx::SqlitePool;

pub struct SqliteFileLibrary {
    pool: SqlitePool,
}
impl SqliteFileLibrary {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    path: String,
    recursive: bool,
    enabled: bool,
    last_scan: Option<String>,
    created_at: String,
}
#[derive(sqlx::FromRow)]
struct ActivityRow {
    id: String,
    file_path: String,
    status: String,
    timestamp: Option<String>,
}

#[async_trait::async_trait]
impl FileLibraryPort for SqliteFileLibrary {
    async fn remove_folder(&self, path: &str) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM watch_folders WHERE path = ?")
            .bind(path)
            .execute(&mut *tx)
            .await?;
        let count = sqlx::query(
            r#"DELETE FROM documents WHERE id IN (
            SELECT d.id FROM documents d
            WHERE d.file_path = ?1
               OR d.file_path LIKE ?2 ESCAPE '\'
        )"#,
        )
        .bind(path)
        .bind(directory_prefix_pattern(path))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(count)
    }
    async fn indexed_folders(&self) -> Result<Vec<IndexedFolder>> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, FolderRow>("SELECT path, recursive, enabled, last_scan, created_at FROM watch_folders ORDER BY created_at DESC")
            .fetch_all(&mut *tx).await?;
        let mut folders = Vec::with_capacity(rows.len());
        for row in rows {
            let count = sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM documents d
                WHERE d.file_path = ?1
                   OR d.file_path LIKE ?2 ESCAPE '\'"#,
            )
            .bind(&row.path)
            .bind(directory_prefix_pattern(&row.path))
            .fetch_one(&mut *tx)
            .await?;
            folders.push(IndexedFolder {
                path: row.path,
                recursive: row.recursive,
                enabled: row.enabled,
                last_scan: row.last_scan,
                document_count: count,
                created_at: row.created_at,
            });
        }
        tx.commit().await?;
        Ok(folders)
    }
    async fn indexing_activities(&self, limit: usize) -> Result<Vec<IndexingActivity>> {
        let rows = sqlx::query_as::<_, ActivityRow>("SELECT d.id, d.file_path as file_path, d.status, d.indexed_at as timestamp FROM documents d ORDER BY d.indexed_at DESC LIMIT ?")
            .bind(limit.min(10000) as i64).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| IndexingActivity {
                id: row.id,
                action: "indexed".into(),
                file_path: row.file_path,
                status: row.status,
                timestamp: row.timestamp.unwrap_or_default(),
                details: None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (SqlitePool, SqliteFileLibrary) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::raw_sql("INSERT INTO watch_folders (id, path, recursive, enabled, last_scan, created_at)
                VALUES ('watch', '/vault/a%_', 1, 1, NULL, '2026-01-01');
            INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, indexed_at, checksum, status) VALUES
                ('d1', '/vault/a%_/inside', 'inside', 1, '2026-01-01', '2026-01-01', 'checksum', 'indexed'),
                ('d2', '/vault/abc/sibling', 'sibling', 1, '2026-01-01', '', 'checksum', 'pending'),
                ('d3', '/vault/a%_-sibling/other', 'other', 1, '2025-01-01', '2025-01-01', 'checksum', 'indexed');")
            .execute(&pool).await.unwrap();
        let repo = SqliteFileLibrary::new(pool.clone());
        (pool, repo)
    }

    #[tokio::test]
    async fn document_paths_preserve_siblings_and_match_displayed_counts() {
        let (pool, repo) = fixture().await;
        let folders = repo.indexed_folders().await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].document_count, 1);
        assert_eq!(repo.remove_folder("/vault/a%_").await.unwrap(), 1);
        assert!(repo.indexed_folders().await.unwrap().is_empty());
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM documents ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, vec!["d2", "d3"]);
        let files: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(files, 0); // Indexed documents do not require a separate file resource.
    }

    #[tokio::test]
    async fn folder_removal_failure_restores_watch_entry_and_documents() {
        let (pool, repo) = fixture().await;
        sqlx::query("CREATE TRIGGER reject_document_delete BEFORE DELETE ON documents BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
            .execute(&pool).await.unwrap();
        assert!(repo.remove_folder("/vault/a%_").await.is_err());
        assert_eq!(repo.indexed_folders().await.unwrap()[0].document_count, 1);
        sqlx::query("DROP TRIGGER reject_document_delete")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repo.remove_folder("/vault/a%_").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn activities_use_document_paths_and_handle_pending_documents() {
        let (_, repo) = fixture().await;
        assert!(repo.indexing_activities(0).await.unwrap().is_empty());
        let activities = repo.indexing_activities(usize::MAX).await.unwrap();
        assert_eq!(activities.len(), 3);
        assert_eq!(activities[0].file_path, "/vault/a%_/inside");
        assert_eq!(activities[2].status, "pending");
        assert_eq!(activities[2].timestamp, "");
    }
}
