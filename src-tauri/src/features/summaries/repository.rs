//! SQLite persistence for generated summaries.
//!
//! SQLite is authoritative for summaries exactly as it is for chunks: the
//! vector index is a derived artifact that can be rebuilt, and a hit whose row
//! has gone (document deleted, identity rotated) is dropped at read time
//! rather than trusted.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use crate::features::summaries::entity::{DocumentSummary, SummaryLevel};
use crate::shared::error::{AppError, Result};

/// Port for persisting and reading document/section summaries.
///
/// Narrow on purpose: the summaries feature is the only writer, so the
/// contract stays next to its implementation.
#[async_trait]
pub trait SummaryRepositoryPort: Send + Sync {
    /// Replace every summary for one document under one identity.
    ///
    /// Returns the ids of the rows that were removed, so the caller can drop
    /// their vectors from the summary index in the same pass.
    async fn replace_for_document(
        &self,
        document_id: &str,
        model_identity: &str,
        summaries: &[DocumentSummary],
    ) -> Result<Vec<String>>;

    /// All summaries for the given documents under one identity.
    async fn list_for_documents(
        &self,
        document_ids: &HashSet<String>,
        model_identity: &str,
    ) -> Result<Vec<DocumentSummary>>;

    /// Summary ids and their rows, for resolving vector hits.
    async fn find_by_ids(&self, summary_ids: &[String]) -> Result<Vec<DocumentSummary>>;

    /// Delete every summary for a document, returning the removed ids.
    async fn delete_for_document(
        &self,
        document_id: &str,
        model_identity: &str,
    ) -> Result<Vec<String>>;

    /// Vector identifiers whose database rows were already removed but whose
    /// derived-index cleanup has not been acknowledged.
    async fn pending_vector_cleanup(&self, model_identity: &str, limit: u32)
        -> Result<Vec<String>>;

    /// Acknowledge only after the matching index removal succeeds.
    async fn acknowledge_vector_cleanup(
        &self,
        model_identity: &str,
        vector_ids: &[String],
    ) -> Result<()>;

    /// Drop rows written under any other embedding identity. Their vectors
    /// live in a different index file and can never be searched again.
    async fn prune_other_identities(&self, model_identity: &str) -> Result<usize>;

    /// How many summaries exist under an identity. Zero means the tier is off.
    async fn count_for_identity(&self, model_identity: &str) -> Result<i64>;
}

/// Document-level summary text keyed by document id, for catalog augmentation.
pub fn document_level_text(summaries: &[DocumentSummary]) -> HashMap<String, String> {
    summaries
        .iter()
        .filter(|summary| summary.level == SummaryLevel::Document)
        .map(|summary| (summary.document_id.clone(), summary.summary_text.clone()))
        .collect()
}

pub struct SqliteSummaryRepository {
    pool: SqlitePool,
}

impl SqliteSummaryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_summary(row: &sqlx::sqlite::SqliteRow) -> Result<DocumentSummary> {
        let level: String = row
            .try_get("level")
            .map_err(|e| AppError::Database(format!("row.level: {e}")))?;
        let created_at: String = row
            .try_get("created_at")
            .map_err(|e| AppError::Database(format!("row.created_at: {e}")))?;
        Ok(DocumentSummary {
            id: row
                .try_get("id")
                .map_err(|e| AppError::Database(format!("row.id: {e}")))?,
            document_id: row
                .try_get("document_id")
                .map_err(|e| AppError::Database(format!("row.document_id: {e}")))?,
            section: row
                .try_get("section")
                .map_err(|e| AppError::Database(format!("row.section: {e}")))?,
            level: SummaryLevel::parse(&level).ok_or_else(|| {
                AppError::Database(format!("unknown document_summaries.level '{level}'"))
            })?,
            summary_text: row
                .try_get("summary_text")
                .map_err(|e| AppError::Database(format!("row.summary_text: {e}")))?,
            model_identity: row
                .try_get("model_identity")
                .map_err(|e| AppError::Database(format!("row.model_identity: {e}")))?,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map_err(|e| AppError::Parsing(format!("created_at parse: {e}")))?
                .with_timezone(&Utc),
        })
    }
}

const SELECT_COLUMNS: &str =
    "SELECT id, document_id, section, level, summary_text, model_identity, created_at FROM document_summaries";

#[async_trait]
impl SummaryRepositoryPort for SqliteSummaryRepository {
    async fn replace_for_document(
        &self,
        document_id: &str,
        model_identity: &str,
        summaries: &[DocumentSummary],
    ) -> Result<Vec<String>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("begin summary tx: {e}")))?;
        let removed: Vec<String> =
            sqlx::query_scalar("SELECT id FROM document_summaries WHERE document_id = ?")
                .bind(document_id)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("read stale summaries: {e}")))?;
        // Tombstones and row replacement commit together. If indexing later
        // fails, a retry still has the exact keys needed to reclaim vectors.
        if !removed.is_empty() {
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT OR IGNORE INTO summary_vector_cleanup(vector_id, model_identity) SELECT 'summary_' || id, model_identity FROM document_summaries WHERE document_id = ",
            );
            query.push_bind(document_id);
            query.push(" AND model_identity = ");
            query.push_bind(model_identity);
            query
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("record stale summary vectors: {e}")))?;
        }
        sqlx::query("DELETE FROM document_summaries WHERE document_id = ?")
            .bind(document_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("delete summaries: {e}")))?;
        for summary in summaries {
            sqlx::query(
                "INSERT INTO document_summaries (id, document_id, section, level, summary_text, model_identity, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&summary.id)
            .bind(&summary.document_id)
            .bind(&summary.section)
            .bind(summary.level.as_str())
            .bind(&summary.summary_text)
            .bind(model_identity)
            .bind(summary.created_at.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("insert summary: {e}")))?;
        }
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("commit summaries: {e}")))?;
        Ok(removed)
    }

    async fn list_for_documents(
        &self,
        document_ids: &HashSet<String>,
        model_identity: &str,
    ) -> Result<Vec<DocumentSummary>> {
        if document_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(SELECT_COLUMNS);
        query.push(" WHERE model_identity = ");
        query.push_bind(model_identity.to_string());
        query.push(" AND document_id IN (");
        let mut ids: Vec<&String> = document_ids.iter().collect();
        ids.sort();
        let mut separated = query.separated(", ");
        for id in ids {
            separated.push_bind(id.clone());
        }
        query.push(") ORDER BY document_id, level, section");
        let rows = query
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("list summaries: {e}")))?;
        rows.iter().map(Self::row_to_summary).collect()
    }

    async fn find_by_ids(&self, summary_ids: &[String]) -> Result<Vec<DocumentSummary>> {
        if summary_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(SELECT_COLUMNS);
        query.push(" WHERE id IN (");
        let mut separated = query.separated(", ");
        for id in summary_ids {
            separated.push_bind(id.clone());
        }
        query.push(")");
        let rows = query
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("find summaries: {e}")))?;
        rows.iter().map(Self::row_to_summary).collect()
    }

    async fn delete_for_document(
        &self,
        document_id: &str,
        model_identity: &str,
    ) -> Result<Vec<String>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("begin summary delete tx: {e}")))?;
        let removed: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM document_summaries WHERE document_id = ? AND model_identity = ?",
        )
        .bind(document_id)
        .bind(model_identity)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("read document summaries to delete: {e}")))?;
        if !removed.is_empty() {
            sqlx::query(
                "INSERT OR IGNORE INTO summary_vector_cleanup(vector_id, model_identity) SELECT 'summary_' || id, model_identity FROM document_summaries WHERE document_id = ? AND model_identity = ?",
            )
            .bind(document_id)
            .bind(model_identity)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("record deleted summary vectors: {e}")))?;
        }
        sqlx::query("DELETE FROM document_summaries WHERE document_id = ? AND model_identity = ?")
            .bind(document_id)
            .bind(model_identity)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("delete document summaries: {e}")))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("commit summary delete: {e}")))?;
        Ok(removed)
    }

    async fn pending_vector_cleanup(
        &self,
        model_identity: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        sqlx::query_scalar(
            "SELECT vector_id FROM summary_vector_cleanup WHERE model_identity = ? ORDER BY requested_at, vector_id LIMIT ?",
        )
        .bind(model_identity)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("read summary vector cleanup: {e}")))
    }

    async fn acknowledge_vector_cleanup(
        &self,
        model_identity: &str,
        vector_ids: &[String],
    ) -> Result<()> {
        if vector_ids.is_empty() {
            return Ok(());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "DELETE FROM summary_vector_cleanup WHERE model_identity = ",
        );
        query.push_bind(model_identity);
        query.push(" AND vector_id IN (");
        let mut separated = query.separated(", ");
        for id in vector_ids {
            separated.push_bind(id);
        }
        query.push(")");
        query
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("acknowledge summary vector cleanup: {e}")))?;
        Ok(())
    }

    async fn prune_other_identities(&self, model_identity: &str) -> Result<usize> {
        let result = sqlx::query("DELETE FROM document_summaries WHERE model_identity != ?")
            .bind(model_identity)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("prune summaries: {e}")))?;
        Ok(result.rows_affected() as usize)
    }

    async fn count_for_identity(&self, model_identity: &str) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM document_summaries WHERE model_identity = ?")
            .bind(model_identity)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("count summaries: {e}")))
    }
}

/// Queue identities only; repeated indexing replaces one revision atomically.
pub(crate) async fn enqueue_work(
    pool: &sqlx::SqlitePool,
    id: &str,
) -> crate::shared::error::Result<()> {
    sqlx::query("INSERT INTO summary_work_queue(document_id, revision, requested_at) SELECT id, lower(hex(randomblob(16))), unixepoch() FROM documents WHERE id = ? ON CONFLICT(document_id) DO UPDATE SET revision = excluded.revision, requested_at = excluded.requested_at, retry_at = 0")
        .bind(id).execute(pool).await?;
    Ok(())
}
pub(crate) async fn next_work(
    pool: &sqlx::SqlitePool,
) -> crate::shared::error::Result<Option<(String, String)>> {
    Ok(sqlx::query_as("SELECT document_id, revision FROM summary_work_queue WHERE retry_at <= unixepoch() ORDER BY requested_at, document_id LIMIT 1").fetch_optional(pool).await?)
}
pub(crate) async fn acknowledge_work(
    pool: &sqlx::SqlitePool,
    id: &str,
    revision: &str,
) -> crate::shared::error::Result<()> {
    sqlx::query("DELETE FROM summary_work_queue WHERE document_id = ? AND revision = ?")
        .bind(id)
        .bind(revision)
        .execute(pool)
        .await?;
    Ok(())
}
pub(crate) async fn remove_work(
    pool: &sqlx::SqlitePool,
    id: &str,
) -> crate::shared::error::Result<()> {
    sqlx::query("DELETE FROM summary_work_queue WHERE document_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
pub(crate) async fn document_id_at_path(
    pool: &sqlx::SqlitePool,
    path: &std::path::Path,
) -> crate::shared::error::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM documents WHERE file_path = ?")
            .bind(path.to_string_lossy().as_ref())
            .fetch_optional(pool)
            .await?,
    )
}

pub(crate) async fn defer_work(
    pool: &sqlx::SqlitePool,
    id: &str,
    revision: &str,
) -> crate::shared::error::Result<()> {
    sqlx::query("UPDATE summary_work_queue SET retry_at = unixepoch() + 60 WHERE document_id = ? AND revision = ?").bind(id).bind(revision).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod work_queue_tests {
    use super::*;

    #[tokio::test]
    async fn work_is_coalesced_and_revision_guarded_and_deleted_with_document() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE documents(id TEXT PRIMARY KEY, status TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/20260927010000_summary_work_queue.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO documents VALUES ('doc', 'pending')")
            .execute(&pool)
            .await
            .unwrap();
        enqueue_work(&pool, "doc").await.unwrap();
        let (id, revision) = next_work(&pool).await.unwrap().unwrap();
        for _ in 0..1000 {
            enqueue_work(&pool, "doc").await.unwrap();
        }
        acknowledge_work(&pool, &id, &revision).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM summary_work_queue")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        sqlx::query("DELETE FROM documents WHERE id = 'doc'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(next_work(&pool).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn indexed_state_durably_enqueues_summary_and_rolls_back_with_transaction() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE documents(id TEXT PRIMARY KEY, status TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/20260927010000_summary_work_queue.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO documents VALUES ('rolled-back', 'indexed')")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        assert!(next_work(&pool).await.unwrap().is_none());

        sqlx::query("INSERT INTO documents VALUES ('doc', 'pending')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET status = 'indexed' WHERE id = 'doc'")
            .execute(&pool)
            .await
            .unwrap();
        let (id, revision) = next_work(&pool).await.unwrap().unwrap();
        assert_eq!(id, "doc");
        assert!(!revision.is_empty());
    }

    #[tokio::test]
    async fn vector_cleanup_tombstone_survives_failed_removal_and_clears_after_ack() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE documents(id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE document_summaries(id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, section TEXT, level TEXT NOT NULL, summary_text TEXT NOT NULL, model_identity TEXT NOT NULL, created_at TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/20260927030000_summary_vector_cleanup.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO documents VALUES ('doc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_summaries VALUES ('sum-1', 'doc', NULL, 'document', 'summary', 'model-a', '2026-09-27T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        let repository = SqliteSummaryRepository::new(pool.clone());

        repository
            .delete_for_document("doc", "model-a")
            .await
            .unwrap();
        let retry_after_failed_index_remove = repository
            .pending_vector_cleanup("model-a", 256)
            .await
            .unwrap();
        assert_eq!(retry_after_failed_index_remove, ["summary_sum-1"]);

        // Repeating the drain is safe; only successful index removal is
        // followed by acknowledgement of the durable identifier.
        repository
            .acknowledge_vector_cleanup("model-a", &retry_after_failed_index_remove)
            .await
            .unwrap();
        assert!(repository
            .pending_vector_cleanup("model-a", 256)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn cascade_delete_captures_old_summary_vector_ids() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE documents(id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE document_summaries(id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, section TEXT, level TEXT NOT NULL, summary_text TEXT NOT NULL, model_identity TEXT NOT NULL, created_at TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/20260927030000_summary_vector_cleanup.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO documents VALUES ('doc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_summaries VALUES ('sum-2', 'doc', NULL, 'document', 'summary', 'model-a', '2026-09-27T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM documents WHERE id = 'doc'")
            .execute(&pool)
            .await
            .unwrap();

        let repository = SqliteSummaryRepository::new(pool);
        assert_eq!(
            repository
                .pending_vector_cleanup("model-a", 256)
                .await
                .unwrap(),
            ["summary_sum-2"]
        );
    }
}
