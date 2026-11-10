//! Resumable preparation of a new vector space. Existing vectors remain untouched.
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::embedding_encoding;
use sha2::{Digest, Sha256};

pub fn content_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub async fn chunk_page_after(
    pool: &impl DriverPool,
    cursor: &str,
) -> Result<Vec<(String, String)>> {
    let pool = pool.sqlx_pool();
    Ok(sqlx::query_as("SELECT id, COALESCE(NULLIF(contextualized_content, ''), content) FROM text_chunks WHERE id > ? ORDER BY id LIMIT 64")
        .bind(cursor).fetch_all(pool).await?)
}

pub async fn is_prepared(
    pool: &impl DriverPool,
    identity: &str,
    chunk_id: &str,
    hash: &str,
    dimension: usize,
) -> Result<bool> {
    let pool = pool.sqlx_pool();
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_generation_vectors WHERE model_identity = ? AND chunk_id = ? AND content_hash = ? AND dimension = ?")
        .bind(identity).bind(chunk_id).bind(hash).bind(dimension as i64).fetch_one(pool).await?;
    Ok(exists > 0)
}

/// The statement rechecks the chunk's text, so a chunk deleted or edited while
/// inference ran stores nothing.
pub async fn store_prepared(
    pool: &impl DriverPool,
    identity: &str,
    hash: &str,
    vector: &[f32],
    chunk_id: &str,
    text: &str,
) -> Result<()> {
    let pool = pool.sqlx_pool();
    sqlx::query("INSERT OR REPLACE INTO embedding_generation_vectors (model_identity, chunk_id, content_hash, embedding, dimension) SELECT ?, id, ?, ?, ? FROM text_chunks WHERE id = ? AND COALESCE(NULLIF(contextualized_content, ''), content) = ?")
        .bind(identity).bind(hash).bind(embedding_encoding::encode_embedding(vector)).bind(vector.len() as i64).bind(chunk_id).bind(text).execute(pool).await?;
    Ok(())
}

pub async fn count_chunks_with_content(pool: &impl DriverPool) -> Result<i64> {
    let pool = pool.sqlx_pool();
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE content != ''")
            .fetch_one(pool)
            .await?,
    )
}

/// Never restore vectors from another model, even if dimensions happen to match.
pub async fn restore(
    pool: &impl DriverPool,
    identity: &str,
    dimension: usize,
) -> Result<Vec<(String, Vec<f32>, String, String, String)>> {
    let pool = pool.sqlx_pool();
    let mut result = Vec::new();
    let mut cursor = String::new();
    loop {
        let page = restore_page(pool, identity, dimension, &cursor, 128).await?;
        if page.cursor.is_empty() {
            break;
        }
        cursor = page.cursor;
        result.extend(page.entries);
    }
    Ok(result)
}

/// Read and decode one bounded keyset page. `chunk_id` is kept in each row so
/// the caller can advance without offset scans or retaining all embeddings.
pub struct RestorePage {
    pub entries: Vec<(String, Vec<f32>, String, String, String)>,
    pub cursor: String,
}

pub async fn restore_page(
    pool: &impl DriverPool,
    identity: &str,
    dimension: usize,
    after_chunk_id: &str,
    limit: i64,
) -> Result<RestorePage> {
    let pool = pool.sqlx_pool();
    if limit <= 0 {
        return Err(AppError::InvalidInput(
            "Embedding restore page limit must be positive".into(),
        ));
    }
    // Keep this helper bounded even when a caller supplies a very large SQL
    // LIMIT. Negative LIMIT values in SQLite mean unlimited rows.
    const MAX_RESTORE_PAGE_SIZE: i64 = 128;
    let limit = limit.min(MAX_RESTORE_PAGE_SIZE);
    type RestoredRow = (String, Vec<u8>, String, Option<String>, String);
    let rows: Vec<RestoredRow> = sqlx::query_as(
        "SELECT tc.id, COALESCE(te.embedding, eg.embedding), tc.document_id, CASE WHEN te.embedding IS NULL THEN eg.content_hash ELSE NULL END, COALESCE(NULLIF(tc.contextualized_content, ''), tc.content) FROM text_chunks tc LEFT JOIN text_embeddings te ON te.chunk_id = tc.id AND te.model_name = ? AND te.dimension = ? LEFT JOIN embedding_generation_vectors eg ON eg.chunk_id = tc.id AND eg.model_identity = ? AND eg.dimension = ? WHERE (te.embedding IS NOT NULL OR eg.embedding IS NOT NULL) AND tc.id > ? ORDER BY tc.id LIMIT ?")
        .bind(identity).bind(dimension as i64).bind(identity).bind(dimension as i64).bind(after_chunk_id).bind(limit).fetch_all(pool).await?;
    let cursor = rows.last().map(|row| row.0.clone()).unwrap_or_default();
    let mut result = Vec::with_capacity(rows.len());
    for (id, bytes, doc_id, hash, source) in rows {
        if hash
            .as_ref()
            .is_some_and(|expected| *expected != content_hash(&source))
        {
            continue;
        }
        let vector = embedding_encoding::decode_embedding(&bytes)?;
        if vector.len() != dimension || vector.iter().any(|v| !v.is_finite()) {
            return Err(AppError::InvalidState("Invalid persisted embedding".into()));
        }
        result.push((
            embedding_encoding::vector_key(&id),
            vector,
            String::new(),
            id,
            doc_id,
        ));
    }
    Ok(RestorePage {
        entries: result,
        cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE text_chunks (id TEXT PRIMARY KEY, content TEXT, contextualized_content TEXT, document_id TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE text_embeddings (chunk_id TEXT, model_name TEXT, dimension INTEGER, embedding BLOB)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE embedding_generation_vectors (model_identity TEXT NOT NULL, chunk_id TEXT NOT NULL REFERENCES text_chunks(id) ON DELETE CASCADE, content_hash TEXT NOT NULL, embedding BLOB NOT NULL, dimension INTEGER NOT NULL, PRIMARY KEY(model_identity, chunk_id))").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO text_chunks VALUES ('a', 'alpha', NULL, 'doc'), ('b', 'beta', NULL, 'doc')").execute(&pool).await.unwrap();
        pool
    }
    #[tokio::test]
    async fn restore_page_rejects_unbounded_nonpositive_limits() {
        let pool = database().await;
        assert!(restore_page(&pool, "new", 2, "", 0).await.is_err());
        assert!(restore_page(&pool, "new", 2, "", -1).await.is_err());
    }

    #[tokio::test]
    async fn oversized_restore_page_limit_stays_bounded_and_cursor_continues() {
        let pool = database().await;
        for index in 0..130 {
            let id = format!("c{index:03}");
            let text = format!("content-{index}");
            sqlx::query("INSERT INTO text_chunks (id, content, document_id) VALUES (?, ?, 'doc')")
                .bind(&id)
                .bind(&text)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO embedding_generation_vectors (model_identity, chunk_id, content_hash, embedding, dimension) VALUES ('new', ?, ?, ?, 2)")
                .bind(&id)
                .bind(content_hash(&text))
                .bind(embedding_encoding::encode_embedding(&[1.0, 0.0]))
                .execute(&pool)
                .await
                .unwrap();
        }

        let first = restore_page(&pool, "new", 2, "", i64::MAX).await.unwrap();
        assert_eq!(first.entries.len(), 128);
        assert_eq!(first.cursor, "c127");
        let second = restore_page(&pool, "new", 2, &first.cursor, i64::MAX)
            .await
            .unwrap();
        assert_eq!(second.entries.len(), 2);
        assert_eq!(second.cursor, "c129");
        let end = restore_page(&pool, "new", 2, &second.cursor, i64::MAX)
            .await
            .unwrap();
        assert!(end.entries.is_empty());
        assert!(end.cursor.is_empty());
    }
}
