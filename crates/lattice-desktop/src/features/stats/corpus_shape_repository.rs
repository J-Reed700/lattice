//! Library-wide type mix, recent growth and newest indexing time, read straight
//! from the document table. A chat's attachments are not part of the library.
//!
//! Deliberately its own narrow port rather than a method on the document
//! repository: nothing else needs a grouped count, and the two queries here are
//! the whole contract.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Local, NaiveTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::features::stats::type_label::type_label;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::Transactions;

/// How far back "recently grown" looks.
pub const GROWTH_WINDOW_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusShape {
    pub total: i64,
    /// Label, count — sorted count descending, then label ascending.
    pub by_type: Vec<(String, i64)>,
    pub grown_last_7_days: i64,
    /// Documents indexed since local midnight.
    pub grown_today: i64,
    /// When the newest library document was indexed, if there is one.
    pub last_indexed_at: Option<String>,
}

fn start_of_local_day(now: DateTime<Local>) -> DateTime<Local> {
    now.date_naive()
        .and_time(NaiveTime::MIN)
        .and_local_timezone(Local)
        .earliest()
        .unwrap_or(now - now.time().signed_duration_since(NaiveTime::MIN))
}

#[async_trait]
pub trait CorpusShapeRepositoryPort: Send + Sync {
    async fn corpus_shape(&self) -> Result<CorpusShape>;
}

pub struct SqliteCorpusShapeRepository {
    pool: SqlitePool,
}

impl SqliteCorpusShapeRepository {
    pub fn new(pool: impl DriverPool) -> Self {
        let pool = pool.sqlx_pool().clone();
        Self { pool }
    }
}

#[async_trait]
impl CorpusShapeRepositoryPort for SqliteCorpusShapeRepository {
    async fn corpus_shape(&self) -> Result<CorpusShape> {
        let mut tx = self
            .pool
            .begin_read()
            .await
            .map_err(|e| AppError::Database(format!("corpus shape begin: {}", e)))?;
        let rows = sqlx::query(
            r#"
            SELECT COALESCE(NULLIF(source_type, ''), 'local') AS source_type,
                   LOWER(COALESCE(file_type, ''))             AS file_type,
                   COUNT(*)                                   AS n
            FROM documents
            WHERE owner_conversation_id IS NULL
            GROUP BY source_type, file_type
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("corpus shape counts: {}", e)))?;

        let mut buckets: HashMap<String, i64> = HashMap::new();
        let mut total: i64 = 0;
        for row in rows {
            let source_type: String = row.try_get("source_type").unwrap_or_default();
            let file_type: String = row.try_get("file_type").unwrap_or_default();
            let n: i64 = row.try_get("n").unwrap_or(0);
            total += n;
            *buckets
                .entry(type_label(&source_type, &file_type))
                .or_insert(0) += n;
        }

        let mut by_type: Vec<(String, i64)> = buckets.into_iter().collect();
        by_type.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let week_cutoff = (Utc::now() - Duration::days(GROWTH_WINDOW_DAYS)).to_rfc3339();
        let day_cutoff = start_of_local_day(Local::now())
            .with_timezone(&Utc)
            .to_rfc3339();
        let growth = sqlx::query(
            r#"
            SELECT COUNT(CASE WHEN indexed_at >= ? THEN 1 END) AS week,
                   COUNT(CASE WHEN indexed_at >= ? THEN 1 END) AS today,
                   MAX(indexed_at)                             AS newest
            FROM documents
            WHERE owner_conversation_id IS NULL
            "#,
        )
        .bind(&week_cutoff)
        .bind(&day_cutoff)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("corpus shape growth: {}", e)))?;
        let grown_last_7_days: i64 = growth.try_get("week").unwrap_or(0);
        let grown_today: i64 = growth.try_get("today").unwrap_or(0);
        let last_indexed_at: Option<String> = growth
            .try_get("newest")
            .map_err(|e| AppError::Database(format!("corpus shape newest: {}", e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("corpus shape commit: {}", e)))?;

        Ok(CorpusShape {
            total,
            by_type,
            grown_last_7_days,
            grown_today,
            last_indexed_at,
        })
    }
}

#[cfg(test)]
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
    async fn a_chat_attachment_is_not_part_of_the_library_summary() {
        let pool = pool().await;
        let now = Utc::now().to_rfc3339();
        insert_doc(&pool, "kept", "pdf", "local", &now).await;
        insert_doc(&pool, "attached", "pdf", "local", &now).await;
        sqlx::query("UPDATE documents SET owner_conversation_id = 'chat-1' WHERE id = 'attached'")
            .execute(&pool)
            .await
            .unwrap();

        let repo = SqliteCorpusShapeRepository::new(pool);
        let shape = repo.corpus_shape().await.unwrap();

        assert_eq!(shape.total, 1);
        assert_eq!(shape.by_type, vec![("PDF".to_string(), 1)]);
        assert_eq!(shape.grown_last_7_days, 1);
    }

    #[tokio::test]
    async fn today_starts_at_local_midnight_and_the_newest_indexing_time_is_reported() {
        let pool = pool().await;
        let now = Local::now();
        let midnight = start_of_local_day(now);
        let stamp = |at: DateTime<Local>| at.with_timezone(&Utc).to_rfc3339();
        let before_midnight = stamp(midnight - Duration::minutes(1));
        let just_now = stamp(now);
        insert_doc(&pool, "today", "pdf", "local", &just_now).await;
        insert_doc(&pool, "yesterday", "pdf", "local", &before_midnight).await;
        insert_doc(
            &pool,
            "chat",
            "pdf",
            "local",
            &stamp(now + Duration::hours(1)),
        )
        .await;
        sqlx::query("UPDATE documents SET owner_conversation_id = 'chat-1' WHERE id = 'chat'")
            .execute(&pool)
            .await
            .unwrap();

        let repo = SqliteCorpusShapeRepository::new(pool);
        let shape = repo.corpus_shape().await.unwrap();

        assert_eq!(shape.total, 2);
        assert_eq!(shape.grown_today, 1);
        assert_eq!(shape.grown_last_7_days, 2);
        assert_eq!(shape.last_indexed_at, Some(just_now));
    }

    #[tokio::test]
    async fn empty_vault_reports_zeroes() {
        let repo = SqliteCorpusShapeRepository::new(pool().await);
        let shape = repo.corpus_shape().await.unwrap();
        assert_eq!(shape.total, 0);
        assert!(shape.by_type.is_empty());
        assert_eq!(shape.grown_last_7_days, 0);
        assert_eq!(shape.grown_today, 0);
        assert_eq!(shape.last_indexed_at, None);
    }
}
