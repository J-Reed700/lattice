//! Resumable preparation of a new vector space. Existing vectors remain untouched.
use crate::infrastructure::persistence::repositories::embedding::generation::{
    chunk_page_after, prepared_hashes, store_prepared_batch, PreparedChunk,
};
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::persistence::Database;

const EMBED_BATCH_SIZE: usize = 32;

struct PendingSingleChunk {
    id: String,
    text: String,
    hash: String,
    part_text: String,
}

fn normalize_embedding(vector: &mut [f32], dimension: usize) -> Result<()> {
    if vector.len() != dimension || vector.iter().any(|v| !v.is_finite()) {
        return Err(AppError::InvalidState(
            "Invalid embedding during migration".into(),
        ));
    }
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(AppError::InvalidState(
            "Zero or non-finite embedding during migration".into(),
        ));
    }
    for value in vector {
        *value /= norm;
    }
    Ok(())
}

async fn flush_single_batch(
    database: &Database,
    model: &dyn EmbeddingPort,
    identity: &str,
    batch: &mut Vec<PendingSingleChunk>,
) -> Result<usize> {
    if batch.is_empty() {
        return Ok(0);
    }
    let dimension = model.dimension();
    let inputs: Vec<String> = batch.iter().map(|item| item.part_text.clone()).collect();
    let mut vectors = model.embed_batch(&inputs).await?;
    if vectors.len() != batch.len() {
        return Err(AppError::InvalidState(
            "Embedding batch was incomplete".into(),
        ));
    }
    for vector in &mut vectors {
        normalize_embedding(vector, dimension)?;
    }
    let records: Vec<PreparedChunk<'_>> = batch
        .iter()
        .zip(vectors.iter())
        .map(|(item, vector)| PreparedChunk {
            chunk_id: &item.id,
            text: &item.text,
            content_hash: &item.hash,
            vector,
        })
        .collect();
    store_prepared_batch(database, identity, &records).await?;
    let count = batch.len();
    batch.clear();
    Ok(count)
}

/// Reuses completed work only when both model artifacts and source content match.
/// A passage the new model's window cannot take in one pass is represented by
/// a normalized, length-weighted mean of its segments, preserving existing
/// chunk IDs and citation anchors.
///
/// The text embedded is `contextualized_content` — the context prefix plus the
/// chunk, exactly what the indexer embedded — so the prefix never has to be
/// rebuilt from the document row here.
pub async fn prepare(database: &Database, model: &dyn EmbeddingPort) -> Result<usize> {
    let identity = model.model_identity();
    let dimension = model.dimension();
    let mut cursor = String::new();
    let mut completed = 0;
    loop {
        let rows = chunk_page_after(database, &cursor).await?;
        if rows.is_empty() {
            break;
        }
        let chunk_ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
        let existing = prepared_hashes(database, &identity, &chunk_ids, dimension).await?;
        let mut single_batch = Vec::with_capacity(EMBED_BATCH_SIZE);

        for (id, text) in rows {
            cursor = id.clone();
            let hash = sha256_hex(&text);
            if existing.get(&id).is_some_and(|saved| *saved == hash) {
                completed += 1;
                continue;
            }
            // Not `split_text`: that sizes to the chunk target, and a stored
            // chunk already fills the target *plus* its context prefix, so it
            // would cut nearly every full chunk in two and store an average.
            let parts = model.window_parts(&text).await?;
            if parts.is_empty() {
                continue;
            }
            if let [single] = parts.as_slice() {
                single_batch.push(PendingSingleChunk {
                    id,
                    text,
                    hash,
                    part_text: single.text.clone(),
                });
                if single_batch.len() >= EMBED_BATCH_SIZE {
                    completed +=
                        flush_single_batch(database, model, &identity, &mut single_batch).await?;
                }
                continue;
            }

            // Persist queued single-window chunks before handling a multi-part split.
            completed += flush_single_batch(database, model, &identity, &mut single_batch).await?;

            let inputs: Vec<String> = parts.iter().map(|p| p.text.clone()).collect();
            let vectors = model.embed_batch(&inputs).await?;
            if vectors.len() != parts.len() {
                return Err(AppError::InvalidState(
                    "Embedding batch was incomplete".into(),
                ));
            }
            let mut combined = vec![0.0f32; dimension];
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
            normalize_embedding(&mut combined, dimension)?;
            // The source may have been deleted or edited while inference ran.
            store_prepared_batch(
                database,
                &identity,
                &[PreparedChunk {
                    chunk_id: &id,
                    text: &text,
                    content_hash: &hash,
                    vector: &combined,
                }],
            )
            .await?;
            completed += 1;
        }
        completed += flush_single_batch(database, model, &identity, &mut single_batch).await?;
        tracing::info!(completed, model_identity = %identity, "Preparing embedding generation");
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::repositories::embedding::generation::restore;
    use lattice_core::application::ports::embedding_port::EmbeddingTextChunk;
    use sqlx::SqlitePool;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Model {
        identity: &'static str,
        calls: AtomicUsize,
        fail_at: usize,
        split_batches: AtomicUsize,
    }
    fn part(text: &str, start: usize) -> EmbeddingTextChunk {
        EmbeddingTextChunk {
            text: text.to_owned(),
            start,
            end: start + text.len(),
            token_count: text.len(),
        }
    }
    impl Model {
        fn new(identity: &'static str, fail_at: usize) -> Self {
            Self {
                identity,
                calls: AtomicUsize::new(0),
                fail_at,
                split_batches: AtomicUsize::new(0),
            }
        }
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
        /// A chunk-target split that always halves, as the real one does for a
        /// stored chunk that carries its context prefix.
        async fn split_text(&self, text: &str, _: &str) -> Result<Vec<EmbeddingTextChunk>> {
            self.split_batches.fetch_add(1, Ordering::SeqCst);
            let mid = text.len() / 2;
            Ok(vec![part(&text[..mid], 0), part(&text[mid..], mid)])
        }
        /// A window of eight bytes.
        async fn window_parts(&self, text: &str) -> Result<Vec<EmbeddingTextChunk>> {
            if text.len() > 8 {
                self.split_text(text, "").await
            } else {
                Ok(vec![part(text, 0)])
            }
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
        sqlx::query("CREATE TABLE embedding_generation_vectors (model_identity TEXT NOT NULL, chunk_id TEXT NOT NULL REFERENCES text_chunks(id) ON DELETE CASCADE, content_hash TEXT NOT NULL, embedding BLOB NOT NULL, dimension INTEGER NOT NULL, PRIMARY KEY(model_identity, chunk_id))").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO text_chunks VALUES ('a', 'alpha', NULL, 'doc'), ('b', 'beta', NULL, 'doc')").execute(&pool).await.unwrap();
        pool
    }
    #[tokio::test]
    async fn interrupted_generation_resumes_and_never_restores_another_vector_space() {
        let pool = database().await;
        sqlx::query(
            "INSERT INTO text_chunks VALUES ('c', 'a much longer stored chunk', NULL, 'doc')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let db = Database::new(pool.clone());
        let first = Model::new("new", 1);
        assert!(prepare(&db, &first).await.is_err());
        let resumed = Model::new("new", usize::MAX);
        assert_eq!(prepare(&db, &resumed).await.unwrap(), 3);
        assert_eq!(
            resumed.calls.load(Ordering::SeqCst),
            1,
            "completed source must not be recomputed"
        );
        assert_eq!(restore(&db, "new", 2).await.unwrap().len(), 3);
        assert!(restore(&db, "old-same-dimension", 2)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("UPDATE text_chunks SET content = 'changed' WHERE id = 'a'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            restore(&db, "new", 2).await.unwrap().len(),
            2,
            "stale source must not reuse its old vector"
        );
        prepare(&db, &resumed).await.unwrap();
        assert_eq!(resumed.calls.load(Ordering::SeqCst), 2);
        assert_eq!(restore(&db, "new", 2).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_stored_chunk_that_fits_the_window_is_embedded_whole() {
        let pool = database().await;
        let db = Database::new(pool.clone());
        sqlx::query(
            "INSERT INTO text_chunks VALUES ('c', 'a much longer stored chunk', NULL, 'doc')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let model = Model::new("new", usize::MAX);
        assert_eq!(prepare(&db, &model).await.unwrap(), 3);
        // 'alpha' and 'beta' fit the window and are batched together; only the
        // chunk past the window is split.
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
        assert_eq!(model.split_batches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn batches_up_to_32_unsplit_chunks_per_embed_and_write_transaction() {
        let pool = database().await;
        let db = Database::new(pool.clone());
        for idx in 0..33 {
            sqlx::query("INSERT INTO text_chunks VALUES (?, ?, NULL, 'doc')")
                .bind(format!("k{idx:02}"))
                .bind(format!("c{idx:02}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        let model = Model::new("new", usize::MAX);
        assert_eq!(prepare(&db, &model).await.unwrap(), 35);
        // 35 single-window chunks -> 2 batches (32 + 3), 0 splits.
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
        assert_eq!(model.split_batches.load(Ordering::SeqCst), 0);
        assert_eq!(restore(&db, "new", 2).await.unwrap().len(), 35);
    }
}
