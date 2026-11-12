//! Chunk Repository Operations
//!
//! Generic SQL operations for chunk persistence that work with any SQLite executor.
//! These functions can be used with both connection pools and transactions.

use crate::infrastructure::persistence::mappers::{ChunkMapper, ChunkModel};
use lattice_core::domain::entities::chunk::Chunk as ChunkEntity;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::bind_limit::rows_per_statement;
use lattice_core::shared::persistence::WriteTx;
use sqlx::QueryBuilder;
use sqlx::Row;
use sqlx::SqliteConnection;
use std::collections::HashMap;

const IDS_PER_STATEMENT: usize = rows_per_statement(1, 0);
/// Chunk rows one INSERT takes: fifteen bound fields each.
pub(super) const CHUNK_ROWS_PER_INSERT: usize = rows_per_statement(15, 0);

pub async fn save(tx: &mut WriteTx, chunk: &ChunkEntity) -> Result<()> {
    let model = ChunkMapper::to_model(chunk)?;

    sqlx::query(
        r#"
        INSERT INTO text_chunks (
            id, document_id, content, chunk_index,
            contextualized_content, context_prefix, start_char, end_char,
            language, token_count, word_count, has_code, section, page_number, from_ocr
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            section = excluded.section, page_number = excluded.page_number,
            from_ocr = excluded.from_ocr
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
    .bind(model.from_ocr)
    .execute(&mut **tx)
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
            token_count, word_count, has_code, section, page_number, from_ocr
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
            token_count, word_count, has_code, section, page_number, from_ocr
        FROM text_chunks
        ORDER BY document_id, chunk_index ASC
        "#,
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list all chunks: {}", e)))?;

    ChunkMapper::to_entities(&db_models)
}

pub async fn find_by_ids(conn: &mut SqliteConnection, ids: &[String]) -> Result<Vec<ChunkEntity>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut all = Vec::new();

    for batch in ids.chunks(IDS_PER_STATEMENT) {
        let mut query_builder = QueryBuilder::new(
            r#"
            SELECT
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix,
                start_char, end_char, language,
                token_count, word_count, has_code, section, page_number, from_ocr
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

        all.extend(ChunkMapper::to_entities(&db_models)?);
    }

    Ok(all)
}

/// The text of each of these chunks, keyed by chunk id.
pub async fn find_content_by_ids(
    conn: &mut SqliteConnection,
    ids: &[String],
) -> Result<HashMap<String, String>> {
    let mut content = HashMap::with_capacity(ids.len());
    for batch in ids.chunks(IDS_PER_STATEMENT) {
        let mut query =
            QueryBuilder::<sqlx::Sqlite>::new("SELECT id, content FROM text_chunks WHERE id IN (");
        let mut separated = query.separated(", ");
        for id in batch {
            separated.push_bind(id);
        }
        query.push(")");
        let rows = query.build().fetch_all(&mut *conn).await.map_err(|error| {
            AppError::Database(format!("Failed to fetch candidate chunk content: {error}"))
        })?;
        for row in rows {
            let id: String = row
                .try_get("id")
                .map_err(|error| AppError::Database(format!("row.id: {error}")))?;
            let body: String = row
                .try_get("content")
                .map_err(|error| AppError::Database(format!("row.content: {error}")))?;
            content.insert(id, body);
        }
    }
    Ok(content)
}

/// Ids of the chunks of documents attached to a conversation.
pub async fn find_conversation_attached_chunk_ids(
    conn: &mut SqliteConnection,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT tc.id FROM text_chunks tc JOIN documents d ON d.id = tc.document_id WHERE d.owner_conversation_id IS NOT NULL",
    )
    .fetch_all(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to list conversation attachment chunks: {e}"
        ))
    })
}

pub async fn find_by_document(
    conn: &mut SqliteConnection,
    document_id: &str,
) -> Result<Vec<ChunkEntity>> {
    find_by_document_limit(conn, document_id, None).await
}

pub async fn find_by_document_limit(
    conn: &mut SqliteConnection,
    document_id: &str,
    limit: Option<usize>,
) -> Result<Vec<ChunkEntity>> {
    let limit = limit
        .map(i64::try_from)
        .transpose()
        .map_err(|_| AppError::InvalidInput("Chunk limit exceeds SQLite range".into()))?
        .unwrap_or(-1);
    let db_models = sqlx::query_as::<_, ChunkModel>(
        r#"
        SELECT
            id, document_id, content, chunk_index,
            contextualized_content, context_prefix,
            start_char, end_char, language,
            token_count, word_count, has_code, section, page_number, from_ocr
        FROM text_chunks
        WHERE document_id = ?
        ORDER BY chunk_index ASC
        LIMIT ?
        "#,
    )
    .bind(document_id)
    .bind(limit)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find chunks by document: {}", e)))?;

    ChunkMapper::to_entities(&db_models)
}

pub async fn delete(tx: &mut WriteTx, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM text_chunks WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete chunk: {}", e)))?;
    Ok(())
}

pub async fn delete_by_document(tx: &mut WriteTx, document_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut **tx)
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
pub async fn save_batch_optimized(tx: &mut WriteTx, chunks: &[ChunkEntity]) -> Result<()> {
    use sqlx::QueryBuilder;
    use std::time::Instant;
    use tracing::info;

    if chunks.is_empty() {
        return Ok(());
    }

    let start = Instant::now();

    let models: Vec<ChunkModel> = chunks
        .iter()
        .map(ChunkMapper::to_model)
        .collect::<Result<_>>()?;
    let batch_count = models.len().div_ceil(CHUNK_ROWS_PER_INSERT);

    for batch in models.chunks(CHUNK_ROWS_PER_INSERT) {
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO text_chunks (
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix, start_char, end_char,
                language, token_count, word_count, has_code, section, page_number, from_ocr
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
                .push_bind(model.page_number)
                .push_bind(model.from_ocr);
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
             section = excluded.section, \
             page_number = excluded.page_number, \
             from_ocr = excluded.from_ocr",
        );

        query_builder
            .build()
            .execute(&mut **tx)
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
        batch_size = CHUNK_ROWS_PER_INSERT,
        chunks_per_second = format!("{:.1}", chunks_per_second),
        "Chunks saved successfully using bulk INSERT"
    );

    Ok(())
}

/// Optimized batch delete for multiple chunks
/// Uses single DELETE with IN clause to reduce database round-trips
pub async fn delete_batch_optimized(tx: &mut WriteTx, ids: &[&str]) -> Result<()> {
    use std::time::Instant;
    use tracing::info;

    if ids.is_empty() {
        return Ok(());
    }

    let start = Instant::now();

    for batch in ids.chunks(IDS_PER_STATEMENT) {
        let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("DELETE FROM text_chunks WHERE id IN ({})", placeholders);

        let mut query_builder = sqlx::query(&query);
        for id in batch {
            query_builder = query_builder.bind(id);
        }

        query_builder
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete chunk batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    info!(
        duration_ms = elapsed.as_millis(),
        id_count = ids.len(),
        batch_size = IDS_PER_STATEMENT,
        "Chunks deleted successfully using bulk DELETE"
    );

    Ok(())
}
