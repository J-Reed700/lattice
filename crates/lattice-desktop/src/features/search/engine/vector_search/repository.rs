use super::manifest::SourceStamp;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::driver::DriverPool;

/// Read the stamp for one generation.
///
/// Two aggregate queries against covering indexes; no embedding blob is
/// touched. The counter is a single-row primary key lookup.
pub async fn read_source_stamp(
    pool: &impl DriverPool,
    identity: &str,
    dimension: usize,
) -> Result<SourceStamp> {
    let dimension = dimension as i64;
    // Mirrors `generation::restore`'s row set. `restore` also drops
    // a prepared vector whose source text has since changed, which this count
    // cannot see; that direction is safe, because the extra row makes the
    // stamp differ and buys a rebuild.
    let (row_count, write_counter) = source_stamp_counts(pool, identity, dimension).await?;

    Ok(SourceStamp {
        row_count: row_count.max(0) as u64,
        write_counter: write_counter.max(0) as u64,
    })
}

/// Both values in one SQLite snapshot: separate SELECTs can combine an old
/// count with a new counter when a writer commits between them.
pub(super) async fn source_stamp_counts(
    pool: &impl DriverPool,
    identity: &str,
    dimension: i64,
) -> Result<(i64, i64)> {
    let pool = pool.sqlx_pool();
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
    .fetch_one(pool)
    .await?)
}
