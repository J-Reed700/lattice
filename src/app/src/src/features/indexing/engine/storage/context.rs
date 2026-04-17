//! Contextualized chunk storage operations.

use super::checksum::calculate_checksum;
use crate::infrastructure::indexing::chunker::ContextualizedChunk;
use crate::infrastructure::indexing::error::{IndexingError, Result};
use crate::features::embedding::service::MODEL_NAME;
use crate::shared::utils::path::path_to_string;
use chrono::Utc;
use serde::Deserialize;
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::path::Path;
use uuid::Uuid;

/// Metadata structure from web article metadata.json
#[derive(Debug, Deserialize)]
struct WebArticleMetadata {
    title: String,
}

/// Check if path is a web article (has sibling metadata.json)
fn is_web_article(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        parent.join("metadata.json").exists()
    } else {
        false
    }
}

/// Extract title from web article metadata.json
async fn get_web_article_title(path: &Path) -> Result<String> {
    let parent = path.parent().ok_or_else(|| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: "Path has no parent directory".to_string(),
    })?;

    let metadata_path = parent.join("metadata.json");
    let metadata_content = tokio::fs::read_to_string(&metadata_path)
        .await
        .map_err(|e| IndexingError::FileRead {
            path: metadata_path.display().to_string(),
            reason: format!("Failed to read metadata.json: {}", e),
        })?;

    let metadata: WebArticleMetadata =
        serde_json::from_str(&metadata_content).map_err(|e| IndexingError::FileRead {
            path: metadata_path.display().to_string(),
            reason: format!("Failed to parse metadata.json: {}", e),
        })?;

    Ok(metadata.title)
}

/// Helper to insert document metadata.
async fn insert_document_metadata(
    tx: &mut Transaction<'_, Sqlite>,
    doc_id: &str,
    path: &Path,
    mime_type: &str,
) -> Result<()> {
    // Extract file_name - use title for web articles
    let file_name = if is_web_article(path) {
        // Web article: extract title from metadata.json
        match get_web_article_title(path).await {
            Ok(title) => title,
            Err(e) => {
                tracing::warn!(
                    "Failed to extract title from web article metadata.json, using filename: {}",
                    e
                );
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            }
        }
    } else {
        // Local file: use filename
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    };

    let file_path = path_to_string(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: format!("Invalid path encoding: {}", e),
    })?;

    // Read filesystem metadata
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
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Helper to insert chunks and embeddings.
async fn insert_chunks_and_embeddings(
    tx: &mut Transaction<'_, Sqlite>,
    doc_id: &str,
    chunks: &[ContextualizedChunk],
    embeddings: &[Vec<f32>],
) -> Result<()> {
    sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;

    for (idx, chunk) in chunks.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO text_chunks (
                id, document_id, content, contextualized_content,
                context_prefix, chunk_index, start_char, end_char
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chunk_id)
        .bind(doc_id)
        .bind(&chunk.original_content)
        .bind(&chunk.contextualized_content)
        .bind(&chunk.context_prefix)
        .bind(idx as i32)
        .bind(chunk.start_idx as i32)
        .bind(chunk.end_idx as i32)
        .execute(&mut **tx)
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
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// Store document with contextualized chunks.
pub async fn store_document_with_context(
    pool: &SqlitePool,
    path: &Path,
    mime_type: &str,
    chunks: Vec<ContextualizedChunk>,
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

    insert_document_metadata(&mut tx, &doc_id, path, mime_type).await?;
    insert_chunks_and_embeddings(&mut tx, &doc_id, &chunks, &embeddings).await?;

    tx.commit().await?;

    Ok(doc_id)
}

/// Store document with context and file ID.
pub async fn store_document_with_context_and_file(
    pool: &SqlitePool,
    path: &Path,
    _file_id: &str,
    mime_type: &str,
    chunks: Vec<ContextualizedChunk>,
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

    insert_document_metadata(&mut tx, &doc_id, path, mime_type).await?;
    insert_chunks_and_embeddings(&mut tx, &doc_id, &chunks, &embeddings).await?;

    tx.commit().await?;

    Ok(doc_id)
}

/// Store document with context using external transaction.
pub async fn store_document_with_context_and_file_tx(
    tx: &mut Transaction<'_, Sqlite>,
    path: &Path,
    _file_id: &str,
    mime_type: &str,
    chunks: Vec<ContextualizedChunk>,
    embeddings: Vec<Vec<f32>>,
) -> Result<String> {
    if chunks.len() != embeddings.len() {
        return Err(IndexingError::InvalidConfig(format!(
            "Chunks and embeddings must have same length: {} chunks vs {} embeddings",
            chunks.len(),
            embeddings.len()
        )));
    }

    let doc_id = Uuid::new_v4().to_string();

    insert_document_metadata(tx, &doc_id, path, mime_type).await?;
    insert_chunks_and_embeddings(tx, &doc_id, &chunks, &embeddings).await?;

    Ok(doc_id)
}
