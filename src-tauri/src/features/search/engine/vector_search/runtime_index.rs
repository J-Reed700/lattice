//! A shared index handle that can bind the first downloaded model without restarting.
//! Once bound, changing vector spaces still requires the normal restart migration.
use super::USearchVectorIndex;
use crate::application::contracts::search::SearchResultRecord;
use crate::application::ports::vector_search_port::VectorIndexEntry;
use crate::application::ports::{EmbeddingPort, VectorSearchPort};
use crate::features::search::{engine::service::SearchResult, SearchServiceTrait};
use crate::shared::error::{AppError, Result};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub struct RuntimeVectorIndex {
    initial: Arc<USearchVectorIndex>,
    bound: OnceLock<(String, Arc<USearchVectorIndex>)>,
    path: PathBuf,
}

impl RuntimeVectorIndex {
    pub fn new(initial: Arc<USearchVectorIndex>, identity: Option<String>, path: PathBuf) -> Self {
        let bound = OnceLock::new();
        if let Some(identity) = identity {
            let _ = bound.set((identity, initial.clone()));
        }
        Self {
            initial,
            bound,
            path,
        }
    }

    pub fn identity(&self) -> Option<&str> {
        self.bound.get().map(|(id, _)| id.as_str())
    }

    fn current(&self) -> &USearchVectorIndex {
        self.bound
            .get()
            .map(|(_, index)| index.as_ref())
            .unwrap_or(&self.initial)
    }

    /// Called inside the embedding loader's single-flight operation, before it
    /// publishes a ready model. All search and indexing consumers share this handle.
    pub async fn bind_first(
        &self,
        pool: &sqlx::SqlitePool,
        model: &dyn EmbeddingPort,
        late_chunking: bool,
    ) -> Result<()> {
        let identity = model.model_identity();
        if let Some(current) = self.identity() {
            return if current == identity && self.dimension() == model.dimension() {
                Ok(())
            } else {
                Err(AppError::ModelLoadFailed("Embedding model changed. Restart Lattice to open its prepared search generation.".into()))
            };
        }
        if !late_chunking {
            crate::features::embedding::generation::prepare(pool, model).await?;
        }
        let rows =
            crate::features::embedding::generation::restore(pool, &identity, model.dimension())
                .await?;
        let path = self
            .path
            .with_file_name(format!("usearch-{}.usearch", identity.replace(':', "-")));
        let dimension = model.dimension();
        let index = tokio::task::spawn_blocking(move || {
            let index = USearchVectorIndex::open_or_create(dimension, path)?;
            let expected = rows.len();
            if index.rebuild_from_embeddings(rows)? != expected {
                return Err(AppError::InvalidState(
                    "Incomplete vector index rebuild".into(),
                ));
            }
            Ok::<_, AppError>(Arc::new(index))
        })
        .await
        .map_err(|e| {
            AppError::InternalError(format!("Vector index initialization failed: {e}"))
        })??;
        self.bound.set((identity, index)).map_err(|_| {
            AppError::InvalidState("Embedding index was already initialized".into())
        })?;
        Ok(())
    }
}

impl VectorSearchPort for RuntimeVectorIndex {
    fn search(&self, query: &[f32], k: usize, threshold: f32) -> Result<Vec<SearchResultRecord>> {
        VectorSearchPort::search(self.current(), query, k, threshold)
    }
    fn search_scoped(
        &self,
        query: &[f32],
        k: usize,
        threshold: f32,
        allowed: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultRecord>> {
        self.current().search_scoped(query, k, threshold, allowed)
    }
    fn add_embedding(&self, id: String, embedding: Vec<f32>) -> Result<()> {
        self.current().add_embedding(id, embedding)
    }
    fn add_embedding_with_content(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: String,
        chunk_id: String,
        document_id: String,
    ) -> Result<()> {
        self.current()
            .add_embedding_with_content(id, embedding, content, chunk_id, document_id)
    }
    fn publish_embeddings(&self, entries: Vec<VectorIndexEntry>) -> Result<()> {
        self.current().publish_embeddings(entries)
    }
    fn remove_embedding(&self, id: &str) -> Result<()> {
        self.current().remove_embedding(id)
    }
    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        self.current().remove_embeddings(ids)
    }
    fn clear(&self) -> Result<()> {
        self.current().clear()
    }
    fn count(&self) -> usize {
        self.current().count()
    }
    fn dimension(&self) -> usize {
        self.current().dimension()
    }
}

#[async_trait::async_trait]
impl SearchServiceTrait for RuntimeVectorIndex {
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        SearchServiceTrait::search(self.current(), query, k)
    }
    async fn search_with_metadata(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        self.current().search_with_metadata(query, k).await
    }
    fn search_with_threshold(
        &self,
        query: &[f32],
        k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>> {
        self.current().search_with_threshold(query, k, threshold)
    }
    fn batch_search(&self, queries: &[Vec<f32>], k: usize) -> Result<Vec<Vec<SearchResult>>> {
        self.current().batch_search(queries, k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Model(&'static str);
    #[async_trait::async_trait]
    impl EmbeddingPort for Model {
        fn model_identity(&self) -> String {
            self.0.into()
        }
        fn dimension(&self) -> usize {
            2
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
        async fn embed_single(&self, _: &str) -> Result<Vec<f32>> {
            Ok(vec![1., 0.])
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![vec![1., 0.]; texts.len()])
        }
    }

    async fn database() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE text_chunks (id TEXT PRIMARY KEY, content TEXT, contextualized_content TEXT, document_id TEXT)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE text_embeddings (chunk_id TEXT, model_name TEXT, dimension INTEGER, embedding BLOB)").execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn first_download_binds_existing_consumers_and_survives_restart() {
        let pool = database().await;
        let dir = tempfile::tempdir().unwrap();
        let initial = Arc::new(USearchVectorIndex::new(384, None).unwrap());
        let runtime = Arc::new(RuntimeVectorIndex::new(
            initial,
            None,
            dir.path().join("usearch-unconfigured.usearch"),
        ));
        // These handles exist before the user downloads their first model.
        let indexing: Arc<dyn VectorSearchPort> = runtime.clone();
        let searching: Arc<dyn SearchServiceTrait> = runtime.clone();
        runtime
            .bind_first(&pool, &Model("sha256:first"), false)
            .await
            .unwrap();
        assert_eq!(indexing.dimension(), 2);
        indexing
            .add_embedding_with_content(
                "vector".into(),
                vec![1., 0.],
                "Chicago".into(),
                "chunk".into(),
                "doc".into(),
            )
            .unwrap();
        assert_eq!(searching.search(&[1., 0.], 1).unwrap().len(), 1);
        assert_eq!(indexing.search(&[1., 0.], 1, 0.).unwrap().len(), 1);
        runtime
            .bind_first(&pool, &Model("sha256:first"), false)
            .await
            .unwrap();
        assert_eq!(
            indexing.count(),
            1,
            "repeated load must not clear the index"
        );
        assert!(runtime
            .bind_first(&pool, &Model("sha256:other"), false)
            .await
            .is_err());
        assert_eq!(runtime.identity(), Some("sha256:first"));
        let reopened =
            USearchVectorIndex::open_or_create(2, dir.path().join("usearch-sha256-first.usearch"))
                .unwrap();
        assert_eq!(reopened.count(), 1);
    }

    #[tokio::test]
    async fn failed_preparation_leaves_unconfigured_index_retryable() {
        let pool = database().await;
        let dir = tempfile::tempdir().unwrap();
        let runtime = RuntimeVectorIndex::new(
            Arc::new(USearchVectorIndex::new(384, None).unwrap()),
            None,
            dir.path().join("index"),
        );
        sqlx::query("DROP TABLE text_embeddings")
            .execute(&pool)
            .await
            .unwrap();
        assert!(runtime
            .bind_first(&pool, &Model("sha256:first"), false)
            .await
            .is_err());
        assert_eq!(runtime.identity(), None);
        assert_eq!(runtime.dimension(), 384);
        sqlx::query("CREATE TABLE text_embeddings (chunk_id TEXT, model_name TEXT, dimension INTEGER, embedding BLOB)").execute(&pool).await.unwrap();
        runtime
            .bind_first(&pool, &Model("sha256:first"), false)
            .await
            .unwrap();
        assert_eq!(runtime.dimension(), 2);
    }
}
