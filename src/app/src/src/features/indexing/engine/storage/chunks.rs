//! Text chunk and embedding storage operations.

use super::checksum::calculate_checksum;
use crate::infrastructure::indexing::chunker::TextChunk;
use crate::infrastructure::indexing::error::{IndexingError, Result};
use crate::features::mentions::repository::MentionRepository;
use crate::infrastructure::services::embedding::MODEL_NAME;
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

    sqlx::query!(
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
        "#,
        doc_id,
        file_path,
        file_name,
        mime_type,
        size_bytes,
        modified_at_str,
        indexed_at,
        checksum,
    )
    .execute(&mut *tx)
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

        let embedding_bytes = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect::<Vec<u8>>();

        let embedding_id = Uuid::new_v4().to_string();
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

    // Extract mentions from document content
    let mention_repo = MentionRepository::new(pool.clone());
    let full_text = chunks
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if let Err(e) = mention_repo
        .extract_and_store_mentions(&doc_id, &full_text)
        .await
    {
        tracing::warn!("Failed to extract mentions for document {}: {}", doc_id, e);
    }

    tx.commit().await?;

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
