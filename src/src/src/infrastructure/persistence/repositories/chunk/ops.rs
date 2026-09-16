//! Chunk Repository Operations
//!
//! Generic SQL operations for chunk persistence that work with any SQLite executor.
//! These functions can be used with both connection pools and transactions.

use crate::domain::entities::chunk::Chunk as ChunkEntity;
use crate::infrastructure::persistence::mappers::{ChunkMapper, ChunkModel};
use crate::shared::error::{AppError, Result};
use sqlx::QueryBuilder;
use sqlx::SqliteConnection;

pub async fn save(conn: &mut SqliteConnection, chunk: &ChunkEntity) -> Result<()> {
    let model = ChunkMapper::to_model(chunk);

    sqlx::query(
        r#"
        INSERT INTO text_chunks (
            id, document_id, content, chunk_index,
            contextualized_content, context_prefix, start_char, end_char,
            language, token_count, word_count, has_code, section, page_number
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            content = excluded.content,
            chunk_index = excluded.chunk_index,
            contextualized_content = excluded.contextualized_content,
            context_prefix = excluded.context_prefix,
            start_char = excluded.start_char,
            end_char = excluded.end_char,
            language = excluded.language,
            token_count = excluded.token_count,
            word_count = excluded.word_count,
            has_code = excluded.has_code,
            section = excluded.section, page_number = excluded.page_number
        "#,
    )
    .bind(&model.id)
    .bind(&model.document_id)
    .bind(&model.content)
    .bind(model.chunk_index)
    .bind(&model.contextualized_content)
    .bind(&model.context_prefix)
    .bind(model.start_char)
    .bind(model.end_char)
    .bind(&model.language)
    .bind(model.token_count)
    .bind(model.word_count)
    .bind(model.has_code)
    .bind(&model.section)
    .bind(model.page_number)
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to save chunk: {}", e)))?;

    Ok(())
}

pub async fn find_by_id(conn: &mut SqliteConnection, id: &str) -> Result<Option<ChunkEntity>> {
    let db_model = sqlx::query_as::<_, ChunkModel>(
        r#"
        SELECT
            id, document_id, content, chunk_index,
            contextualized_content, context_prefix,
            start_char, end_char, language,
            token_count, word_count, has_code, section, page_number
        FROM text_chunks
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find chunk by id: {}", e)))?;

    match db_model {
        Some(model) => Ok(Some(ChunkMapper::to_entity(&model)?)),
        None => Ok(None),
    }
}

pub async fn find_all(conn: &mut SqliteConnection) -> Result<Vec<ChunkEntity>> {
    let db_models = sqlx::query_as::<_, ChunkModel>(
        r#"
        SELECT
            id, document_id, content, chunk_index,
            contextualized_content, context_prefix,
            start_char, end_char, language,
            token_count, word_count, has_code, section, page_number
        FROM text_chunks
        ORDER BY document_id, chunk_index ASC
        "#,
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list all chunks: {}", e)))?;

    Ok(ChunkMapper::to_entities(&db_models))
}

pub async fn find_by_ids(conn: &mut SqliteConnection, ids: &[String]) -> Result<Vec<ChunkEntity>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    const BATCH_SIZE: usize = 900; // SQLite parameter limit safety
    let mut all = Vec::new();

    for batch in ids.chunks(BATCH_SIZE) {
        let mut query_builder = QueryBuilder::new(
            r#"
            SELECT
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix,
                start_char, end_char, language,
                token_count, word_count, has_code, section, page_number
            FROM text_chunks
            WHERE id IN ("#,
        );

        let mut separated = query_builder.separated(", ");
        for id in batch {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let db_models: Vec<ChunkModel> = query_builder
            .build_query_as()
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to find chunks by ids: {}", e)))?;

        all.extend(ChunkMapper::to_entities(&db_models));
    }

    Ok(all)
}

pub async fn find_by_document(
    conn: &mut SqliteConnection,
    document_id: &str,
) -> Result<Vec<ChunkEntity>> {
    let db_models = sqlx::query_as::<_, ChunkModel>(
        r#"
        SELECT
            id, document_id, content, chunk_index,
            contextualized_content, context_prefix,
            start_char, end_char, language,
            token_count, word_count, has_code, section, page_number
        FROM text_chunks
        WHERE document_id = ?
        ORDER BY chunk_index ASC
        "#,
    )
    .bind(document_id)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find chunks by document: {}", e)))?;

    Ok(ChunkMapper::to_entities(&db_models))
}

pub async fn delete(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM text_chunks WHERE id = ?")
        .bind(id)
        .execute(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete chunk: {}", e)))?;
    Ok(())
}

pub async fn delete_by_document(conn: &mut SqliteConnection, document_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
        .bind(document_id)
        .execute(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete chunks: {}", e)))?;
    Ok(())
}

pub async fn count_all(conn: &mut SqliteConnection) -> Result<i64> {
    let result = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_chunks")
        .fetch_one(conn)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to count all chunks");
            AppError::Database(format!("Failed to count chunks: {}", e))
        })?;

    tracing::debug!(count = result, "Counted all chunks");
    Ok(result)
}

pub async fn count_indexed_documents(conn: &mut SqliteConnection) -> Result<i64> {
    let result =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE status = 'indexed'")
            .fetch_one(conn)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to count indexed documents");
                AppError::Database(format!("Failed to count indexed documents: {}", e))
            })?;

    tracing::debug!(count = result, "Counted indexed documents");
    Ok(result)
}

pub async fn count(conn: &mut SqliteConnection) -> Result<usize> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_chunks")
        .fetch_one(conn)
        .await
        .map(|c| c as usize)
        .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))
}

pub async fn count_by_document(conn: &mut SqliteConnection, document_id: &str) -> Result<usize> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
        .bind(document_id)
        .fetch_one(conn)
        .await
        .map(|c| c as usize)
        .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))
}

pub async fn exists(conn: &mut SqliteConnection, id: &str) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM text_chunks WHERE id = ?)")
        .bind(id)
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to check chunk existence: {}", e)))
}

/// Optimized batch save for multiple chunks
/// Uses bulk INSERT with QueryBuilder to reduce database round-trips
pub async fn save_batch_optimized(
    conn: &mut SqliteConnection,
    chunks: &[ChunkEntity],
) -> Result<()> {
    use sqlx::QueryBuilder;
    use std::time::Instant;
    use tracing::info;

    if chunks.is_empty() {
        return Ok(());
    }

    let start = Instant::now();
    const CHUNK_BATCH_SIZE: usize = 150; // SQLite parameter limit safe for chunks

    let models: Vec<ChunkModel> = chunks.iter().map(ChunkMapper::to_model).collect();
    let batch_count = models.len().div_ceil(CHUNK_BATCH_SIZE);

    for batch in models.chunks(CHUNK_BATCH_SIZE) {
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO text_chunks (
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix, start_char, end_char,
                language, token_count, word_count, has_code, section, page_number
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
                .push_bind(model.end_char)
                .push_bind(&model.language)
                .push_bind(model.token_count)
                .push_bind(model.word_count)
                .push_bind(model.has_code)
                .push_bind(&model.section)
                .push_bind(model.page_number);
        });

        query_builder.push(
            " ON CONFLICT(id) DO UPDATE SET \
             content = excluded.content, \
             chunk_index = excluded.chunk_index, \
             contextualized_content = excluded.contextualized_content, \
             context_prefix = excluded.context_prefix, \
             start_char = excluded.start_char, \
             end_char = excluded.end_char, \
             language = excluded.language, \
             token_count = excluded.token_count, \
             word_count = excluded.word_count, \
             has_code = excluded.has_code, \
             section = excluded.section",
        );

        query_builder
            .build()
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert chunk batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    let chunks_per_second = if elapsed.as_secs_f64() > 0.0 {
        chunks.len() as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    info!(
        duration_ms = elapsed.as_millis(),
        chunk_count = chunks.len(),
        batch_count = batch_count,
        batch_size = CHUNK_BATCH_SIZE,
        chunks_per_second = format!("{:.1}", chunks_per_second),
        "Chunks saved successfully using bulk INSERT"
    );

    Ok(())
}

/// Optimized batch delete for multiple chunks
/// Uses single DELETE with IN clause to reduce database round-trips
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
        let query = format!("DELETE FROM text_chunks WHERE id IN ({})", placeholders);

        let mut query_builder = sqlx::query(&query);
        for id in batch {
            query_builder = query_builder.bind(id);
        }

        query_builder
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete chunk batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    info!(
        duration_ms = elapsed.as_millis(),
        id_count = ids.len(),
        batch_size = DELETE_BATCH_SIZE,
        "Chunks deleted successfully using bulk DELETE"
    );

    Ok(())
}
