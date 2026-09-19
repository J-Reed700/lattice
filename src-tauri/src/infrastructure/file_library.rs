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
    // Stored paths must be spelled with the host separator. `indexed_folders`
    // builds its `LIKE` pattern with `directory_prefix_pattern`, which appends
    // `std::path::MAIN_SEPARATOR` — a backslash on Windows — so POSIX-spelled
    // fixture rows would be asked to match `\`-separated children and count
    // zero documents.
    use crate::shared::test_paths::abs_str;

    async fn fixture() -> (SqlitePool, SqliteFileLibrary) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO watch_folders (id, path, recursive, enabled, last_scan, created_at)
                VALUES ('watch', ?1, 1, 1, NULL, '2026-01-01')",
        )
        .bind(watched_folder())
        .execute(&pool)
        .await
        .unwrap();
        // Bound rather than inlined: a Windows path is full of backslashes,
        // which are far easier to get wrong inside a SQL literal than as
        // parameters.
        for (id, path, name, modified, indexed, status) in [
            (
                "d1",
                inside_document(),
                "inside",
                "2026-01-01",
                "2026-01-01",
                "indexed",
            ),
            (
                "d2",
                abs_str("vault/abc/sibling"),
                "sibling",
                "2026-01-01",
                "",
                "pending",
            ),
            (
                "d3",
                abs_str("vault/a%_-sibling/other"),
                "other",
                "2025-01-01",
                "2025-01-01",
                "indexed",
            ),
        ] {
            sqlx::query(
                "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, indexed_at, checksum, status)
                    VALUES (?1, ?2, ?3, 1, ?4, ?5, 'checksum', ?6)",
            )
            .bind(id)
            .bind(path)
            .bind(name)
            .bind(modified)
            .bind(indexed)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        }
        let repo = SqliteFileLibrary::new(pool.clone());
        (pool, repo)
    }

    /// The watched folder, whose name deliberately contains both `LIKE`
    /// metacharacters.
    fn watched_folder() -> String {
        abs_str("vault/a%_")
    }

    fn inside_document() -> String {
        abs_str("vault/a%_/inside")
    }

    /// The folder pattern must match documents inside the `a%_` folder without
    /// dragging in its `abc` or `a%_-sibling` neighbours: `%` and `_` in the
    /// path are literal, so the LIKE pattern has to escape them.
    #[tokio::test]
    async fn document_counts_match_the_folder_and_exclude_siblings() {
        let (_, repo) = fixture().await;
        let folders = repo.indexed_folders().await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].path, watched_folder());
        assert_eq!(folders[0].document_count, 1);
    }

    #[tokio::test]
    async fn activities_use_document_paths_and_handle_pending_documents() {
        let (_, repo) = fixture().await;
        assert!(repo.indexing_activities(0).await.unwrap().is_empty());
        let activities = repo.indexing_activities(usize::MAX).await.unwrap();
        assert_eq!(activities.len(), 3);
        assert_eq!(activities[0].file_path, inside_document());
        assert_eq!(activities[2].status, "pending");
        assert_eq!(activities[2].timestamp, "");
    }
}
