//! Storage statistics queries.

use crate::features::indexing::engine::error::Result;
use sqlx::SqlitePool;

/// Get count of indexed documents.
pub async fn get_indexed_count(pool: &SqlitePool) -> Result<i64> {
    let count = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM documents WHERE status = 'indexed'
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Get total number of text chunks.
pub async fn get_total_chunks(pool: &SqlitePool) -> Result<i64> {
    let count = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM text_chunks
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}
