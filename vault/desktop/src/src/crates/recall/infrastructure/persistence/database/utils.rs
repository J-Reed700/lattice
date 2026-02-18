use crate::shared::error::{AppError, Result};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

/// Maximum number of parameters in a single SQLite query
///
/// SQLite has a hardcoded limit of 999 parameters per query (SQLITE_MAX_VARIABLE_NUMBER).
/// We use 900 to provide a safety margin and account for other query parameters.
///
/// Any query with dynamic IN clauses or batch operations MUST use this constant
/// and process items in chunks to avoid crashes.
///
/// Example usage:
/// ```rust
/// for batch in items.chunks(SQLITE_MAX_PARAMS) {
///     let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
///     let query = format!("SELECT * FROM table WHERE id IN ({})", placeholders);
///     // Execute query with batch...
/// }
/// ```
const SQLITE_MAX_PARAMS: usize = 900;

/// Batch size for bulk inserts
const BATCH_INSERT_SIZE: usize = 100;

pub struct DatabaseUtils;

impl DatabaseUtils {
    /// Issue #6: Fix Unbounded IN Clauses (P0 - Crashes)
    /// Query with IN clause, automatically batching to avoid SQLite 999 parameter limit
    pub async fn query_in_batches<T, F, Fut>(
        pool: &SqlitePool,
        ids: Vec<String>,
        batch_size: usize,
        query_fn: F,
    ) -> Result<Vec<T>>
    where
        F: Fn(Vec<String>) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<T>>>,
    {
        let batch_size = batch_size.min(SQLITE_MAX_PARAMS);
        let mut results = Vec::new();

        for chunk in ids.chunks(batch_size) {
            let batch_results = query_fn(chunk.to_vec()).await?;
            results.extend(batch_results);
        }

        Ok(results)
    }

    /// Issue #9: Batch Insert Optimization (P0 - 100x Faster)
    /// Insert multiple records efficiently in batches
    pub async fn batch_insert_chunks(
        pool: &SqlitePool,
        chunks: Vec<(String, String, String, i32, i32, i32)>, // (id, doc_id, content, index, start, end)
    ) -> Result<()> {
        let mut tx = pool.begin().await?;

        // Use BEGIN IMMEDIATE for proper transaction isolation
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *tx).await?;

        for batch in chunks.chunks(BATCH_INSERT_SIZE) {
            let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
                "INSERT INTO text_chunks (id, document_id, content, chunk_index, start_char, end_char) "
            );

            query_builder.push_values(batch, |mut b, chunk| {
                b.push_bind(&chunk.0)
                    .push_bind(&chunk.1)
                    .push_bind(&chunk.2)
                    .push_bind(chunk.3)
                    .push_bind(chunk.4)
                    .push_bind(chunk.5);
            });

            query_builder.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Batch insert embeddings with binary storage
    pub async fn batch_insert_embeddings(
        pool: &SqlitePool,
        embeddings: Vec<(String, String, Vec<f32>, String, i32)>, // (id, chunk_id, vector, model, dim)
    ) -> Result<()> {
        let mut tx = pool.begin().await?;

        sqlx::query("BEGIN IMMEDIATE").execute(&mut *tx).await?;

        for batch in embeddings.chunks(BATCH_INSERT_SIZE) {
            for (id, chunk_id, vector, model, dim) in batch {
                // Issue #12: Fix Embedding Storage Efficiency
                // Store as BLOB, not TEXT
                let embedding_bytes: Vec<u8> =
                    vector.iter().flat_map(|f| f.to_le_bytes()).collect();

                sqlx::query(
                    "INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) 
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(chunk_id)
                .bind(&embedding_bytes)
                .bind(model)
                .bind(dim)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    /// Deserialize embedding from BLOB storage
    pub fn deserialize_embedding(blob: &[u8], dimension: usize) -> Result<Vec<f32>> {
        blob.chunks_exact(4)
            .take(dimension)
            .map(|bytes| {
                let arr: [u8; 4] = bytes.try_into().map_err(|e| {
                    AppError::Database(format!("Failed to deserialize embedding bytes: {:?}", e))
                })?;
                Ok(f32::from_le_bytes(arr))
            })
            .collect()
    }

    /// Run periodic maintenance tasks
    pub async fn run_maintenance(pool: &SqlitePool) -> Result<()> {
        // Update statistics
        sqlx::query("ANALYZE").execute(pool).await?;

        // Check if vacuum is needed
        let size: (i64, i64) = sqlx::query_as(
            "SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()",
        )
        .fetch_one(pool)
        .await?;

        let db_size = size.0 * size.1;

        // Vacuum if over 100MB and fragmented
        if db_size > 100_000_000 {
            let freelist: (i64,) =
                sqlx::query_as("SELECT freelist_count FROM pragma_freelist_count()")
                    .fetch_one(pool)
                    .await?;

            let free_ratio = freelist.0 as f64 / size.0 as f64;

            if free_ratio > 0.2 {
                tracing::info!(
                    "Vacuuming database: size={} bytes, fragmentation={:.1}%",
                    db_size,
                    free_ratio * 100.0
                );
                sqlx::query("VACUUM").execute(pool).await?;
            }
        }

        Ok(())
    }
}

/// Issue #10: Fix SQL Injection in Dynamic Queries
/// Safe query builder that prevents SQL injection
pub struct SafeQueryBuilder {
    query: String,
    params: Vec<String>,
}

impl SafeQueryBuilder {
    pub fn new(base_query: &str) -> Self {
        Self {
            query: base_query.to_string(),
            params: Vec::new(),
        }
    }

    pub fn add_where_clause(&mut self, column: &str, values: &[String]) -> Result<()> {
        // Validate column name to prevent injection
        if !column.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(AppError::InvalidInput(format!(
                "Invalid column name: {}",
                column
            )));
        }

        if !values.is_empty() {
            let placeholders = vec!["?"; values.len()].join(", ");

            // SECURITY NOTE: This format! is SAFE despite using SQL string interpolation
            // because:
            // 1. Column name is validated above (alphanumeric + underscore only)
            // 2. Only placeholders ("?") are interpolated, NOT actual values
            // 3. Actual values are bound via .bind() in execute() method (line 201)
            // 4. This follows SQLx parameterized query pattern - SQL structure is built,
            //    but data is kept separate and properly escaped by the database driver
            //
            // Example: If column="user_id" and values=["1", "2"], this creates:
            //   Query: "WHERE user_id IN (?, ?)"
            //   Params: ["1", "2"]  <- bound separately
            //
            // NOT vulnerable to SQL injection because user data never enters SQL string.
            self.query
                .push_str(&format!(" WHERE {} IN ({})", column, placeholders));
            self.params.extend_from_slice(values);
        }

        Ok(())
    }

    pub async fn execute(&self, pool: &SqlitePool) -> Result<Vec<sqlx::sqlite::SqliteRow>> {
        let mut query = sqlx::query(&self.query);
        for param in &self.params {
            query = query.bind(param);
        }

        Ok(query.fetch_all(pool).await?)
    }
}
