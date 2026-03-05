use crate::domain::entities::model_file::ModelFile;
use crate::domain::value_objects::model_status::FileStatus;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Row, SqliteConnection};
use std::str::FromStr;
use tracing::{debug, error, info};

// Database row DTO for model file queries
#[derive(Debug, sqlx::FromRow)]
struct ModelFileRow {
    id: String,
    model_id: String,
    file_name: String,
    file_path: String,
    relative_path: String,
    size_bytes: i64,
    downloaded_bytes: i64,
    status: String,
    checksum_sha256: Option<String>,
    download_url: String,
    downloaded_at: Option<String>,
    created_at: String,
    updated_at: String,
}

pub async fn create(conn: &mut SqliteConnection, model_file: &ModelFile) -> Result<()> {
    let status_str = model_file.status.to_string();
    let downloaded_at_str = model_file.downloaded_at.map(|dt| dt.to_rfc3339());
    let created_at_str = model_file.created_at.to_rfc3339();
    let updated_at_str = model_file.updated_at.to_rfc3339();

    let result = sqlx::query(
        r#"
        INSERT INTO model_files (
            id, model_id, file_name, file_path, relative_path, size_bytes,
            downloaded_bytes, checksum_sha256, download_url, status,
            created_at, updated_at, downloaded_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(model_id, file_name) DO UPDATE SET
            file_path = excluded.file_path,
            relative_path = excluded.relative_path,
            size_bytes = excluded.size_bytes,
            downloaded_bytes = excluded.downloaded_bytes,
            checksum_sha256 = excluded.checksum_sha256,
            download_url = excluded.download_url,
            status = excluded.status,
            updated_at = excluded.updated_at,
            downloaded_at = excluded.downloaded_at
        "#,
    )
    .bind(&model_file.id)
    .bind(&model_file.model_id)
    .bind(&model_file.file_name)
    .bind(&model_file.file_path)
    .bind(&model_file.relative_path)
    .bind(model_file.size_bytes)
    .bind(model_file.downloaded_bytes)
    .bind(&model_file.checksum_sha256)
    .bind(&model_file.download_url)
    .bind(&status_str)
    .bind(&created_at_str)
    .bind(&updated_at_str)
    .bind(&downloaded_at_str)
    .execute(conn)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            model_id = %model_file.model_id,
            file_name = %model_file.file_name,
            "Failed to create model_file record"
        );
        AppError::Database(format!("Failed to create model_file record: {}", e))
    })?;

    info!(
        model_id = %model_file.model_id,
        file_name = %model_file.file_name,
        status = %status_str,
        rows_affected = result.rows_affected(),
        "Upserted model_file record"
    );

    Ok(())
}

pub async fn find_by_id(conn: &mut SqliteConnection, file_id: &str) -> Result<Option<ModelFile>> {
    let record = sqlx::query_as::<_, ModelFileRow>(
        r#"
        SELECT id, model_id, file_name, file_path, relative_path, size_bytes, downloaded_bytes,
               status, checksum_sha256, download_url, downloaded_at, created_at, updated_at
        FROM model_files
        WHERE id = ?
        "#,
    )
    .bind(file_id)
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        error!(error = %e, file_id = %file_id, "Failed to find model file by id");
        AppError::Database(format!("Failed to find model file: {}", e))
    })?;

    match record {
        Some(row) => Ok(Some(parse_model_file_from_row(row)?)),
        None => Ok(None),
    }
}

pub async fn find_by_model_id(
    conn: &mut SqliteConnection,
    model_id: &str,
) -> Result<Vec<ModelFile>> {
    let records = sqlx::query_as::<_, ModelFileRow>(
        r#"
        SELECT id, model_id, file_name, file_path, relative_path, size_bytes, downloaded_bytes,
               status, checksum_sha256, download_url, downloaded_at, created_at, updated_at
        FROM model_files
        WHERE model_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(model_id)
    .fetch_all(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to find model files by model_id");
        AppError::Database(format!("Failed to find model files: {}", e))
    })?;

    let files = records
        .into_iter()
        .filter_map(|row| match parse_model_file_from_row(row) {
            Ok(file) => Some(file),
            Err(e) => {
                error!(error = %e, "Failed to parse model file row");
                None
            }
        })
        .collect();

    Ok(files)
}

pub async fn update_file_status(
    conn: &mut SqliteConnection,
    file_id: &str,
    status: FileStatus,
) -> Result<()> {
    let status_str = status.to_string();

    if status == FileStatus::Completed {
        let result = sqlx::query!(
            r#"
            UPDATE model_files
            SET status = ?1, downloaded_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?2
            "#,
            status_str,
            file_id
        )
        .execute(conn)
        .await
        .map_err(|e| {
            error!(error = %e, file_id = %file_id, "Failed to update file status to completed");
            AppError::Database(format!("Failed to update file status: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Model file not found: {}",
                file_id
            )));
        }
    } else {
        let result = sqlx::query!(
            r#"
            UPDATE model_files
            SET status = ?1, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?2
            "#,
            status_str,
            file_id
        )
        .execute(conn)
        .await
        .map_err(|e| {
            error!(error = %e, file_id = %file_id, "Failed to update file status");
            AppError::Database(format!("Failed to update file status: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Model file not found: {}",
                file_id
            )));
        }
    }

    debug!(file_id = %file_id, status = %status_str, "File status updated successfully");
    Ok(())
}

pub async fn count_completed_files(conn: &mut SqliteConnection, model_id: &str) -> Result<usize> {
    let completed_status = FileStatus::Completed.to_string();

    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM model_files WHERE model_id = ? AND status = ?",
    )
    .bind(model_id)
    .bind(&completed_status)
    .fetch_one(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to count completed files");
        AppError::Database(format!("Failed to count completed files: {}", e))
    })?;

    Ok(result as usize)
}

pub async fn count_total_files(conn: &mut SqliteConnection, model_id: &str) -> Result<usize> {
    let result =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM model_files WHERE model_id = ?")
            .bind(model_id)
            .fetch_one(conn)
            .await
            .map_err(|e| {
                error!(error = %e, model_id = %model_id, "Failed to count total files");
                AppError::Database(format!("Failed to count total files: {}", e))
            })?;

    Ok(result as usize)
}

pub async fn update_file_status_by_model_and_name(
    conn: &mut SqliteConnection,
    model_id: &str,
    file_name: &str,
    status: FileStatus,
    size_bytes: Option<i64>,
) -> Result<()> {
    let status_str = status.to_string();

    if status == FileStatus::Completed {
        let size_bytes = size_bytes.unwrap_or(0);
        let result = sqlx::query!(
            r#"
            UPDATE model_files
            SET status = ?1, downloaded_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP, size_bytes = ?2
            WHERE model_id = ?3 AND file_name = ?4
            "#,
            status_str,
            size_bytes,
            model_id,
            file_name
        )
        .execute(conn)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, file_name = %file_name, "Failed to update file status to completed");
            AppError::Database(format!("Failed to update file status: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Model file not found: {}/{}",
                model_id, file_name
            )));
        }
    } else {
        let result = sqlx::query!(
            r#"
            UPDATE model_files
            SET status = ?1, updated_at = CURRENT_TIMESTAMP
            WHERE model_id = ?2 AND file_name = ?3
            "#,
            status_str,
            model_id,
            file_name
        )
        .execute(conn)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, file_name = %file_name, "Failed to update file status");
            AppError::Database(format!("Failed to update file status: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Model file not found: {}/{}",
                model_id, file_name
            )));
        }
    }

    debug!(model_id = %model_id, file_name = %file_name, status = %status_str, "File status updated successfully");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_model_file_from_row(row: ModelFileRow) -> Result<ModelFile> {
    let status_enum = FileStatus::from_str(&row.status)
        .map_err(|e| AppError::InvalidData(format!("Invalid file status: {}", e)))?;

    let downloaded_at_dt =
        parse_optional_db_timestamp(row.downloaded_at.as_deref(), "downloaded_at")?;
    let created_at_dt = parse_required_db_timestamp(&row.created_at, "created_at")?;
    let updated_at_dt = parse_required_db_timestamp(&row.updated_at, "updated_at")?;

    Ok(ModelFile {
        id: row.id,
        model_id: row.model_id,
        file_name: row.file_name,
        file_path: row.file_path,
        relative_path: row.relative_path,
        size_bytes: row.size_bytes,
        downloaded_bytes: row.downloaded_bytes,
        checksum_sha256: row.checksum_sha256,
        download_url: row.download_url,
        status: status_enum,
        created_at: created_at_dt,
        updated_at: updated_at_dt,
        downloaded_at: downloaded_at_dt,
    })
}

fn parse_required_db_timestamp(value: &str, field_name: &str) -> Result<DateTime<Utc>> {
    parse_db_timestamp(value)
        .ok_or_else(|| AppError::InvalidData(format!("Invalid {} timestamp format", field_name)))
}

fn parse_optional_db_timestamp(
    value: Option<&str>,
    field_name: &str,
) -> Result<Option<DateTime<Utc>>> {
    match value {
        Some(v) => {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            parse_db_timestamp(trimmed).map(Some).ok_or_else(|| {
                AppError::InvalidData(format!("Invalid {} timestamp format", field_name))
            })
        }
        None => Ok(None),
    }
}

fn parse_db_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_db_timestamp_accepts_rfc3339() {
        let ts = "2026-03-04T19:53:19Z";
        let parsed = parse_db_timestamp(ts).expect("should parse rfc3339 timestamp");
        assert_eq!(parsed.to_rfc3339(), "2026-03-04T19:53:19+00:00");
    }

    #[test]
    fn parse_db_timestamp_accepts_sqlite_current_timestamp_format() {
        let ts = "2026-03-04 19:53:19";
        let parsed = parse_db_timestamp(ts).expect("should parse sqlite timestamp");
        assert_eq!(parsed.to_rfc3339(), "2026-03-04T19:53:19+00:00");
    }
}
