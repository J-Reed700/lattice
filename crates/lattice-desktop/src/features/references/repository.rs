//! Persistence owner for `passage_references`.
//!
//! Runtime-checked `sqlx` only — no `query!` macros, so the offline `.sqlx`
//! cache stays valid.

use lattice_core::application::contracts::references::PassageReferenceRecord;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

const REFERENCE_COLUMNS: &str = r#"
    id, document_id, chunk_id, file_path, file_name,
    locator, text, title, note, created_at
"#;

/// Decodes one `REFERENCE_COLUMNS` row. The record lives in core, which does
/// not depend on the database driver, so it cannot derive `sqlx::FromRow`.
fn reference_from_row(row: SqliteRow) -> std::result::Result<PassageReferenceRecord, sqlx::Error> {
    Ok(PassageReferenceRecord {
        id: row.try_get("id")?,
        document_id: row.try_get("document_id")?,
        chunk_id: row.try_get("chunk_id")?,
        file_path: row.try_get("file_path")?,
        file_name: row.try_get("file_name")?,
        locator: row.try_get("locator")?,
        text: row.try_get("text")?,
        title: row.try_get("title")?,
        note: row.try_get("note")?,
        created_at: row.try_get("created_at")?,
    })
}

#[derive(Debug, Clone)]
pub struct PassageReferenceRepository {
    db: Database,
}

impl PassageReferenceRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn insert(&self, record: &PassageReferenceRecord) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
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
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            AppError::Database(format!("Failed to insert passage reference: {error}"))
        })?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<PassageReferenceRecord> {
        let query = format!("SELECT {REFERENCE_COLUMNS} FROM passage_references WHERE id = ?");
        let mut read = self.db.begin_read().await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(id)
            .try_map(reference_from_row)
            .fetch_optional(&mut *read)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to fetch passage reference {id}: {error}"))
            })?;
        read.commit().await?;
        row.ok_or_else(|| AppError::NotFound(format!("Passage reference not found: {id}")))
    }

    /// Newest first. `limit` is clamped by the caller.
    pub async fn list(&self, limit: i64) -> Result<Vec<PassageReferenceRecord>> {
        let query = format!(
            "SELECT {REFERENCE_COLUMNS} FROM passage_references \
             ORDER BY created_at DESC LIMIT ?"
        );
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(limit)
            .try_map(reference_from_row)
            .fetch_all(&mut *read)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to list passage references: {error}"))
            })?;
        read.commit().await?;
        Ok(rows)
    }

    /// Newest first, `created_at >= cutoff`. `cutoff` MUST be produced by
    /// `format_db_timestamp` — this table is only ever written with
    /// `now_db_timestamp()`, so the canonical form is safe to compare.
    pub async fn list_created_since(&self, cutoff: &str) -> Result<Vec<PassageReferenceRecord>> {
        let query = format!(
            "SELECT {REFERENCE_COLUMNS} FROM passage_references \
             WHERE created_at >= ? ORDER BY created_at DESC"
        );
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(cutoff)
            .try_map(reference_from_row)
            .fetch_all(&mut *read)
            .await
            .map_err(|error| {
                AppError::Database(format!(
                    "Failed to list passage references since {cutoff}: {error}"
                ))
            })?;
        read.commit().await?;
        Ok(rows)
    }

    /// Annotation-only update. Passing `None` clears the field.
    pub async fn update_annotations(
        &self,
        id: &str,
        title: Option<&str>,
        note: Option<&str>,
    ) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        let result = sqlx::query("UPDATE passage_references SET title = ?, note = ? WHERE id = ?")
            .bind(title)
            .bind(note)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to update passage reference {id}: {error}"))
            })?;
        tx.commit().await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Passage reference not found: {id}"
            )));
        }
        Ok(())
    }

    /// Returns false when nothing matched.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let result = sqlx::query("DELETE FROM passage_references WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to delete passage reference {id}: {error}"))
            })?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn count(&self) -> Result<i64> {
        let mut read = self.db.begin_read().await?;
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM passage_references")
            .fetch_one(&mut *read)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to count passage references: {error}"))
            })?;
        read.commit().await?;
        Ok(count)
    }
}
