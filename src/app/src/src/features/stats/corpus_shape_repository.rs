//! Vault-wide type mix and recent growth, read straight from the document table.
//!
//! Deliberately its own narrow port rather than a method on the document
//! repository: nothing else needs a grouped count, and the two queries here are
//! the whole contract.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::{Row, SqlitePool};

use crate::features::stats::type_label::type_label;
use crate::shared::error::{AppError, Result};

/// How far back "recently grown" looks.
pub const GROWTH_WINDOW_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusShape {
    pub total: i64,
    /// Label, count — sorted count descending, then label ascending.
    pub by_type: Vec<(String, i64)>,
    pub grown_last_7_days: i64,
}

#[async_trait]
pub trait CorpusShapeRepositoryPort: Send + Sync {
    async fn corpus_shape(&self) -> Result<CorpusShape>;
}

pub struct SqliteCorpusShapeRepository {
    pool: SqlitePool,
}

impl SqliteCorpusShapeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CorpusShapeRepositoryPort for SqliteCorpusShapeRepository {
    async fn corpus_shape(&self) -> Result<CorpusShape> {
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(NULLIF(source_type, ''), 'local') AS source_type,
                   LOWER(COALESCE(file_type, ''))             AS file_type,
                   COUNT(*)                                   AS n
            FROM documents
            GROUP BY source_type, file_type
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("corpus shape counts: {}", e)))?;

        let mut buckets: HashMap<String, i64> = HashMap::new();
        let mut total: i64 = 0;
        for row in rows {
            let source_type: String = row.try_get("source_type").unwrap_or_default();
            let file_type: String = row.try_get("file_type").unwrap_or_default();
            let n: i64 = row.try_get("n").unwrap_or(0);
            total += n;
            *buckets.entry(type_label(&source_type, &file_type)).or_insert(0) += n;
        }

        let mut by_type: Vec<(String, i64)> = buckets.into_iter().collect();
        by_type.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let cutoff = (Utc::now() - Duration::days(GROWTH_WINDOW_DAYS)).to_rfc3339();
        let grown_last_7_days: i64 =
            sqlx::query("SELECT COUNT(*) AS n FROM documents WHERE indexed_at >= ?")
                .bind(&cutoff)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("corpus shape growth: {}", e)))?
                .try_get("n")
                .unwrap_or(0);

        Ok(CorpusShape {
            total,
            by_type,
            grown_last_7_days,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_doc(
        pool: &SqlitePool,
        id: &str,
        file_type: &str,
        source_type: &str,
        indexed_at: &str,
    ) {
        sqlx::query(
            r#"
            INSERT INTO documents
                (id, file_path, file_name, file_type, size_bytes, modified_at,
                 indexed_at, checksum, status, source_type)
            VALUES (?, ?, ?, ?, 0, ?, ?, 'sum', 'indexed', ?)
            "#,
        )
        .bind(id)
        .bind(format!("/tmp/{}.{}", id, file_type))
        .bind(format!("{}.{}", id, file_type))
        .bind(file_type)
        .bind(indexed_at)
        .bind(indexed_at)
        .bind(source_type)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn counts_by_type_in_descending_order() {
        let pool = pool().await;
        let now = Utc::now().to_rfc3339();
        for i in 0..3 {
            insert_doc(&pool, &format!("pdf{i}"), "pdf", "local", &now).await;
        }
        for i in 0..2 {
            insert_doc(&pool, &format!("md{i}"), "md", "local", &now).await;
        }
        insert_doc(&pool, "web0", "html", "web", &now).await;

        let repo = SqliteCorpusShapeRepository::new(pool);
        let shape = repo.corpus_shape().await.unwrap();

        assert_eq!(shape.total, 6);
        assert_eq!(
            shape.by_type,
            vec![
                ("PDF".to_string(), 3),
                ("Markdown".to_string(), 2),
                ("Web".to_string(), 1),
            ]
        );
    }

    #[tokio::test]
    async fn growth_counts_only_the_last_seven_days() {
        let pool = pool().await;
        let recent = (Utc::now() - Duration::days(3)).to_rfc3339();
        let old = (Utc::now() - Duration::days(30)).to_rfc3339();
        insert_doc(&pool, "recent", "pdf", "local", &recent).await;
        insert_doc(&pool, "old", "pdf", "local", &old).await;

        let repo = SqliteCorpusShapeRepository::new(pool);
        let shape = repo.corpus_shape().await.unwrap();

        assert_eq!(shape.total, 2);
        assert_eq!(shape.grown_last_7_days, 1);
    }

    #[tokio::test]
    async fn empty_vault_reports_zeroes() {
        let repo = SqliteCorpusShapeRepository::new(pool().await);
        let shape = repo.corpus_shape().await.unwrap();
        assert_eq!(shape.total, 0);
        assert!(shape.by_type.is_empty());
        assert_eq!(shape.grown_last_7_days, 0);
    }
}
