use crate::application::ports::batch_job_repository_port::{
    BatchJobItem, BatchJobItemStatus, BatchJobStatus, BatchJobSummary,
};
use crate::shared::error::AppError;
use sqlx::SqliteConnection;
use uuid::Uuid;

pub async fn get_job_options(
    conn: &mut SqliteConnection,
    job_id: &str,
) -> Result<Option<String>, AppError> {
    Ok(
        sqlx::query_scalar("SELECT options FROM batch_jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(conn)
            .await?,
    )
}

/// Caller owns a transaction; every validation failure rolls the claim back.
pub async fn requeue_failed_files(
    conn: &mut SqliteConnection,
    job_id: &str,
    item_id: Option<&str>,
    replacement_path: Option<&str>,
) -> Result<usize, AppError> {
    if replacement_path.is_some() && item_id.is_none() {
        return Err(AppError::InvalidInput(
            "Choose one failed file to replace".into(),
        ));
    }
    let claimed = sqlx::query("UPDATE batch_jobs SET status = 'pending', completed_at = NULL, error_message = NULL WHERE id = ? AND job_type = 'file_import' AND status IN ('completed', 'failed', 'cancelled')")
        .bind(job_id).execute(&mut *conn).await?.rows_affected();
    if claimed != 1 {
        return Err(AppError::InvalidInput(
            "This import is already running or is unavailable. Refresh its status before retrying."
                .into(),
        ));
    }
    let count = sqlx::query("UPDATE batch_job_items SET status = 'pending', item_url = COALESCE(?, item_url), error_message = NULL, processed_at = NULL WHERE job_id = ? AND status = 'failed' AND (? IS NULL OR id = ?)")
        .bind(replacement_path).bind(job_id).bind(item_id).bind(item_id)
        .execute(&mut *conn).await?.rows_affected();
    if count == 0 {
        return Err(AppError::InvalidInput(
            "No failed files to retry in this import".into(),
        ));
    }
    sqlx::query("UPDATE batch_jobs SET failed_items = (SELECT count(*) FROM batch_job_items WHERE job_id = ? AND status = 'failed'), completed_items = (SELECT count(*) FROM batch_job_items WHERE job_id = ? AND status = 'completed'), progress = (SELECT CAST(count(*) AS REAL) FROM batch_job_items WHERE job_id = ? AND status IN ('completed', 'failed')) / MAX(total_items, 1) WHERE id = ?")
        .bind(job_id).bind(job_id).bind(job_id).bind(job_id).execute(conn).await?;
    Ok(count as usize)
}

// Database row DTOs
#[derive(Debug, sqlx::FromRow)]
struct BatchJobRow {
    id: String,
    job_type: String,
    status: String,
    total_items: i64,
    completed_items: i64,
    failed_items: i64,
    progress: f64,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    error_message: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchJobItemRow {
    id: String,
    item_url: String,
    document_id: Option<String>,
    status: String,
    error_message: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchJobSummaryRow {
    id: String,
    job_type: String,
    status: String,
    total_items: i64,
    completed_items: i64,
    failed_items: i64,
    progress: f64,
    created_at: String,
    completed_at: Option<String>,
}

pub async fn create_batch_job(
    conn: &mut SqliteConnection,
    job_id: &str,
    job_type: &str,
    total_items: i64,
    options: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO batch_jobs (id, job_type, status, total_items, completed_items, failed_items, progress, options)
        VALUES (?1, ?2, 'pending', ?3, 0, 0, 0.0, ?4)
        "#,
        job_id,
        job_type,
        total_items,
        options
    )
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to create batch job: {}", e)))?;

    Ok(())
}

pub async fn create_batch_items(
    conn: &mut SqliteConnection,
    job_id: &str,
    urls: Vec<String>,
) -> Result<(), AppError> {
    for url in urls {
        let item_id = Uuid::new_v4().to_string();
        sqlx::query!(
            r#"
            INSERT INTO batch_job_items (id, job_id, item_url, status)
            VALUES (?1, ?2, ?3, 'pending')
            "#,
            item_id,
            job_id,
            url
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create batch item: {}", e)))?;
    }

    Ok(())
}

pub async fn update_job_status(
    conn: &mut SqliteConnection,
    job_id: &str,
    status: &str,
    started_at: Option<String>,
    completed_at: Option<String>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        UPDATE batch_jobs
        SET status = ?1,
            started_at = COALESCE(?2, started_at),
            completed_at = COALESCE(?3, completed_at)
        WHERE id = ?4
        "#,
        status,
        started_at,
        completed_at,
        job_id
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to update job status: {}", e)))?;

    Ok(())
}

pub async fn update_progress(
    conn: &mut SqliteConnection,
    job_id: &str,
    completed: i64,
    failed: i64,
    progress: f64,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        UPDATE batch_jobs
        SET completed_items = ?1,
            failed_items = ?2,
            progress = ?3
        WHERE id = ?4
        "#,
        completed,
        failed,
        progress,
        job_id
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to update progress: {}", e)))?;

    Ok(())
}

pub async fn update_item_status(
    conn: &mut SqliteConnection,
    item_id: &str,
    status: &str,
    document_id: Option<&str>,
    error_message: Option<&str>,
) -> Result<(), AppError> {
    let processed_at = if status != "pending" {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };

    sqlx::query!(
        r#"
        UPDATE batch_job_items
        SET status = ?1,
            document_id = COALESCE(?2, document_id),
            error_message = ?3,
            processed_at = COALESCE(?4, processed_at)
        WHERE id = ?5
        "#,
        status,
        document_id,
        error_message,
        processed_at,
        item_id
    )
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to update item status: {}", e)))?;

    Ok(())
}

pub async fn get_batch_job(
    conn: &mut SqliteConnection,
    job_id: &str,
) -> Result<BatchJobStatus, AppError> {
    let job = sqlx::query_as::<_, BatchJobRow>(
        r#"
        SELECT id, job_type, status, total_items, completed_items, failed_items, progress,
               created_at, started_at, completed_at, error_message
        FROM batch_jobs
        WHERE id = ?
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch batch job: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("Batch job not found: {}", job_id)))?;

    let items = sqlx::query_as::<_, BatchJobItemRow>(
        r#"
        SELECT id, item_url, document_id, status, error_message
        FROM batch_job_items
        WHERE job_id = ?
        ORDER BY created_at ASC, rowid ASC
        "#,
    )
    .bind(job_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch batch items: {}", e)))?
    .into_iter()
    .map(|row| BatchJobItemStatus {
        id: row.id,
        url: row.item_url,
        document_id: row.document_id,
        status: row.status,
        error_message: row.error_message,
    })
    .collect();

    Ok(BatchJobStatus {
        id: job.id,
        job_type: job.job_type,
        status: job.status,
        total_items: job.total_items,
        completed_items: job.completed_items,
        failed_items: job.failed_items,
        progress: job.progress,
        created_at: job.created_at,
        started_at: job.started_at,
        completed_at: job.completed_at,
        error_message: job.error_message,
        items,
    })
}

pub async fn get_pending_items(
    conn: &mut SqliteConnection,
    job_id: &str,
) -> Result<Vec<BatchJobItem>, AppError> {
    let items = sqlx::query_as::<_, (String, String)>("SELECT id, item_url FROM batch_job_items WHERE job_id = ? AND status = 'pending' ORDER BY created_at ASC, rowid ASC").bind(job_id)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch pending items: {}", e)))?
    .into_iter()
    .map(|row| BatchJobItem {
        id: row.0,
        url: row.1,
    })
    .collect();

    Ok(items)
}

pub async fn cancel_pending_items(
    conn: &mut SqliteConnection,
    job_id: &str,
) -> Result<usize, AppError> {
    let result = sqlx::query!(
        r#"
        UPDATE batch_job_items
        SET status = 'cancelled'
        WHERE job_id = ?1 AND status = 'pending'
        "#,
        job_id
    )
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to cancel items: {}", e)))?;

    Ok(result.rows_affected() as usize)
}

pub async fn list_batch_jobs(
    conn: &mut SqliteConnection,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<BatchJobSummary>, AppError> {
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);

    let jobs = sqlx::query_as::<_, BatchJobSummaryRow>(
        r#"
        SELECT id, job_type, status, total_items, completed_items, failed_items,
               progress, created_at, started_at, completed_at
        FROM batch_jobs
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list jobs: {}", e)))?;

    let summaries = jobs
        .into_iter()
        .map(|row| BatchJobSummary {
            id: row.id,
            job_type: row.job_type,
            status: row.status,
            total_items: row.total_items,
            completed_items: row.completed_items,
            failed_items: row.failed_items,
            progress: row.progress,
            created_at: row.created_at,
            completed_at: row.completed_at,
        })
        .collect();

    Ok(summaries)
}

pub async fn delete_batch_job(conn: &mut SqliteConnection, job_id: &str) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM batch_jobs WHERE id = ?1", job_id)
        .execute(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete job: {}", e)))?;

    Ok(())
}
