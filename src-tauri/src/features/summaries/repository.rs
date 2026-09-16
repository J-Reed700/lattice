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
    async fn delete_for_document(&self, document_id: &str) -> Result<Vec<String>>;

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

    async fn delete_for_document(&self, document_id: &str) -> Result<Vec<String>> {
        self.replace_for_document(document_id, "", &[]).await
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
