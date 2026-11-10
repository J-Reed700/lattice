//! Storage statistics queries.

use crate::features::indexing::engine::error::Result;
use lattice_core::shared::persistence::DatabaseHandle;

/// Get count of indexed documents.
///
/// Files attached to a chat are indexed too, but they are not library
/// documents and the library listing does not show them. Counting them here
/// would put a number on the dashboard that the library beside it contradicts.
pub async fn get_indexed_count(pool: &impl DatabaseHandle) -> Result<i64> {
    let pool = pool.sqlx_pool();
    let count = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM documents
        WHERE status = 'indexed' AND owner_conversation_id IS NULL
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Get total number of text chunks.
pub async fn get_total_chunks(pool: &impl DatabaseHandle) -> Result<i64> {
    let pool = pool.sqlx_pool();
    let count = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM text_chunks
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}
