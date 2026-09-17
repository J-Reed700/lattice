use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::SqliteConnection;
use std::str::FromStr;
use tracing::{debug, error, info};

// Database row DTOs
#[derive(Debug, sqlx::FromRow)]
struct ModelRow {
    id: String,
    model_id: String,
    model_name: String,
    base_path: String,
    total_size_bytes: i64,
    status: String,
    model_type: String,
    downloaded_at: Option<String>,
    last_used_at: Option<String>,
    use_count: i64,
    is_active_for_chat: i64,
    is_active_for_embedding: i64,
    metadata: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ModelFileSimpleRow {
    id: String,
    model_id: String,
    file_name: String,
    file_path: String,
    relative_path: String,
    size_bytes: i64,
    status: String,
    checksum: Option<String>,
    downloaded_at: Option<String>,
    created_at: String,
    updated_at: String,
}

pub async fn find_by_model_id(
    conn: &mut SqliteConnection,
    model_id: &str,
) -> Result<Option<Model>> {
    let record = sqlx::query_as::<_, ModelRow>(
        r#"
        SELECT id, model_id, model_name, base_path, total_size_bytes, status, model_type,
               downloaded_at, last_used_at, use_count, is_active_for_chat, is_active_for_embedding,
               metadata, created_at, updated_at
        FROM models
        WHERE model_id = ?
        "#,
    )
    .bind(model_id)
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to find model by model_id");
        AppError::Database(format!("Failed to find model: {}", e))
    })?;

    match record {
        Some(row) => Ok(Some(row.try_into()?)),
        None => Ok(None),
    }
}

pub async fn get_model_files(
    conn: &mut SqliteConnection,
    model_id: &str,
) -> Result<Vec<ModelFile>> {
    let records = sqlx::query_as::<_, ModelFileSimpleRow>(
        r#"
        SELECT id, model_id, file_name, file_path, relative_path, size_bytes, status,
               checksum, downloaded_at, created_at, updated_at
        FROM model_files
        WHERE model_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(model_id)
    .fetch_all(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to get model files");
        AppError::Database(format!("Failed to get model files: {}", e))
    })?;

    let files = records
        .into_iter()
        .filter_map(|row| match row.try_into() {
            Ok(file) => Some(file),
            Err(e) => {
                error!(error = %e, "Failed to parse model file row");
                None
            }
        })
        .collect();

    Ok(files)
}

pub async fn upsert_model(conn: &mut SqliteConnection, model: &Model) -> Result<()> {
    let id = &model.id;
    let model_id = &model.model_id;
    let model_name = &model.name;
    let base_path = &model.base_path;
    let total_size_bytes = model.total_size_bytes;
    let status = model.status.to_string();
    let model_type = model.model_type.to_string();
    let architecture = &model.architecture;
    let description = model.description.as_deref();
    let downloaded_at = model.downloaded_at.map(|dt| dt.to_rfc3339());
    let last_used_at = model.last_used_at.map(|dt| dt.to_rfc3339());
    let use_count = model.use_count as i64;
    let is_active_for_chat = if model.is_active_for_chat { 1 } else { 0 };
    let is_active_for_embedding = if model.is_active_for_embedding { 1 } else { 0 };
    let metadata = model
        .metadata
        .as_ref()
        .map(|m| serde_json::to_string(m).unwrap_or_default());
    let created_at = model.created_at.to_rfc3339();
    let updated_at = model.updated_at.to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO models (
            id, model_id, model_name, description, base_path, total_size_bytes, status, model_type,
            architecture, downloaded_at, last_used_at, use_count, is_active_for_chat, is_active_for_embedding,
            metadata, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(model_id) DO UPDATE SET
            model_name = excluded.model_name,
            description = excluded.description,
            base_path = excluded.base_path,
            total_size_bytes = excluded.total_size_bytes,
            status = excluded.status,
            model_type = excluded.model_type,
            architecture = excluded.architecture,
            downloaded_at = excluded.downloaded_at,
            last_used_at = excluded.last_used_at,
            use_count = excluded.use_count,
            is_active_for_chat = excluded.is_active_for_chat,
            is_active_for_embedding = excluded.is_active_for_embedding,
            metadata = excluded.metadata,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(id)
    .bind(model_id)
    .bind(model_name)
    .bind(description)
    .bind(base_path)
    .bind(total_size_bytes)
    .bind(status)
    .bind(model_type)
    .bind(architecture)
    .bind(downloaded_at)
    .bind(last_used_at)
    .bind(use_count)
    .bind(is_active_for_chat)
    .bind(is_active_for_embedding)
    .bind(metadata)
    .bind(created_at)
    .bind(updated_at)
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to upsert model");
        AppError::Database(format!("Failed to upsert model: {}", e))
    })?;

    Ok(())
}

pub async fn upsert_model_file(conn: &mut SqliteConnection, file: &ModelFile) -> Result<()> {
    let file_id = &file.id;
    let file_model_id = &file.model_id;
    let file_name = &file.file_name;
    let file_path = &file.file_path;
    let relative_path = &file.relative_path;
    let size_bytes = file.size_bytes;
    let file_status = file.status.to_string();
    let checksum = file.checksum_sha256.as_deref();
    let file_downloaded_at = file.downloaded_at.map(|dt| dt.to_rfc3339());
    let file_created_at = file.created_at.to_rfc3339();
    let file_updated_at = file.updated_at.to_rfc3339();

    sqlx::query!(
        r#"
        INSERT INTO model_files (
            id, model_id, file_name, file_path, relative_path, size_bytes, status,
            checksum, downloaded_at, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(id) DO UPDATE SET
            file_name = excluded.file_name,
            file_path = excluded.file_path,
            relative_path = excluded.relative_path,
            size_bytes = excluded.size_bytes,
            status = excluded.status,
            checksum = excluded.checksum,
            downloaded_at = excluded.downloaded_at,
            updated_at = excluded.updated_at
        "#,
        file_id,
        file_model_id,
        file_name,
        file_path,
        relative_path,
        size_bytes,
        file_status,
        checksum,
        file_downloaded_at,
        file_created_at,
        file_updated_at
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, file_id = %file_id, "Failed to upsert model file");
        AppError::Database(format!("Failed to upsert model file: {}", e))
    })?;

    Ok(())
}

pub async fn update_model_status(
    conn: &mut SqliteConnection,
    model_id: &str,
    status: ModelStatus,
) -> Result<()> {
    let status_str = status.to_string();
    let updated_at = Utc::now().to_rfc3339();

    let result = sqlx::query!(
        r#"
        UPDATE models
        SET status = ?1, updated_at = ?2
        WHERE model_id = ?3
        "#,
        status_str,
        updated_at,
        model_id
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to update model status");
        AppError::Database(format!("Failed to update model status: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
    }

    debug!(model_id = %model_id, status = %status_str, "Model status updated");
    Ok(())
}

pub async fn deactivate_all_chat_models(conn: &mut SqliteConnection) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE models
        SET is_active_for_chat = 0
        WHERE is_active_for_chat = 1
        "#
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to deactivate other chat models");
        AppError::Database(format!("Failed to deactivate chat models: {}", e))
    })?;

    Ok(())
}

pub async fn activate_chat_model(conn: &mut SqliteConnection, model_id: &str) -> Result<()> {
    let updated_at = Utc::now().to_rfc3339();
    let result = sqlx::query!(
        r#"
        UPDATE models
        SET is_active_for_chat = 1, updated_at = ?1
        WHERE model_id = ?2
        "#,
        updated_at,
        model_id
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to set active chat model");
        AppError::Database(format!("Failed to set active chat model: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
    }

    info!(model_id = %model_id, "Set active chat model");
    Ok(())
}

pub async fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Model>> {
    let records = sqlx::query_as::<_, ModelRow>(
        r#"
        SELECT id, model_id, model_name, base_path, total_size_bytes, status, model_type,
               downloaded_at, last_used_at, use_count, is_active_for_chat, is_active_for_embedding,
               metadata, created_at, updated_at
        FROM models
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to list models");
        AppError::Database(format!("Failed to list models: {}", e))
    })?;

    let models = records
        .into_iter()
        .filter_map(|row| match row.try_into() {
            Ok(model) => Some(model),
            Err(e) => {
                error!(error = %e, "Failed to parse model row");
                None
            }
        })
        .collect();

    Ok(models)
}

pub async fn get_active_chat_model(conn: &mut SqliteConnection) -> Result<Option<Model>> {
    let record = sqlx::query_as::<_, ModelRow>(
        r#"
        SELECT id, model_id, model_name, base_path, total_size_bytes, status, model_type,
               downloaded_at, last_used_at, use_count, is_active_for_chat, is_active_for_embedding,
               metadata, created_at, updated_at
        FROM models
        WHERE is_active_for_chat = 1
        LIMIT 1
        "#,
    )
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to get active chat model");
        AppError::Database(format!("Failed to get active chat model: {}", e))
    })?;

    match record {
        Some(row) => Ok(Some(row.try_into()?)),
        None => Ok(None),
    }
}

pub async fn get_active_embedding_model(conn: &mut SqliteConnection) -> Result<Option<Model>> {
    let record = sqlx::query_as::<_, ModelRow>(
        r#"
        SELECT id, model_id, model_name, base_path, total_size_bytes, status, model_type,
               downloaded_at, last_used_at, use_count, is_active_for_chat, is_active_for_embedding,
               metadata, created_at, updated_at
        FROM models
        WHERE is_active_for_embedding = 1
        LIMIT 1
        "#,
    )
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to get active embedding model");
        AppError::Database(format!("Failed to get active embedding model: {}", e))
    })?;

    match record {
        Some(row) => Ok(Some(row.try_into()?)),
        None => Ok(None),
    }
}

pub async fn clear_active_chat_model(conn: &mut SqliteConnection) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE models
        SET is_active_for_chat = 0
        WHERE is_active_for_chat = 1
        "#
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to clear active chat model");
        AppError::Database(format!("Failed to clear active chat model: {}", e))
    })?;
    info!("Cleared active chat model");
    Ok(())
}

pub async fn clear_active_embedding_model(conn: &mut SqliteConnection) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE models
        SET is_active_for_embedding = 0
        WHERE is_active_for_embedding = 1
        "#
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to clear active embedding model");
        AppError::Database(format!("Failed to clear active embedding model: {}", e))
    })?;
    info!("Cleared active embedding model");
    Ok(())
}

pub async fn delete_if_not_active(conn: &mut SqliteConnection, model_id: &str) -> Result<()> {
    let result = sqlx::query!(
        r#"
        DELETE FROM models
        WHERE model_id = ?1
          AND is_active_for_chat = 0
          AND is_active_for_embedding = 0
        "#,
        model_id
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to delete model");
        AppError::Database(format!("Failed to delete model: {}", e))
    })?;

    if result.rows_affected() > 0 {
        info!(model_id = %model_id, "Model deleted successfully (was not active)");
    }

    Ok(())
}

pub async fn update_status_completed(conn: &mut SqliteConnection, model_id: &str) -> Result<()> {
    let status_str = ModelStatus::Completed.to_string();
    let updated_at = Utc::now().to_rfc3339();

    let result = sqlx::query!(
        r#"
        UPDATE models
        SET status = ?1, downloaded_at = CURRENT_TIMESTAMP, updated_at = ?2
        WHERE model_id = ?3
        "#,
        status_str,
        updated_at,
        model_id
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to update model status to completed");
        AppError::Database(format!("Failed to update model status: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
    }

    debug!(model_id = %model_id, status = %status_str, "Model status updated to completed");
    Ok(())
}

pub async fn update_status_failed(conn: &mut SqliteConnection, model_id: &str) -> Result<()> {
    let status_str = ModelStatus::Failed.to_string();
    let updated_at = Utc::now().to_rfc3339();

    let result = sqlx::query!(
        r#"
        UPDATE models
        SET status = ?1, updated_at = ?2
        WHERE model_id = ?3
        "#,
        status_str,
        updated_at,
        model_id
    )
    .execute(conn)
    .await
    .map_err(|e| {
        error!(error = %e, model_id = %model_id, "Failed to update model status to failed");
        AppError::Database(format!("Failed to update model status: {}", e))
    })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Model not found: {}", model_id)));
    }

    debug!(model_id = %model_id, status = %status_str, "Model status updated to failed");
    Ok(())
}

impl TryFrom<ModelRow> for Model {
    type Error = AppError;

    fn try_from(row: ModelRow) -> Result<Self, Self::Error> {
        let status_enum = ModelStatus::from_str(&row.status)
            .map_err(|e| AppError::InvalidData(format!("Invalid model status: {}", e)))?;

        let downloaded_at_dt = parse_optional_db_timestamp(row.downloaded_at.as_deref())?;
        let last_used_at_dt = parse_optional_db_timestamp(row.last_used_at.as_deref())?;
        let created_at_dt = parse_required_db_timestamp(&row.created_at, "created_at")?;
        let updated_at_dt = parse_required_db_timestamp(&row.updated_at, "updated_at")?;

        let metadata_val = match row.metadata {
            Some(json_str) => Some(
                serde_json::from_str(&json_str)
                    .map_err(|e| AppError::InvalidData(format!("Invalid metadata JSON: {}", e)))?,
            ),
            None => None,
        };

        Ok(Model {
            id: row.id,
            model_id: row.model_id,
            name: row.model_name,
            description: None,
            base_path: row.base_path,
            total_size_bytes: row.total_size_bytes,
            architecture: "unknown".to_string(),
            model_type: row.model_type,
            status: status_enum,
            files: vec![],
            is_active_for_chat: row.is_active_for_chat != 0,
            is_active_for_embedding: row.is_active_for_embedding != 0,
            use_count: row.use_count as u64,
            last_used_at: last_used_at_dt,
            metadata: metadata_val,
            created_at: created_at_dt,
            updated_at: updated_at_dt,
            downloaded_at: downloaded_at_dt,
        })
    }
}

impl TryFrom<ModelFileSimpleRow> for ModelFile {
    type Error = AppError;

    fn try_from(row: ModelFileSimpleRow) -> Result<Self, Self::Error> {
        let status_enum = FileStatus::from_str(&row.status)
            .map_err(|e| AppError::InvalidData(format!("Invalid file status: {}", e)))?;

        let downloaded_at_dt = parse_optional_db_timestamp(row.downloaded_at.as_deref())?;
        let created_at_dt = parse_required_db_timestamp(&row.created_at, "created_at")?;
        let updated_at_dt = parse_required_db_timestamp(&row.updated_at, "updated_at")?;

        Ok(ModelFile {
            id: row.id,
            model_id: row.model_id,
            file_name: row.file_name,
            file_path: row.file_path,
            relative_path: row.relative_path,
            size_bytes: row.size_bytes,
            downloaded_bytes: 0,
            checksum_sha256: row.checksum,
            download_url: String::new(),
            status: status_enum,
            created_at: created_at_dt,
            updated_at: updated_at_dt,
            downloaded_at: downloaded_at_dt,
        })
    }
}

fn parse_required_db_timestamp(value: &str, field_name: &str) -> Result<DateTime<Utc>> {
    parse_db_timestamp(value)
        .ok_or_else(|| AppError::InvalidData(format!("Invalid {} timestamp format", field_name)))
}

fn parse_optional_db_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    match value {
        Some(v) => parse_db_timestamp(v)
            .map(Some)
            .ok_or_else(|| AppError::InvalidData("Invalid timestamp format".to_string())),
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
