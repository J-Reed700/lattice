//! Document Repository Operations
//!
//! Generic SQL operations for document persistence that work with any SQLite executor.
//! These functions can be used with both connection pools and transactions.

use crate::application::ports::Filter;
use crate::domain::entities::chunk::Chunk;
use crate::domain::entities::Document as DocumentEntity;
use crate::infrastructure::persistence::mappers::{
    ChunkMapper, ChunkModel, DocumentMapper, DocumentModel,
};
use crate::shared::domain_types::TagId;
use crate::shared::error::{AppError, Result};
use sqlx::{QueryBuilder, SqliteConnection, SqlitePool};
use std::time::Instant;
use tracing::{info, instrument};

const CHUNK_BATCH_SIZE: usize = 120;
const TAG_BATCH_SIZE: usize = 450;

pub async fn find_by_id(conn: &mut SqliteConnection, id: &str) -> Result<Option<DocumentEntity>> {
    let db_model = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count
        FROM documents
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to find document with id '{}'. Document may not exist or database may be corrupted. Error: {}",
            id, e
        ))
    })?;

    match db_model {
        Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
        None => Ok(None),
    }
}

pub async fn find_by_path(
    conn: &mut SqliteConnection,
    file_path: &str,
) -> Result<Option<DocumentEntity>> {
    let db_model = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count
        FROM documents
        WHERE file_path = ?
        "#,
    )
    .bind(file_path)
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to find document at path '{}'. File may not be indexed or database may be corrupted. Error: {}",
            file_path, e
        ))
    })?;

    match db_model {
        Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
        None => Ok(None),
    }
}

pub async fn find_by_path_pattern(
    conn: &mut SqliteConnection,
    pattern: &str,
) -> Result<Vec<DocumentEntity>> {
    let db_models: Vec<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count
        FROM documents
        WHERE file_path LIKE ?
        ORDER BY file_path DESC
        "#,
    )
    .bind(pattern)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find documents by pattern: {}", e)))?;

    Ok(DocumentMapper::to_entities(&db_models))
}

pub async fn find_by_checksum(
    conn: &mut SqliteConnection,
    checksum: &crate::domain::value_objects::Checksum,
) -> Result<Option<DocumentEntity>> {
    let db_model = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count
        FROM documents
        WHERE checksum = ?
        "#,
    )
    .bind(checksum.as_str())
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!("Failed to find document by checksum. Error: {}", e))
    })?;

    match db_model {
        Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
        None => Ok(None),
    }
}

pub async fn find_all(conn: &mut SqliteConnection) -> Result<Vec<DocumentEntity>> {
    let db_models: Vec<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT
            id,
            file_path,
            file_name,
            file_type,
            mime_type,
            size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count
        FROM documents
        ORDER BY indexed_at DESC
        "#,
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list documents: {}", e)))?;

    Ok(DocumentMapper::to_entities(&db_models))
}

pub async fn exists_by_path(conn: &mut SqliteConnection, file_path: &str) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents WHERE file_path = ?)")
        .bind(file_path)
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to check document existence: {}", e)))
}

pub async fn count_documents(conn: &mut SqliteConnection) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count documents: {}", e)))
}

pub async fn count_chunks(conn: &mut SqliteConnection) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_chunks")
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))
}

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

pub async fn count(conn: &mut SqliteConnection) -> Result<usize> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
        .fetch_one(conn)
        .await
        .map(|count| count as usize)
        .map_err(|e| AppError::Database(format!("Failed to count documents: {}", e)))
}

pub async fn exists(conn: &mut SqliteConnection, id: &str) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?)")
        .bind(id)
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to check document existence: {}", e)))
}

pub async fn find_file_path_by_id(
    conn: &mut SqliteConnection,
    document_id: &str,
) -> Result<String> {
    let file_path = sqlx::query!(
        r#"
        SELECT file_path as "path!"
        FROM documents
        WHERE id = ?
        "#,
        document_id
    )
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to find file path for document '{}': {}",
            document_id, e
        ))
    })?;

    match file_path {
        Some(record) => Ok(record.path),
        None => Err(AppError::NotFound(format!(
            "Document not found: {}",
            document_id
        ))),
    }
}

pub async fn find_id_by_path(
    conn: &mut SqliteConnection,
    file_path: &str,
) -> Result<Option<String>> {
    let record = sqlx::query!("SELECT id FROM documents WHERE file_path = ?", file_path)
        .fetch_optional(conn)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to find document ID for path '{}': {}",
                file_path, e
            ))
        })?;

    Ok(record.and_then(|r| r.id))
}

pub async fn fetch_chunks_for_document(
    conn: &mut SqliteConnection,
    doc_id: &str,
) -> Result<Vec<Chunk>> {
    let db_models = sqlx::query_as::<_, ChunkModel>(
        r#"
        SELECT
            id,
            document_id,
            content,
            chunk_index,
            contextualized_content,
            context_prefix,
            start_char,
            end_char,
            language,
            token_count,
            word_count,
            has_code,
            section
        FROM text_chunks
        WHERE document_id = ?
        ORDER BY chunk_index ASC
        "#,
    )
    .bind(doc_id)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch chunks for document: {}", e)))?;

    Ok(ChunkMapper::to_entities(&db_models))
}

pub async fn fetch_tags_for_document(
    conn: &mut SqliteConnection,
    doc_id: &str,
) -> Result<Vec<TagId>> {
    let rows = sqlx::query!(
        "SELECT tag_id FROM document_tags WHERE document_id = ?",
        doc_id
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to fetch tags for document: {}", e)))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| TagId::from_string(row.tag_id).ok())
        .collect())
}

#[instrument(skip(conn, chunks), fields(doc_id = %doc_id, chunk_count = chunks.len()))]
pub async fn save_chunks(
    conn: &mut SqliteConnection,
    doc_id: &str,
    chunks: &[Chunk],
) -> Result<()> {
    let start = Instant::now();

    sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
        .bind(doc_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete old chunks: {}", e)))?;

    if chunks.is_empty() {
        info!(
            duration_ms = start.elapsed().as_millis(),
            chunk_count = 0,
            "No chunks to save"
        );
        return Ok(());
    }

    let batch_count = chunks.len().div_ceil(CHUNK_BATCH_SIZE);
    let models: Vec<ChunkModel> = chunks.iter().map(ChunkMapper::to_model).collect();

    for batch in models.chunks(CHUNK_BATCH_SIZE) {
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO text_chunks (
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix, start_char, end_char
            ) ",
        );

        query_builder.push_values(batch, |mut b, model| {
            b.push_bind(&model.id)
                .push_bind(&model.document_id)
                .push_bind(&model.content)
                .push_bind(model.chunk_index)
                .push_bind(&model.contextualized_content)
                .push_bind(&model.context_prefix)
                .push_bind(model.start_char)
                .push_bind(model.end_char);
        });

        query_builder
            .build()
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert chunk batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    info!(
        duration_ms = elapsed.as_millis(),
        chunks_per_second = if elapsed.as_secs() > 0 {
            chunks.len() as u64 / elapsed.as_secs()
        } else {
            chunks.len() as u64
        },
        batch_count = batch_count,
        batch_size = CHUNK_BATCH_SIZE,
        "Chunks saved successfully"
    );

    Ok(())
}

#[instrument(skip(conn, tags), fields(doc_id = %doc_id, tag_count = tags.len()))]
pub async fn save_tags(conn: &mut SqliteConnection, doc_id: &str, tags: &[TagId]) -> Result<()> {
    let start = Instant::now();

    sqlx::query!("DELETE FROM document_tags WHERE document_id = ?", doc_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete old tags: {}", e)))?;

    if tags.is_empty() {
        info!(
            duration_ms = start.elapsed().as_millis(),
            tag_count = 0,
            "No tags to save"
        );
        return Ok(());
    }

    let batch_count = tags.len().div_ceil(TAG_BATCH_SIZE);

    for batch in tags.chunks(TAG_BATCH_SIZE) {
        let mut query_builder =
            QueryBuilder::new("INSERT INTO document_tags (document_id, tag_id) ");

        query_builder.push_values(batch, |mut b, tag_id| {
            b.push_bind(doc_id).push_bind(tag_id.as_str());
        });

        query_builder
            .build()
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert tag batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    info!(
        duration_ms = elapsed.as_millis(),
        tags_per_second = if elapsed.as_secs() > 0 {
            tags.len() as u64 / elapsed.as_secs()
        } else {
            tags.len() as u64
        },
        batch_count = batch_count,
        batch_size = TAG_BATCH_SIZE,
        "Tags saved successfully"
    );

    Ok(())
}

#[instrument(skip(conn, aggregate), fields(document_id = %aggregate.id().as_str()))]
pub async fn save_aggregate(conn: &mut SqliteConnection, aggregate: &DocumentEntity) -> Result<()> {
    use crate::domain::entities::document::DocumentStatus as EntityStatus;

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
        content: aggregate.document().content().to_string(),
    };

    sqlx::query!(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, file_type, mime_type,
            size_bytes, modified_at, indexed_at, checksum, status,
            language, category, quality_score, access_count, last_accessed_at, word_count
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

pub async fn find_aggregate_by_checksum_pool(
    pool: &SqlitePool,
    checksum: &str,
) -> Result<Option<DocumentEntity>> {
    let doc_model: Option<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT id, file_path, file_name, file_type, mime_type, size_bytes, modified_at,
               indexed_at, checksum, status, language, category, quality_score,
               access_count, last_accessed_at, word_count
        FROM documents
        WHERE checksum = ?
        LIMIT 1
        "#,
    )
    .bind(checksum)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find document by checksum: {}", e)))?;

    match doc_model {
        Some(model) => {
            let doc_entity = DocumentMapper::to_entity(&model)?;
            let mut conn = pool.acquire().await?;
            let chunks = fetch_chunks_for_document(&mut conn, doc_entity.id().as_str()).await?;
            let tags = fetch_tags_for_document(&mut conn, doc_entity.id().as_str()).await?;
            let aggregate = doc_entity.from_parts(chunks, tags)?;
            Ok(Some(aggregate))
        }
        None => Ok(None),
    }
}

pub async fn find_aggregate_by_checksum_tx(
    conn: &mut SqliteConnection,
    checksum: &str,
) -> Result<Option<DocumentEntity>> {
    let doc_model: Option<DocumentModel> = sqlx::query_as::<_, DocumentModel>(
        r#"
        SELECT id, file_path, file_name, file_type, mime_type, size_bytes, modified_at,
               indexed_at, checksum, status, language, category, quality_score,
               access_count, last_accessed_at, word_count
        FROM documents
        WHERE checksum = ?
        LIMIT 1
        "#,
    )
    .bind(checksum)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find document by checksum: {}", e)))?;

    match doc_model {
        Some(model) => {
            let doc_entity = DocumentMapper::to_entity(&model)?;
            let chunks = fetch_chunks_for_document(conn, doc_entity.id().as_str()).await?;
            let tags = fetch_tags_for_document(conn, doc_entity.id().as_str()).await?;
            let aggregate = doc_entity.from_parts(chunks, tags)?;
            Ok(Some(aggregate))
        }
        None => Ok(None),
    }
}

/// Optimized batch save for multiple documents
/// Uses bulk INSERT with QueryBuilder to reduce database round-trips
#[instrument(skip(conn, entities), fields(document_count = entities.len()))]
pub async fn save_batch_optimized(
    conn: &mut SqliteConnection,
    entities: &[DocumentEntity],
) -> Result<()> {
    use sqlx::QueryBuilder;
    use std::time::Instant;
    use tracing::info;

    if entities.is_empty() {
        return Ok(());
    }

    let start = Instant::now();
    const DOCUMENT_BATCH_SIZE: usize = 100; // SQLite parameter limit safe

    let models: Vec<DocumentModel> = entities.iter().map(DocumentMapper::to_model).collect();
    let batch_count = models.len().div_ceil(DOCUMENT_BATCH_SIZE);

    for batch in models.chunks(DOCUMENT_BATCH_SIZE) {
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO documents (
                id, file_path, file_name, file_type, mime_type,
                size_bytes, modified_at, indexed_at, checksum, status,
                language, category, quality_score, access_count, last_accessed_at, word_count
            ) ",
        );

        query_builder.push_values(batch, |mut b, model| {
            b.push_bind(&model.id)
                .push_bind(&model.file_path)
                .push_bind(&model.file_name)
                .push_bind(&model.file_type)
                .push_bind(&model.mime_type)
                .push_bind(model.size_bytes)
                .push_bind(&model.modified_at)
                .push_bind(&model.indexed_at)
                .push_bind(&model.checksum)
                .push_bind(&model.status)
                .push_bind(&model.language)
                .push_bind(&model.category)
                .push_bind(model.quality_score)
                .push_bind(model.access_count)
                .push_bind(&model.last_accessed_at)
                .push_bind(model.word_count);
        });

        query_builder.push(
            " ON CONFLICT(file_path) DO UPDATE SET \
             file_name = excluded.file_name, \
             file_type = excluded.file_type, \
             mime_type = excluded.mime_type, \
             size_bytes = excluded.size_bytes, \
             modified_at = excluded.modified_at, \
             indexed_at = excluded.indexed_at, \
             checksum = excluded.checksum, \
             status = excluded.status, \
             language = excluded.language, \
             category = excluded.category, \
             quality_score = excluded.quality_score, \
             access_count = excluded.access_count, \
             last_accessed_at = excluded.last_accessed_at, \
             word_count = excluded.word_count, \
             updated_at = CURRENT_TIMESTAMP",
        );

        query_builder
            .build()
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert document batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    let docs_per_second = if elapsed.as_secs_f64() > 0.0 {
        entities.len() as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    info!(
        duration_ms = elapsed.as_millis(),
        document_count = entities.len(),
        batch_count = batch_count,
        batch_size = DOCUMENT_BATCH_SIZE,
        docs_per_second = format!("{:.1}", docs_per_second),
        "Documents saved successfully using bulk INSERT"
    );

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
