//! Persistence owner for `passage_references`.
//!
//! Runtime-checked `sqlx` only — no `query!` macros, so the offline `.sqlx`
//! cache stays valid.

use crate::shared::error::{AppError, Result};
use sqlx::{FromRow, SqlitePool};

const REFERENCE_COLUMNS: &str = r#"
    id, document_id, chunk_id, file_path, file_name,
    locator, text, title, note, created_at
"#;

#[derive(Debug, Clone, FromRow)]
pub struct PassageReferenceRecord {
    pub id: String,
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub file_path: String,
    pub file_name: String,
    pub locator: Option<String>,
    pub text: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct PassageReferenceRepository {
    pool: SqlitePool,
}

impl PassageReferenceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, record: &PassageReferenceRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO passage_references (
                id, document_id, chunk_id, file_path, file_name,
                locator, text, title, note, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.id)
        .bind(&record.document_id)
        .bind(&record.chunk_id)
        .bind(&record.file_path)
        .bind(&record.file_name)
        .bind(&record.locator)
        .bind(&record.text)
        .bind(&record.title)
        .bind(&record.note)
        .bind(&record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!("Failed to insert passage reference: {error}"))
        })?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<PassageReferenceRecord> {
        let query = format!("SELECT {REFERENCE_COLUMNS} FROM passage_references WHERE id = ?");
        sqlx::query_as::<_, PassageReferenceRecord>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to fetch passage reference {id}: {error}"))
            })?
            .ok_or_else(|| AppError::NotFound(format!("Passage reference not found: {id}")))
    }

    /// Newest first. `limit` is clamped by the caller.
    pub async fn list(&self, limit: i64) -> Result<Vec<PassageReferenceRecord>> {
        let query = format!(
            "SELECT {REFERENCE_COLUMNS} FROM passage_references \
             ORDER BY created_at DESC LIMIT ?"
        );
        sqlx::query_as::<_, PassageReferenceRecord>(&query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to list passage references: {error}"))
            })
    }

    /// Newest first, `created_at >= cutoff`. `cutoff` MUST be produced by
    /// `format_db_timestamp` — this table is only ever written with
    /// `now_db_timestamp()`, so the canonical form is safe to compare.
    pub async fn list_created_since(
        &self,
        cutoff: &str,
        limit: i64,
    ) -> Result<Vec<PassageReferenceRecord>> {
        let query = format!(
            "SELECT {REFERENCE_COLUMNS} FROM passage_references \
             WHERE created_at >= ? ORDER BY created_at DESC LIMIT ?"
        );
        sqlx::query_as::<_, PassageReferenceRecord>(&query)
            .bind(cutoff)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!(
                    "Failed to list passage references since {cutoff}: {error}"
                ))
            })
    }

    /// Annotation-only update. Passing `None` clears the field.
    pub async fn update_annotations(
        &self,
        id: &str,
        title: Option<&str>,
        note: Option<&str>,
    ) -> Result<()> {
        let result = sqlx::query("UPDATE passage_references SET title = ?, note = ? WHERE id = ?")
            .bind(title)
            .bind(note)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to update passage reference {id}: {error}"))
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Passage reference not found: {id}"
            )));
        }
        Ok(())
    }

    /// Returns false when nothing matched.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM passage_references WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to delete passage reference {id}: {error}"))
            })?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn count(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM passage_references")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to count passage references: {error}"))
            })
    }
}
