//! Text chunk and embedding storage operations.

use super::checksum::calculate_checksum;
use crate::features::embedding::service::MODEL_NAME;
use crate::features::mentions::repository::MentionRepository;
use crate::infrastructure::indexing::chunker::TextChunk;
use crate::infrastructure::indexing::error::{IndexingError, Result};
use crate::shared::utils::path::path_to_string;
use chrono::Utc;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Type alias for a single chunked document (path, mime_type, chunks, embeddings).
pub type ChunkedDocument = (PathBuf, String, Vec<TextChunk>, Vec<Vec<f32>>);

/// Type alias for multiple chunked documents.
pub type ChunkedDocuments = Vec<ChunkedDocument>;

/// Store a document with text chunks and embeddings.
pub async fn store_document(
    pool: &SqlitePool,
    path: &Path,
    mime_type: &str,
    chunks: Vec<TextChunk>,
    embeddings: Vec<Vec<f32>>,
) -> Result<String> {
    if chunks.len() != embeddings.len() {
        return Err(IndexingError::InvalidConfig(format!(
            "Chunks and embeddings must have same length: {} chunks vs {} embeddings",
            chunks.len(),
            embeddings.len()
        )));
    }

    let mut tx = pool.begin().await?;

    let doc_id = Uuid::new_v4().to_string();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let file_path = path_to_string(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: format!("Invalid path encoding: {}", e),
    })?;

    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::FileRead {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    let size_bytes = metadata.len() as i64;

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let checksum = calculate_checksum(path).await?;

    let indexed_at = Utc::now().to_rfc3339();
    let modified_at_str = chrono::DateTime::from_timestamp(modified_at as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| {
            tracing::warn!("Invalid timestamp {}, using current time", modified_at);
            indexed_at.clone()
        });

    // `RETURNING id` because the upsert's DO UPDATE branch keeps the row's
    // original id. Re-indexing a modified file otherwise bound chunks to a
    // freshly-generated UUID that no document row has, so the delete below
    // matched nothing and the chunk inserts died on a foreign-key violation.
    // Non-macro form to avoid regenerating the offline sqlx cache.
    let doc_id: String = sqlx::query_scalar(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, mime_type,
            size_bytes, modified_at, indexed_at,
            checksum, status
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'indexed')
        ON CONFLICT(file_path) DO UPDATE SET
            indexed_at = excluded.indexed_at,
            checksum = excluded.checksum,
            status = 'indexed',
            updated_at = CURRENT_TIMESTAMP
        RETURNING id
        "#,
    )
    .bind(&doc_id)
    .bind(&file_path)
    .bind(file_name)
    .bind(mime_type)
    .bind(size_bytes)
    .bind(&modified_at_str)
    .bind(&indexed_at)
    .bind(&checksum)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
        .bind(&doc_id)
        .execute(&mut *tx)
        .await?;

    for (idx, chunk) in chunks.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();
        let chunk_index = idx as i32;
        let start_char = chunk.start_idx as i32;
        let end_char = chunk.end_idx as i32;

        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index, start_char, end_char) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&chunk_id)
        .bind(&doc_id)
        .bind(&chunk.text)
        .bind(chunk_index)
        .bind(start_char)
        .bind(end_char)
        .execute(&mut *tx)
        .await?;

        let embedding = embeddings.get(idx).ok_or_else(|| {
            IndexingError::InvalidData(format!("Embedding index {} out of bounds", idx))
        })?;

        let embedding_bytes = crate::features::embedding::encoding::encode_embedding(embedding);

        // Canonical key so delete/rebuild/insert all agree — see
        // `embedding::encoding::vector_key`.
        let embedding_id = crate::features::embedding::encoding::vector_key(&chunk_id);
        let dimension = embedding.len() as i32;

        sqlx::query!(
            r#"
            INSERT INTO text_embeddings (
                id, chunk_id, embedding, model_name, dimension
            )
            VALUES (?, ?, ?, ?, ?)
            "#,
            embedding_id,
            chunk_id,
            embedding_bytes,
            MODEL_NAME,
            dimension,
        )
        .execute(&mut *tx)
        .await?;
    }

    let full_text = chunks
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    tx.commit().await?;

    // Mentions are written *after* the commit, deliberately.
    //
    // `MentionRepository` works through the pool, i.e. on a different pooled
    // connection. Calling it while the transaction above was still open meant
    // a second connection asking for SQLite's single writer lock that this
    // very task was holding — a guaranteed self-deadlock that burned the full
    // 5-second `busy_timeout` on *every indexed document*, then failed with
    // SQLITE_BUSY and dropped the mentions via a warning nobody reads.
    // Indexing a 500-file folder spent ~42 minutes purely waiting on a lock
    // it could never acquire.
    //
    // Never open a second connection inside a write transaction on SQLite.
    // Mentions are derived data, so recomputing them outside the transaction
    // costs only that they aren't atomic with the document — far better than
    // never being persisted at all.
    let mention_repo = MentionRepository::new(pool.clone());
    if let Err(e) = mention_repo
        .extract_and_store_mentions(&doc_id, &full_text)
        .await
    {
        tracing::warn!("Failed to extract mentions for document {}: {}", doc_id, e);
    }

    Ok(doc_id)
}

/// Store multiple documents in batch.
pub async fn batch_store_documents(
    pool: &SqlitePool,
    documents: ChunkedDocuments,
) -> Result<Vec<String>> {
    let mut doc_ids = Vec::new();

    for (path, mime_type, chunks, embeddings) in documents {
        let doc_id = store_document(pool, &path, &mime_type, chunks, embeddings).await?;
        doc_ids.push(doc_id);
    }

    Ok(doc_ids)
}
