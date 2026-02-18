use crate::shared::error::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct FileSearchResult {
    pub file_id: String,
    pub file_name: String,
    pub file_extension: Option<String>,
    pub mime_type: String,
    pub storage_path: String,
    pub score: f32,
}

pub struct FileSearch {
    pool: SqlitePool,
}

impl FileSearch {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn search_by_name(&self, query: &str, limit: usize) -> Result<Vec<FileSearchResult>> {
        let query_lower = query.to_lowercase();
        let query_pattern = format!("%{}%", query_lower);

        let results = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
            r#"
            SELECT
                id,
                file_name,
                file_extension,
                mime_type,
                storage_path
            FROM files
            WHERE LOWER(file_name) LIKE ?1
            ORDER BY
                CASE
                    WHEN LOWER(file_name) = ?2 THEN 1
                    WHEN LOWER(file_name) LIKE ?2 || '%' THEN 2
                    WHEN LOWER(file_name) LIKE '%' || ?2 THEN 3
                    ELSE 4
                END,
                file_name
            LIMIT ?3
            "#,
        )
        .bind(&query_pattern)
        .bind(&query_lower)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(results
            .into_iter()
            .enumerate()
            .map(|(idx, (id, name, ext, mime, path))| {
                let score = 1.0 - (idx as f32 * 0.05);

                FileSearchResult {
                    file_id: id,
                    file_name: name,
                    file_extension: ext,
                    mime_type: mime,
                    storage_path: path,
                    score: score.max(0.1),
                }
            })
            .collect())
    }
}
