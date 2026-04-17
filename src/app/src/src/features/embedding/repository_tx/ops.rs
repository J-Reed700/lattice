//! Embedding Repository Operations
//!
//! Generic SQL operations for embedding persistence that work with any SQLite executor.
//! These functions can be used with both connection pools and transactions.

use crate::domain::entities::embedding::Embedding as DomainEmbedding;
use crate::infrastructure::persistence::mappers::embedding_mapper::{
    EmbeddingDTO, EmbeddingMapper,
};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqliteConnection};

// Database row struct for embedding queries
#[derive(Debug, sqlx::FromRow)]
struct EmbeddingRow {
    chunk_id: String,
    embedding: Vec<u8>,
    model_name: String,
    dimension: i64,
    created_at: String,
}

pub async fn save(
    conn: &mut SqliteConnection,
    entity: &DomainEmbedding,
    vector: Vec<f32>,
) -> Result<()> {
    EmbeddingMapper::validate_dimension(entity, &vector)?;

    let dto = EmbeddingMapper::to_dto(entity, vector);

    let embedding_bytes =
        bincode::serialize(&dto.embedding).map_err(|e| AppError::Serialization(e.to_string()))?;

    let id = format!("emb_{}", dto.chunk_id);
    let dimension = dto.dimension as i32;
    let created_at = dto.computed_at.to_rfc3339();

    sqlx::query!(
        r#"
        INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            chunk_id = excluded.chunk_id,
            embedding = excluded.embedding,
            model_name = excluded.model_name,
            dimension = excluded.dimension,
            created_at = excluded.created_at
        "#,
        id,
        dto.chunk_id,
        embedding_bytes,
        dto.model_name,
        dimension,
        created_at
    )
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to save embedding: {}", e)))?;

    Ok(())
}

pub async fn save_batch(
    conn: &mut SqliteConnection,
    entries: Vec<(DomainEmbedding, Vec<f32>)>,
) -> Result<()> {
    use sqlx::QueryBuilder;
    use std::time::Instant;
    use tracing::info;

    if entries.is_empty() {
        return Ok(());
    }

    let start = Instant::now();

    // Batch size: 6 columns per embedding, 999 param limit
    // 999 / 6 = 166, use 150 for safety
    const EMBEDDING_BATCH_SIZE: usize = 150;

    // Prepare all DTOs and serialized embeddings first
    let prepared: Result<Vec<_>> = entries
        .into_iter()
        .map(|(entity, vector)| {
            let dto = EmbeddingMapper::to_dto(&entity, vector);
            let embedding_bytes = bincode::serialize(&dto.embedding)
                .map_err(|e| AppError::Serialization(e.to_string()))?;
            let id = format!("emb_{}", dto.chunk_id);
            let dimension = dto.dimension as i32;
            let created_at = dto.computed_at.to_rfc3339();
            Ok((
                id,
                dto.chunk_id,
                embedding_bytes,
                dto.model_name,
                dimension,
                created_at,
            ))
        })
        .collect();

    let prepared = prepared?;
    let total_count = prepared.len();
    let batch_count = total_count.div_ceil(EMBEDDING_BATCH_SIZE);

    for batch in prepared.chunks(EMBEDDING_BATCH_SIZE) {
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension, created_at) "
        );

        query_builder.push_values(batch, |mut b, row| {
            b.push_bind(&row.0) // id
                .push_bind(&row.1) // chunk_id
                .push_bind(&row.2) // embedding_bytes
                .push_bind(&row.3) // model_name
                .push_bind(row.4) // dimension
                .push_bind(&row.5); // created_at
        });

        query_builder.push(
            " ON CONFLICT(id) DO UPDATE SET \
             chunk_id = excluded.chunk_id, \
             embedding = excluded.embedding, \
             model_name = excluded.model_name, \
             dimension = excluded.dimension, \
             created_at = excluded.created_at",
        );

        query_builder
            .build()
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert embedding batch: {}", e)))?;
    }

    let elapsed = start.elapsed();
    let embeddings_per_second = if elapsed.as_secs_f64() > 0.0 {
        total_count as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    info!(
        duration_ms = elapsed.as_millis(),
        embedding_count = total_count,
        batch_count = batch_count,
        batch_size = EMBEDDING_BATCH_SIZE,
        embeddings_per_second = format!("{:.1}", embeddings_per_second),
        "Embeddings saved successfully using bulk INSERT"
    );

    Ok(())
}

pub async fn find_by_chunk_id(
    conn: &mut SqliteConnection,
    chunk_id: &str,
) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
    let record = sqlx::query_as::<_, EmbeddingRow>(
        r#"
        SELECT chunk_id, embedding, model_name, dimension, created_at
        FROM text_embeddings
        WHERE chunk_id = ?
        "#,
    )
    .bind(chunk_id)
    .fetch_optional(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find embedding: {}", e)))?;

    match record {
        Some(rec) => {
            let vector: Vec<f32> = bincode::deserialize(&rec.embedding)
                .map_err(|e| AppError::Deserialization(e.to_string()))?;

            let computed_at = DateTime::parse_from_rfc3339(&rec.created_at)
                .map_err(|e| AppError::Parsing(e.to_string()))?
                .with_timezone(&Utc);

            let dto = EmbeddingDTO {
                chunk_id: rec.chunk_id,
                embedding: vector.clone(),
                model_name: rec.model_name,
                dimension: rec.dimension as usize,
                computed_at,
                model_version: None,
            };

            let entity = EmbeddingMapper::from_dto(&dto)?;
            Ok(Some((entity, vector)))
        }
        None => Ok(None),
    }
}

pub async fn find_by_document_id(
    conn: &mut SqliteConnection,
    document_id: &str,
) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
    let records = sqlx::query_as::<_, EmbeddingRow>(
        r#"
        SELECT e.chunk_id, e.embedding, e.model_name, e.dimension, e.created_at
        FROM text_embeddings e
        INNER JOIN text_chunks c ON e.chunk_id = c.id
        WHERE c.document_id = ?
        ORDER BY c.chunk_index ASC
        "#,
    )
    .bind(document_id)
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to find embeddings: {}", e)))?;

    let mut results = Vec::new();
    for rec in records {
        let vector: Vec<f32> = bincode::deserialize(&rec.embedding)
            .map_err(|e| AppError::Deserialization(e.to_string()))?;

        let computed_at = DateTime::parse_from_rfc3339(&rec.created_at)
            .map_err(|e| AppError::Parsing(e.to_string()))?
            .with_timezone(&Utc);

        let dto = EmbeddingDTO {
            chunk_id: rec.chunk_id,
            embedding: vector.clone(),
            model_name: rec.model_name,
            dimension: rec.dimension as usize,
            computed_at,
            model_version: None,
        };

        let entity = EmbeddingMapper::from_dto(&dto)?;
        results.push((entity, vector));
    }

    Ok(results)
}

pub async fn delete_by_chunk_id(conn: &mut SqliteConnection, chunk_id: &str) -> Result<()> {
    sqlx::query!(
        r#"DELETE FROM text_embeddings WHERE chunk_id = ?"#,
        chunk_id
    )
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to delete embedding: {}", e)))?;

    Ok(())
}

pub async fn delete_by_document_id(conn: &mut SqliteConnection, document_id: &str) -> Result<()> {
    sqlx::query(
        r#"
        DELETE FROM text_embeddings
        WHERE chunk_id IN (
            SELECT id FROM text_chunks WHERE document_id = ?
        )
        "#,
    )
    .bind(document_id)
    .execute(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to delete embeddings: {}", e)))?;

    Ok(())
}

pub async fn count(conn: &mut SqliteConnection) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_embeddings")
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count embeddings: {}", e)))
}
