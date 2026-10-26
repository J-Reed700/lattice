//! Saving, renaming and deleting documents.

use super::parts::{save_chunks, save_tags};
use crate::infrastructure::persistence::mappers::DocumentModel;
use lattice_core::domain::entities::Document as DocumentEntity;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;
use std::time::Instant;
use tracing::{info, instrument};

pub async fn save(conn: &mut SqliteConnection, entity: &DocumentEntity) -> Result<()> {
    // Persist the full aggregate (document + chunks + tags).
    // Saving only the document row causes FK failures when embeddings are saved
    // against chunk IDs that were never persisted.
    save_aggregate(conn, entity).await
}

pub async fn delete(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM documents WHERE id = ?", id)
        .execute(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete document: {}", e)))?;

    Ok(())
}

#[instrument(skip(conn, aggregate), fields(document_id = %aggregate.id().as_str()))]
pub async fn save_aggregate(conn: &mut SqliteConnection, aggregate: &DocumentEntity) -> Result<()> {
    use lattice_core::domain::entities::document::DocumentStatus as EntityStatus;

    let start = Instant::now();

    let status = match aggregate.status() {
        EntityStatus::Pending => "pending".to_string(),
        EntityStatus::Processing => "processing".to_string(),
        EntityStatus::Indexed => "indexed".to_string(),
        EntityStatus::Failed => "failed".to_string(),
    };

    let model = DocumentModel {
        id: aggregate.id().as_str().to_string(),
        file_path: aggregate.file_path().display().to_string(),
        file_name: aggregate.file_name().to_string(),
        file_type: aggregate.file_type().map(|s| s.to_string()),
        mime_type: aggregate.mime_type().to_string(),
        size_bytes: aggregate.size_bytes(),
        modified_at: aggregate.modified_at().to_rfc3339(),
        indexed_at: aggregate.indexed_at().to_rfc3339(),
        checksum: aggregate.checksum().as_str().to_string(),
        status,
        error_message: aggregate.error_message().map(|s| s.to_string()),
        language: aggregate.language().to_string(),
        category: aggregate.category().to_string(),
        quality_score: aggregate.quality_score() as f64,
        access_count: aggregate.access_count() as i64,
        last_accessed_at: aggregate.last_accessed_at().map(|dt| dt.to_rfc3339()),
        word_count: aggregate.word_count() as i64,
        content: aggregate.content().to_string(),
        source_context: aggregate
            .source_context()
            .map(|v| serde_json::json!(v).to_string()),
    };

    sqlx::query!(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, file_type, mime_type,
            size_bytes, modified_at, indexed_at, checksum, status, error_message,
            language, category, quality_score, access_count, last_accessed_at, word_count
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(file_path) DO UPDATE SET
            file_path = excluded.file_path,
            file_name = excluded.file_name,
            file_type = excluded.file_type,
            mime_type = excluded.mime_type,
            size_bytes = excluded.size_bytes,
            modified_at = excluded.modified_at,
            indexed_at = excluded.indexed_at,
            checksum = excluded.checksum,
            status = excluded.status,
            error_message = excluded.error_message,
            language = excluded.language,
            category = excluded.category,
            quality_score = excluded.quality_score,
            access_count = excluded.access_count,
            last_accessed_at = excluded.last_accessed_at,
            word_count = excluded.word_count,
            updated_at = CURRENT_TIMESTAMP
        "#,
        model.id,
        model.file_path,
        model.file_name,
        model.file_type,
        model.mime_type,
        model.size_bytes,
        model.modified_at,
        model.indexed_at,
        model.checksum,
        model.status,
        model.error_message,
        model.language,
        model.category,
        model.quality_score,
        model.access_count,
        model.last_accessed_at,
        model.word_count
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to save document: {}", e)))?;

    let doc_id = aggregate.id().as_str();
    sqlx::query("UPDATE documents SET source_context = ? WHERE id = ?")
        .bind(
            aggregate
                .source_context()
                .map(serde_json::to_string)
                .transpose()?,
        )
        .bind(doc_id)
        .execute(&mut *conn)
        .await?;
    save_chunks(&mut *conn, doc_id, aggregate.chunks()).await?;
    save_tags(&mut *conn, doc_id, aggregate.tags()).await?;

    let elapsed = start.elapsed();
    info!(
        duration_ms = elapsed.as_millis(),
        total_operation_time = elapsed.as_secs_f64(),
        chunk_count = aggregate.chunks().len(),
        tag_count = aggregate.tags().len(),
        "Document aggregate saved successfully"
    );

    Ok(())
}

/// A batch has exactly the same aggregate semantics as repeated saves.
/// The caller owns the surrounding transaction.
pub async fn save_batch(conn: &mut SqliteConnection, entities: &[DocumentEntity]) -> Result<()> {
    for entity in entities {
        save_aggregate(conn, entity).await?;
    }
    Ok(())
}

/// Optimized batch delete for multiple documents
/// Uses single DELETE with IN clause to reduce database round-trips
#[instrument(skip(conn), fields(id_count = ids.len()))]
pub async fn delete_batch_optimized(conn: &mut SqliteConnection, ids: &[&str]) -> Result<()> {
    use std::time::Instant;
    use tracing::info;

    if ids.is_empty() {
        return Ok(());
    }

    let start = Instant::now();
    const DELETE_BATCH_SIZE: usize = 500; // SQLite can handle larger batches for DELETE

    for batch in ids.chunks(DELETE_BATCH_SIZE) {
        let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("DELETE FROM documents WHERE id IN ({})", placeholders);

        let mut query_builder = sqlx::query(&query);
        for id in batch {
            query_builder = query_builder.bind(id);
        }

        query_builder
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete document batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    info!(
        duration_ms = elapsed.as_millis(),
        id_count = ids.len(),
        batch_size = DELETE_BATCH_SIZE,
        "Documents deleted successfully using bulk DELETE"
    );

    Ok(())
}

pub async fn rename(conn: &mut SqliteConnection, id: &str, name: &str) -> Result<()> {
    let result = sqlx::query(
        "UPDATE documents SET file_name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(name)
    .bind(id)
    .execute(conn)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Document not found: {id}")));
    }
    Ok(())
}
