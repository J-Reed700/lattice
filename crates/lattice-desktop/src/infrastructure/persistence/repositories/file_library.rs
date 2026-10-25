//! Document-repository schema persistence for the indexed-folder library.
use lattice_core::application::contracts::file_library::{IndexedFolder, IndexingActivity};
use lattice_core::application::ports::file_library::FileLibraryPort;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

const FOLDER_COUNTS: &str = "SELECT json_extract(folder.value,'$[0]'), (
    SELECT COUNT(*) FROM documents d
    WHERE d.file_path=json_extract(folder.value,'$[0]')
      AND json_extract(folder.value,'$[0]')<>json_extract(folder.value,'$[1]')
) + (
    SELECT COUNT(*) FROM documents d
    WHERE d.file_path>=json_extract(folder.value,'$[1]') COLLATE NOCASE
      AND d.file_path<json_extract(folder.value,'$[2]') COLLATE NOCASE
) FROM json_each(?) folder";

fn folder_scope(path: &str) -> (&str, String, String) {
    let mut prefix = path.to_owned();
    if !prefix.ends_with(std::path::MAIN_SEPARATOR) {
        prefix.push(std::path::MAIN_SEPARATOR);
    }
    let mut end = prefix.clone();
    end.pop();
    end.push(if std::path::MAIN_SEPARATOR == '\\' {
        ']'
    } else {
        '0'
    });
    (path, prefix, end)
}

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
        if rows.is_empty() {
            tx.commit().await?;
            return Ok(Vec::new());
        }
        // A correlated LIKE expression cannot use SQLite's prefix-range
        // optimization. Explicit bounds use the NOCASE path index and retain
        // LIKE's ASCII case folding without treating % or _ as wildcards.
        let paths: Vec<_> = rows.iter().map(|row| folder_scope(&row.path)).collect();
        let counts: Vec<(String, i64)> = sqlx::query_as(FOLDER_COUNTS)
            .bind(serde_json::to_string(&paths)?)
            .fetch_all(&mut *tx)
            .await?;
        let mut counts: std::collections::HashMap<_, _> = counts.into_iter().collect();
        let mut folders = Vec::with_capacity(rows.len());
        for row in rows {
            let count = counts.remove(&row.path).unwrap_or_default();
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
    // Stored paths and prefix bounds use the host's separator.
    use lattice_core::shared::testing::paths::abs_str;

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
    async fn folder_counts_batch_overlapping_and_empty_roots() {
        let (pool, repo) = fixture().await;
        for (id, path) in [("root", abs_str("vault")), ("empty", abs_str("empty"))] {
            sqlx::query("INSERT INTO watch_folders (id,path,recursive,enabled,created_at) VALUES (?,?,1,1,'2026-01-02')")
                .bind(id).bind(path).execute(&pool).await.unwrap();
        }
        let (folders, reads) =
            crate::tests::common::query_counts::count_reads(repo.indexed_folders()).await;
        let counts: std::collections::HashMap<_, _> = folders
            .unwrap()
            .into_iter()
            .map(|folder| (folder.path, folder.document_count))
            .collect();
        assert_eq!(reads, 2);
        assert_eq!(counts[&watched_folder()], 1);
        assert_eq!(counts[&abs_str("vault")], 3);
        assert_eq!(counts[&abs_str("empty")], 0);
    }

    #[tokio::test]
    async fn prefix_counts_match_literal_like_and_use_index_ranges() {
        use sqlx::Row;
        let (pool, _) = fixture().await;
        let sep = std::path::MAIN_SEPARATOR;
        for (index, path) in [
            watched_folder(),
            format!("{}{sep}", watched_folder()),
            format!("{}{sep}{sep}child", watched_folder()),
            abs_str("vault/A%_/upper"),
            abs_str("é_%/a"),
            abs_str("É_%/b"),
        ]
        .into_iter()
        .enumerate()
        {
            sqlx::query("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,indexed_at,checksum,status) VALUES (?,?,'prefix',1,'2026-01-01','2026-01-01','checksum','pending')")
                .bind(format!("prefix-{index}")).bind(path).execute(&pool).await.unwrap();
        }
        for path in [
            abs_str("vault"),
            abs_str("VAULT"),
            watched_folder(),
            format!("{}{sep}", watched_folder()),
            format!("{}{sep}{sep}", watched_folder()),
            abs_str("é_%"),
            abs_str("É_%"),
            sep.to_string(),
        ] {
            let scope = serde_json::to_string(&[folder_scope(&path)]).unwrap();
            let (_, count): (String, i64) = sqlx::query_as(FOLDER_COUNTS)
                .bind(&scope)
                .fetch_one(&pool)
                .await
                .unwrap();
            let expected: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM documents WHERE file_path=? OR file_path LIKE ? ESCAPE '\\'",
            )
            .bind(&path)
            .bind(lattice_core::shared::persistence::sql_like::directory_prefix_pattern(&path))
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, expected, "prefix range changed counts for {path}");
            let plan = sqlx::query(&format!("EXPLAIN QUERY PLAN {FOLDER_COUNTS}"))
                .bind(&scope)
                .fetch_all(&pool)
                .await
                .unwrap();
            let details: Vec<String> = plan.iter().map(|row| row.get("detail")).collect();
            assert!(
                details.iter().any(|detail| detail
                    .contains("SEARCH d USING COVERING INDEX idx_documents_file_path")),
                "{details:?}"
            );
            assert!(
                details.iter().all(|detail| !detail.contains("SCAN d")),
                "{details:?}"
            );
        }
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
