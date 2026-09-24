//! A shared index handle that can bind the first downloaded model without restarting.
//! Once bound, changing vector spaces still requires the normal restart migration.
use super::{IndexPersistence, USearchVectorIndex, VectorIndexCompression};
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

/// The name every file of one vector space shares: the embedding identity,
/// plus the compression layout when there is one, because a compression switch
/// reshapes stored vectors as completely as a model change does. `None`
/// contributes no suffix, so an uncompressed vault keeps its filenames.
pub fn generation_name(identity: Option<&str>, compression: &VectorIndexCompression) -> String {
    let generation = match identity {
        Some(id) => id.replace(':', "-"),
        None => "unconfigured".to_string(),
    };
    match compression.layout_token() {
        Some(layout) => format!("{generation}-{layout}"),
        None => generation,
    }
}

pub struct RuntimeVectorIndex {
    initial: Arc<USearchVectorIndex>,
    bound: OnceLock<(String, Arc<USearchVectorIndex>)>,
    path: PathBuf,
    compression: VectorIndexCompression,
    /// Writes the bound index and its manifest. Set at startup when a model is
    /// already active, or by [`Self::bind_first`] for the first one.
    persistence: OnceLock<Arc<IndexPersistence>>,
}

impl RuntimeVectorIndex {
    pub fn new(
        initial: Arc<USearchVectorIndex>,
        identity: Option<String>,
        path: PathBuf,
        compression: VectorIndexCompression,
        persistence: Option<Arc<IndexPersistence>>,
    ) -> Self {
        let bound = OnceLock::new();
        if let Some(identity) = identity {
            let _ = bound.set((identity, initial.clone()));
        }
        let persistence_slot = OnceLock::new();
        if let Some(persistence) = persistence {
            let _ = persistence_slot.set(persistence);
        }
        Self {
            initial,
            bound,
            path,
            compression,
            persistence: persistence_slot,
        }
    }

    pub fn identity(&self) -> Option<&str> {
        self.bound.get().map(|(id, _)| id.as_str())
    }

    /// Writes the index and its manifest. `None` until a model is bound.
    /// Shutdown flushes through it so coalesced saves are not lost.
    pub fn persistence(&self) -> Option<&Arc<IndexPersistence>> {
        self.persistence.get()
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
        // Opened exactly as startup opens a bound index — same file name, same
        // compression, coalesced saves and a manifest writer — so the next
        // launch finds this index trustworthy instead of rebuilding it.
        let generation = generation_name(Some(&identity), &self.compression);
        let path = self
            .path
            .with_file_name(format!("usearch-{generation}.usearch"));
        let dimension = model.dimension();
        let compression = self.compression.clone();
        let index_path = path.clone();
        let index = tokio::task::spawn_blocking(move || {
            let _layout = super::ensure_index_layout_match(&path, dimension, &compression)?;
            let index =
                USearchVectorIndex::open_or_create_with_compression(dimension, path, compression)?
                    .with_coalesced_saves();
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
        let persistence = Arc::new(IndexPersistence::new(
            Arc::clone(&index),
            pool.clone(),
            identity.clone(),
            generation,
            dimension,
            index_path,
        ));
        self.bound.set((identity, index)).map_err(|_| {
            AppError::InvalidState("Embedding index was already initialized".into())
        })?;
        if self.persistence.set(Arc::clone(&persistence)).is_ok() {
            tokio::spawn(persistence.run());
        }
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
        sqlx::query("CREATE TABLE vector_index_state (id INTEGER PRIMARY KEY, write_counter INTEGER NOT NULL DEFAULT 0)").execute(&pool).await.unwrap();
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
            VectorIndexCompression::None,
            None,
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
        // Saves are coalesced, as at startup; the manifest writer persists them.
        assert!(runtime
            .persistence()
            .unwrap()
            .flush_if_dirty()
            .await
            .unwrap());
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
            VectorIndexCompression::None,
            None,
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

    #[tokio::test]
    async fn first_bind_opens_the_index_as_startup_would() {
        use super::super::{manifest::manifest_path_for, VectorQuantization};
        let pool = database().await;
        let dir = tempfile::tempdir().unwrap();
        let compression = VectorIndexCompression::truncated(1, VectorQuantization::F32);
        let runtime = RuntimeVectorIndex::new(
            Arc::new(USearchVectorIndex::new(384, None).unwrap()),
            None,
            dir.path().join("usearch-unconfigured-mrl1f32.usearch"),
            compression.clone(),
            None,
        );
        runtime
            .bind_first(&pool, &Model("sha256:first"), false)
            .await
            .unwrap();
        assert_eq!(runtime.current().compression(), &compression);
        runtime
            .add_embedding_with_content(
                "vector".into(),
                vec![1., 0.],
                "Chicago".into(),
                "chunk".into(),
                "doc".into(),
            )
            .unwrap();
        assert!(
            runtime.current().is_dirty(),
            "saves must be coalesced, not immediate"
        );
        let path = dir.path().join(format!(
            "usearch-{}.usearch",
            generation_name(Some("sha256:first"), &compression)
        ));
        assert!(runtime
            .persistence()
            .unwrap()
            .flush_if_dirty()
            .await
            .unwrap());
        assert!(path.exists());
        assert!(manifest_path_for(&path).exists());
    }
}
