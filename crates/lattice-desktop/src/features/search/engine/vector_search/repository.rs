use futures::FutureExt;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// Both values in one SQLite snapshot: separate SELECTs can combine an old
/// count with a new counter when a writer commits between them.
pub(super) async fn source_stamp_counts(
    pool: &SqlitePool,
    identity: &str,
    dimension: i64,
) -> Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT COUNT(*), COALESCE((SELECT write_counter FROM vector_index_state WHERE id = 1), 0) FROM text_chunks tc \
         LEFT JOIN text_embeddings te ON te.chunk_id = tc.id AND te.model_name = ? AND te.dimension = ? \
         LEFT JOIN embedding_generation_vectors eg ON eg.chunk_id = tc.id AND eg.model_identity = ? AND eg.dimension = ? \
         WHERE te.chunk_id IS NOT NULL OR eg.chunk_id IS NOT NULL",
    )
    .bind(identity)
    .bind(dimension)
    .bind(identity)
    .bind(dimension)
    .fetch_one(pool).boxed()
    .await?)
}
