//! A document's chunks and tags: reading them, and replacing them in batches.

use crate::infrastructure::persistence::mappers::{ChunkMapper, ChunkModel};
use lattice_core::domain::entities::chunk::Chunk;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::TagId;
use sqlx::{QueryBuilder, SqliteConnection};
use std::time::Instant;
use tracing::{info, instrument};

const CHUNK_BATCH_SIZE: usize = 64;
const TAG_BATCH_SIZE: usize = 450;

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
            section,
            page_number,
            from_ocr
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
                contextualized_content, context_prefix, start_char, end_char, language, token_count, word_count, has_code, section, page_number, from_ocr
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
