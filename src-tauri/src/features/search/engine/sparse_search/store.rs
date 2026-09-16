//! SQLite-backed store for per-chunk learned sparse term weights.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::application::ports::{ChunkSparseTerms, SparseTermStorePort};
use crate::shared::result::Result;

/// Bound on how many `(chunk, model, term, weight)` rows one statement binds.
///
/// SQLite's default `SQLITE_MAX_VARIABLE_NUMBER` is 32766 on modern builds and
/// 999 on older ones; four parameters per row at 200 rows stays under both with
/// room to spare, and the whole batch runs in one transaction anyway.
const ROWS_PER_STATEMENT: usize = 200;

/// Writes `chunk_sparse_terms`.
pub struct SqliteSparseTermStore {
    pool: SqlitePool,
}

impl SqliteSparseTermStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SparseTermStorePort for SqliteSparseTermStore {
    async fn replace_chunk_terms(
        &self,
        model_identity: &str,
        entries: &[ChunkSparseTerms],
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;

        // Clear first, unconditionally: a chunk whose text changed so that it
        // now activates fewer terms must not keep serving the terms it dropped.
        for (chunk_id, _) in entries {
            sqlx::query("DELETE FROM chunk_sparse_terms WHERE chunk_id = ? AND model_identity = ?")
                .bind(chunk_id)
                .bind(model_identity)
                .execute(&mut *tx)
                .await?;
        }

        let rows: Vec<(&str, u32, f32)> = entries
            .iter()
            .flat_map(|(chunk_id, sparse)| {
                sparse
                    .iter()
                    .map(move |(term_id, weight)| (chunk_id.as_str(), term_id, weight))
            })
            .collect();

        for batch in rows.chunks(ROWS_PER_STATEMENT) {
            let mut sql = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "INSERT OR REPLACE INTO chunk_sparse_terms (chunk_id, model_identity, term_id, weight) ",
            );
            sql.push_values(batch, |mut values, (chunk_id, term_id, weight)| {
                values
                    .push_bind(*chunk_id)
                    .push_bind(model_identity)
                    .push_bind(i64::from(*term_id))
                    .push_bind(f64::from(*weight));
            });
            sql.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn delete_chunk_terms(&self, chunk_ids: &[String]) -> Result<()> {
        if chunk_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for batch in chunk_ids.chunks(ROWS_PER_STATEMENT) {
            let mut sql = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "DELETE FROM chunk_sparse_terms WHERE chunk_id IN (",
            );
            let mut values = sql.separated(", ");
            for chunk_id in batch {
                values.push_bind(chunk_id);
            }
            sql.push(")");
            sql.build().execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
