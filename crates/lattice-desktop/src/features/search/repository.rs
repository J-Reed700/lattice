use crate::infrastructure::persistence::database::connection::query_with_timeout;
use lattice_core::shared::error::AppError;
use sqlx::{Row, SqlitePool};

pub(super) struct ChunkMetadataRow {
    pub chunk_id: String,
    pub content: String,
    pub document_id: String,
    pub chunk_index: i64,
    pub start_char: Option<i64>,
    pub end_char: Option<i64>,
    pub file_name: String,
    pub file_path: String,
    pub file_type: Option<String>,
    pub size_bytes: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Metadata for a batch of chunk IDs.
pub(super) async fn chunk_metadata(
    pool: &SqlitePool,
    batch: &[String],
) -> Result<Vec<ChunkMetadataRow>, AppError> {
    let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

    let query = format!(
        r#"
        SELECT
            tc.id as chunk_id,
            tc.content,
            tc.document_id,
            tc.chunk_index,
            tc.start_char,
            tc.end_char,
            d.file_name,
            d.file_path,
            d.file_type,
            d.size_bytes,
            d.created_at,
            d.updated_at
        FROM text_chunks tc
        JOIN documents d ON tc.document_id = d.id
        WHERE tc.id IN ({})
          -- A file attached to a chat is not part of the vault. Its chunks
          -- are indexed so that chat can read and cite it, and every
          -- vault-wide search command resolves its hits through this one
          -- statement, so refusing it a document here is what keeps it off
          -- the search page. The callers drop a hit they cannot resolve.
          AND d.owner_conversation_id IS NULL
        "#,
        placeholders
    );

    let mut query_builder = sqlx::query(&query);
    for id in batch {
        query_builder = query_builder.bind(id);
    }

    let rows: Vec<sqlx::sqlite::SqliteRow> =
        query_with_timeout(|| async { query_builder.fetch_all(pool).await }).await?;

    Ok(rows
        .into_iter()
        .map(|row| ChunkMetadataRow {
            chunk_id: row.get("chunk_id"),
            content: row.get("content"),
            document_id: row.get("document_id"),
            chunk_index: row.get("chunk_index"),
            start_char: row.try_get("start_char").ok(),
            end_char: row.try_get("end_char").ok(),
            file_name: row.get("file_name"),
            file_path: row.get("file_path"),
            file_type: row.get("file_type"),
            size_bytes: row.get("size_bytes"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}
