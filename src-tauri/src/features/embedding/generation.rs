//! Resumable preparation of a new vector space. Existing vectors remain untouched.
use crate::application::ports::EmbeddingPort;
use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

pub fn content_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS embedding_generation_vectors (model_identity TEXT NOT NULL, chunk_id TEXT NOT NULL REFERENCES text_chunks(id) ON DELETE CASCADE, content_hash TEXT NOT NULL, embedding BLOB NOT NULL, dimension INTEGER NOT NULL, PRIMARY KEY(model_identity, chunk_id))").execute(pool).await?;
    Ok(())
}

/// Reuses completed work only when both model artifacts and source content match.
/// Long legacy passages are represented by a normalized, length-weighted mean
/// of all their segments, preserving existing chunk IDs and citation anchors.
pub async fn prepare(pool: &SqlitePool, model: &dyn EmbeddingPort) -> Result<usize> {
    ensure_schema(pool).await?;
    let identity = model.model_identity();
    let mut cursor = String::new();
    let mut completed = 0;
    loop {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, COALESCE(NULLIF(contextualized_content, ''), content) FROM text_chunks WHERE id > ? ORDER BY id LIMIT 64")
            .bind(&cursor).fetch_all(pool).await?;
        if rows.is_empty() {
            break;
        }
        for (id, text) in rows {
            cursor = id.clone();
            let hash = content_hash(&text);
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_generation_vectors WHERE model_identity = ? AND chunk_id = ? AND content_hash = ? AND dimension = ?")
                .bind(&identity).bind(&id).bind(&hash).bind(model.dimension() as i64).fetch_one(pool).await?;
            if exists > 0 {
                completed += 1;
                continue;
            }
            let parts = model.split_text(&text, "")?;
            if parts.is_empty() {
                continue;
            }
            let inputs: Vec<String> = parts.iter().map(|p| p.text.clone()).collect();
            let vectors = model.embed_batch(&inputs).await?;
            if vectors.len() != parts.len() {
                return Err(AppError::InvalidState(
                    "Embedding batch was incomplete".into(),
                ));
            }
            let mut combined = vec![0.0f32; model.dimension()];
            for (part, vector) in parts.iter().zip(vectors) {
                if vector.len() != combined.len() || vector.iter().any(|v| !v.is_finite()) {
                    return Err(AppError::InvalidState(
                        "Invalid embedding during migration".into(),
                    ));
                }
                let weight = part.token_count.max(1) as f32;
                for (out, value) in combined.iter_mut().zip(vector) {
                    *out += weight * value;
                }
            }
            let norm = combined.iter().map(|v| v * v).sum::<f32>().sqrt();
            if !norm.is_finite() || norm <= 0.0 {
                return Err(AppError::InvalidState(
                    "Zero or non-finite embedding during migration".into(),
                ));
            }
            for value in &mut combined {
                *value /= norm;
            }
            // The source may have been deleted or edited while inference ran.
            sqlx::query("INSERT OR REPLACE INTO embedding_generation_vectors (model_identity, chunk_id, content_hash, embedding, dimension) SELECT ?, id, ?, ?, ? FROM text_chunks WHERE id = ? AND COALESCE(NULLIF(contextualized_content, ''), content) = ?")
                .bind(&identity).bind(&hash).bind(super::encoding::encode_embedding(&combined)).bind(model.dimension() as i64).bind(&id).bind(&text).execute(pool).await?;
            completed += 1;
        }
        tracing::info!(completed, model_identity = %identity, "Preparing embedding generation");
    }
    Ok(completed)
}

/// Never restore vectors from another model, even if dimensions happen to match.
pub async fn restore(
    pool: &SqlitePool,
    identity: &str,
    dimension: usize,
) -> Result<Vec<(String, Vec<f32>, String, String, String)>> {
    ensure_schema(pool).await?;
    type RestoredRow = (String, Vec<u8>, String, String, Option<String>, String);
    let rows: Vec<RestoredRow> = sqlx::query_as(
        "SELECT tc.id, COALESCE(te.embedding, eg.embedding), tc.content, tc.document_id, CASE WHEN te.embedding IS NULL THEN eg.content_hash ELSE NULL END, COALESCE(NULLIF(tc.contextualized_content, ''), tc.content) FROM text_chunks tc LEFT JOIN text_embeddings te ON te.chunk_id = tc.id AND te.model_name = ? AND te.dimension = ? LEFT JOIN embedding_generation_vectors eg ON eg.chunk_id = tc.id AND eg.model_identity = ? AND eg.dimension = ? WHERE te.embedding IS NOT NULL OR eg.embedding IS NOT NULL")
        .bind(identity).bind(dimension as i64).bind(identity).bind(dimension as i64).fetch_all(pool).await?;
    let mut result = Vec::new();
    for (id, bytes, content, doc_id, hash, source) in rows {
        if hash.is_some_and(|h| h != content_hash(&source)) {
            continue;
        }
        let vector = super::encoding::decode_embedding(&bytes)?;
        if vector.len() != dimension || vector.iter().any(|v| !v.is_finite()) {
            return Err(AppError::InvalidState("Invalid persisted embedding".into()));
        }
        result.push((
            super::encoding::vector_key(&id),
            vector,
            content,
            id,
            doc_id,
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Model {
        identity: &'static str,
        calls: AtomicUsize,
        fail_at: usize,
    }
    #[async_trait::async_trait]
    impl EmbeddingPort for Model {
        fn model_identity(&self) -> String {
            self.identity.into()
        }
        fn dimension(&self) -> usize {
            2
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
        async fn embed_single(&self, _: &str) -> Result<Vec<f32>> {
            Ok(vec![1.0, 0.0])
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == self.fail_at {
                return Err(AppError::InvalidState("injected interruption".into()));
            }
            Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
        }
    }
    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE text_chunks (id TEXT PRIMARY KEY, content TEXT, contextualized_content TEXT, document_id TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE text_embeddings (chunk_id TEXT, model_name TEXT, dimension INTEGER, embedding BLOB)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO text_chunks VALUES ('a', 'alpha', NULL, 'doc'), ('b', 'beta', NULL, 'doc')").execute(&pool).await.unwrap();
        pool
    }
    #[tokio::test]
    async fn interrupted_generation_resumes_and_never_restores_another_vector_space() {
        let pool = database().await;
        let first = Model {
            identity: "new",
            calls: AtomicUsize::new(0),
            fail_at: 1,
        };
        assert!(prepare(&pool, &first).await.is_err());
        let resumed = Model {
            identity: "new",
            calls: AtomicUsize::new(0),
            fail_at: usize::MAX,
        };
        assert_eq!(prepare(&pool, &resumed).await.unwrap(), 2);
        assert_eq!(
            resumed.calls.load(Ordering::SeqCst),
            1,
            "completed source must not be recomputed"
        );
        assert_eq!(restore(&pool, "new", 2).await.unwrap().len(), 2);
        assert!(restore(&pool, "old-same-dimension", 2)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("UPDATE text_chunks SET content = 'changed' WHERE id = 'a'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            restore(&pool, "new", 2).await.unwrap().len(),
            1,
            "stale source must not reuse its old vector"
        );
        prepare(&pool, &resumed).await.unwrap();
        assert_eq!(resumed.calls.load(Ordering::SeqCst), 2);
        assert_eq!(restore(&pool, "new", 2).await.unwrap().len(), 2);
    }
}
