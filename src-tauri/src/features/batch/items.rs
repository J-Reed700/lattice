//! Every read and write of `batch_import_items`: one row per file or URL of a
//! batch import, keyed by its job. The job runtime owns the job's status; an
//! item's status says how far that one file or URL got.
use crate::shared::error::{AppError, Result};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl ItemState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "pending" => Self::Pending,
            "processing" => Self::Processing,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => {
                return Err(AppError::Database(format!(
                    "Invalid batch item status: {other}"
                )))
            }
        })
    }
}

/// One file or URL of a batch import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchItem {
    pub id: String,
    /// Where the user put it in the batch; files of an ordered source keep
    /// this as their reading position.
    pub position: u32,
    /// A file path or a URL.
    pub target: String,
    /// The document it imported. A failed item keeps the document it
    /// committed before a later step failed, so its retry can finish that step.
    pub document_id: Option<String>,
    pub state: ItemState,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemCounts {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
}

impl ItemCounts {
    pub fn of(items: &[BatchItem]) -> Self {
        let tally = |state: ItemState| {
            u32::try_from(items.iter().filter(|item| item.state == state).count())
                .unwrap_or(u32::MAX)
        };
        Self {
            total: u32::try_from(items.len()).unwrap_or(u32::MAX),
            completed: tally(ItemState::Completed),
            failed: tally(ItemState::Failed),
        }
    }

    pub fn processed(&self) -> u32 {
        self.completed + self.failed
    }
}

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn parse(row: &SqliteRow) -> Result<BatchItem> {
    Ok(BatchItem {
        id: row.get("id"),
        position: u32::try_from(row.get::<i64, _>("position")).unwrap_or(u32::MAX),
        target: row.get("target"),
        document_id: row.get("document_id"),
        state: ItemState::parse(row.get::<String, _>("status").as_str())?,
        error_message: row.get("error_message"),
    })
}

fn count(row: &SqliteRow, column: &str) -> u32 {
    u32::try_from(row.get::<i64, _>(column)).unwrap_or(u32::MAX)
}

#[derive(Clone)]
pub struct BatchItems {
    pool: SqlitePool,
}

impl BatchItems {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Saves a new import's items, pending, in the order given.
    pub async fn insert_in(
        connection: &mut SqliteConnection,
        job_id: &str,
        targets: &[String],
    ) -> Result<()> {
        for (position, target) in targets.iter().enumerate() {
            sqlx::query("INSERT INTO batch_import_items(id,job_id,position,target,status) VALUES(?,?,?,?,'pending')")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(job_id)
                .bind(i64::try_from(position).unwrap_or(i64::MAX))
                .bind(target)
                .execute(&mut *connection)
                .await
                .map_err(db)?;
        }
        Ok(())
    }

    /// A job's items in batch order.
    pub async fn list_in(
        connection: &mut SqliteConnection,
        job_id: &str,
    ) -> Result<Vec<BatchItem>> {
        sqlx::query("SELECT * FROM batch_import_items WHERE job_id=? ORDER BY position")
            .bind(job_id)
            .fetch_all(connection)
            .await
            .map_err(db)?
            .iter()
            .map(parse)
            .collect()
    }

    pub async fn list(&self, job_id: &str) -> Result<Vec<BatchItem>> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::list_in(&mut connection, job_id).await
    }

    /// How many items each job has, and how many finished each way.
    pub async fn counts(&self, job_ids: &[String]) -> Result<HashMap<String, ItemCounts>> {
        let ids = serde_json::to_string(job_ids)?;
        let rows = sqlx::query("SELECT job_id, count(*) AS total, sum(status='completed') AS completed, sum(status='failed') AS failed FROM batch_import_items WHERE job_id IN (SELECT value FROM json_each(?)) GROUP BY job_id")
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows
            .iter()
            .map(|row| {
                (
                    row.get("job_id"),
                    ItemCounts {
                        total: count(row, "total"),
                        completed: count(row, "completed"),
                        failed: count(row, "failed"),
                    },
                )
            })
            .collect())
    }

    pub async fn counts_of(&self, job_id: &str) -> Result<ItemCounts> {
        Ok(self
            .counts(&[job_id.to_string()])
            .await?
            .remove(job_id)
            .unwrap_or_default())
    }

    /// Returns the items an attempt left in flight to the queue: the process
    /// ended before it recorded how they went.
    pub async fn reset_processing(&self, job_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE batch_import_items SET status='pending' WHERE job_id=? AND status='processing'",
        )
        .bind(job_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    /// Takes the job's next pending item, in batch order. An item the user
    /// cancelled in the meantime is never taken.
    pub async fn claim_next(&self, job_id: &str) -> Result<Option<BatchItem>> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let next = sqlx::query("SELECT * FROM batch_import_items WHERE job_id=? AND status='pending' ORDER BY position LIMIT 1")
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .as_ref()
            .map(parse)
            .transpose()?;
        let Some(mut item) = next else {
            tx.commit().await.map_err(db)?;
            return Ok(None);
        };
        sqlx::query(
            "UPDATE batch_import_items SET status='processing' WHERE id=? AND status='pending'",
        )
        .bind(&item.id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        item.state = ItemState::Processing;
        Ok(Some(item))
    }

    /// Records how an item went.
    pub async fn finish(
        &self,
        item_id: &str,
        state: ItemState,
        document_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE batch_import_items SET status=?, document_id=coalesce(?,document_id), error_message=?, processed_at=? WHERE id=?")
            .bind(state.as_str())
            .bind(document_id)
            .bind(error_message)
            .bind(now())
            .bind(item_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Puts back an item whose work stopped part-way: pending when the job
    /// resumes later, cancelled when the user cancelled it.
    pub async fn release(&self, item_id: &str, state: ItemState) -> Result<()> {
        sqlx::query("UPDATE batch_import_items SET status=? WHERE id=? AND status='processing'")
            .bind(state.as_str())
            .bind(item_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Cancels every item that has not finished, including the one in flight:
    /// a worker that still finishes it records how it actually went.
    pub async fn cancel_unfinished_in(
        connection: &mut SqliteConnection,
        job_id: &str,
    ) -> Result<usize> {
        let cancelled = sqlx::query("UPDATE batch_import_items SET status='cancelled', processed_at=? WHERE job_id=? AND status IN ('pending','processing')")
            .bind(now())
            .bind(job_id)
            .execute(connection)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(usize::try_from(cancelled).unwrap_or(usize::MAX))
    }

    /// Hands a job's items to its retry.
    pub async fn move_to_in(
        connection: &mut SqliteConnection,
        from_job: &str,
        to_job: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE batch_import_items SET job_id=? WHERE job_id=?")
            .bind(to_job)
            .bind(from_job)
            .execute(connection)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// Queues failed items again: all of them, or the one `item_id` names,
    /// optionally pointed at a replacement target. Returns how many.
    pub async fn requeue_failed_in(
        connection: &mut SqliteConnection,
        job_id: &str,
        item_id: Option<&str>,
        replacement: Option<&str>,
    ) -> Result<usize> {
        let requeued = sqlx::query("UPDATE batch_import_items SET status='pending', target=coalesce(?,target), error_message=NULL, processed_at=NULL WHERE job_id=? AND status='failed' AND (? IS NULL OR id=?)")
            .bind(replacement)
            .bind(job_id)
            .bind(item_id)
            .bind(item_id)
            .execute(connection)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(usize::try_from(requeued).unwrap_or(usize::MAX))
    }
}
