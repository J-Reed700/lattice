//! A shared index handle that binds or hot-rebinds embedding models at
//! runtime without restarting Lattice.
use super::IndexPersistence;
use crate::infrastructure::persistence::repositories::embedding::generation;
use crate::infrastructure::vector_index::{USearchVectorIndex, VectorIndexCompression};
use lattice_core::application::contracts::search::engine::SearchResult;
use lattice_core::application::contracts::search::SearchResultRecord;
use lattice_core::application::ports::vector_search_port::VectorIndexEntry;
use lattice_core::application::ports::{
    EmbeddingIndexBinding, EmbeddingPort, SearchServiceTrait, VectorSearchPort,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::background::BackgroundTasks;
use parking_lot::RwLock;
use std::{collections::HashSet, path::PathBuf, sync::Arc};

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

struct BoundIndex {
    identity: String,
    index: Arc<USearchVectorIndex>,
    persistence: Option<Arc<IndexPersistence>>,
    persistence_task: Option<tokio::task::JoinHandle<()>>,
}

pub struct RuntimeVectorIndex {
    initial: Arc<USearchVectorIndex>,
    bound: RwLock<Option<BoundIndex>>,
    rebind_lock: tokio::sync::Mutex<()>,
    path: PathBuf,
    compression: VectorIndexCompression,
    /// The database each bind prepares and restores its generation from.
    database: Database,
    /// Where the persistence worker runs, so shutdown and rebind flush and stop it.
    background: Arc<BackgroundTasks>,
}

impl RuntimeVectorIndex {
    pub fn new(
        initial: Arc<USearchVectorIndex>,
        identity: Option<String>,
        path: PathBuf,
        compression: VectorIndexCompression,
        persistence: Option<Arc<IndexPersistence>>,
        database: Database,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        let bound = identity.map(|identity| BoundIndex {
            identity,
            index: Arc::clone(&initial),
            persistence,
            persistence_task: None,
        });
        Self {
            initial,
            bound: RwLock::new(bound),
            rebind_lock: tokio::sync::Mutex::new(()),
            path,
            compression,
            database,
            background,
        }
    }

    pub fn identity(&self) -> Option<String> {
        self.bound
            .read()
            .as_ref()
            .map(|bound| bound.identity.clone())
    }

    /// Writes the index and its manifest. `None` until a model is bound.
    /// Shutdown flushes through it so coalesced saves are not lost.
    pub fn persistence(&self) -> Option<Arc<IndexPersistence>> {
        self.bound
            .read()
            .as_ref()
            .and_then(|bound| bound.persistence.clone())
    }

    fn current(&self) -> Arc<USearchVectorIndex> {
        self.bound
            .read()
            .as_ref()
            .map(|bound| Arc::clone(&bound.index))
            .unwrap_or_else(|| Arc::clone(&self.initial))
    }

    /// Called inside the embedding loader's single-flight operation, before it
    /// publishes a ready model. All search and indexing consumers share this
    /// handle; switching embedding models at runtime hot-rebinds to the new
    /// model's generation and flushes/stops the previous persistence worker.
    pub async fn bind_first(&self, model: &dyn EmbeddingPort) -> Result<()> {
        let _rebind = self.rebind_lock.lock().await;
        let identity = model.model_identity();
        let dimension = model.dimension();
        {
            let guard = self.bound.read();
            if let Some(current) = guard.as_ref() {
                if current.identity == identity && current.index.dimension() == dimension {
                    return Ok(());
                }
            }
        }

        let pool = &self.database;
        crate::infrastructure::ml::embedding_generation::prepare(pool, model).await?;
        // Opened exactly as startup opens a bound index — same file name, same
        // compression, coalesced saves and streaming page-by-page restore — so
        // the next launch finds this index trustworthy instead of rebuilding it.
        let generation = generation_name(Some(&identity), &self.compression);
        let path = self
            .path
            .with_file_name(format!("usearch-{generation}.usearch"));
        let compression = self.compression.clone();
        let open_path = path.clone();
        let index = tokio::task::spawn_blocking(move || {
            let _layout = crate::infrastructure::vector_index::ensure_index_layout_match(
                &open_path,
                dimension,
                &compression,
            )?;
            let index = USearchVectorIndex::open_or_create_with_compression(
                dimension,
                open_path,
                compression,
            )?
            .with_coalesced_saves();
            Ok::<_, AppError>(Arc::new(index))
        })
        .await
        .map_err(|e| {
            AppError::InternalError(format!("Vector index initialization failed: {e}"))
        })??;

        let restore = |cursor: String| {
            let database = pool.clone();
            let identity = identity.clone();
            async move { generation::restore_page(&database, &identity, dimension, &cursor, 128).await }
        };
        super::persistence::open_or_rebuild_streaming(
            &index,
            pool,
            &path,
            &identity,
            &generation,
            dimension,
            restore,
        )
        .await?;

        let persistence = Arc::new(IndexPersistence::new(
            Arc::clone(&index),
            pool.clone(),
            identity.clone(),
            generation,
            dimension,
            path,
        ));
        let persistence_task = self
            .background
            .spawn(Arc::clone(&persistence).run(self.background.token()));

        let previous = {
            let mut guard = self.bound.write();
            guard.replace(BoundIndex {
                identity,
                index,
                persistence: Some(persistence),
                persistence_task,
            })
        };

        if let Some(previous) = previous {
            if let Some(old_persistence) = previous.persistence {
                old_persistence.stop();
                if let Err(error) = old_persistence.flush_if_dirty().await {
                    tracing::warn!(
                        %error,
                        identity = %previous.identity,
                        "Failed to flush superseded vector index during hot-rebind"
                    );
                }
                drop(old_persistence);
            }
            if let Some(task) = previous.persistence_task {
                let _ = task.await;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl EmbeddingIndexBinding for RuntimeVectorIndex {
    async fn bind_first(&self, model: &dyn EmbeddingPort) -> Result<()> {
        RuntimeVectorIndex::bind_first(self, model).await
    }
}

impl VectorSearchPort for RuntimeVectorIndex {
    fn search(&self, query: &[f32], k: usize, threshold: f32) -> Result<Vec<SearchResultRecord>> {
        VectorSearchPort::search(self.current().as_ref(), query, k, threshold)
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
        SearchServiceTrait::search(self.current().as_ref(), query, k)
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

    struct Model(&'static str, usize);
    #[async_trait::async_trait]
    impl EmbeddingPort for Model {
        fn model_identity(&self) -> String {
            self.0.into()
        }
        fn dimension(&self) -> usize {
            self.1
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
        async fn embed_single(&self, _: &str) -> Result<Vec<f32>> {
            let mut v = vec![0.0; self.1];
            if let Some(first) = v.first_mut() {
                *first = 1.0;
            }
            Ok(v)
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let mut v = vec![0.0; self.1];
            if let Some(first) = v.first_mut() {
                *first = 1.0;
            }
            Ok(vec![v; texts.len()])
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
        sqlx::query("CREATE TABLE embedding_generation_vectors (model_identity TEXT NOT NULL, chunk_id TEXT NOT NULL, content_hash TEXT NOT NULL, embedding BLOB NOT NULL, dimension INTEGER NOT NULL, PRIMARY KEY(model_identity, chunk_id))").execute(&pool).await.unwrap();
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
            Database::new(pool.clone()),
            Arc::default(),
        ));
        // These handles exist before the user downloads their first model.
        let indexing: Arc<dyn VectorSearchPort> = runtime.clone();
        let searching: Arc<dyn SearchServiceTrait> = runtime.clone();
        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
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
        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
        assert_eq!(
            indexing.count(),
            1,
            "repeated load must not clear the index"
        );
        assert_eq!(runtime.identity().as_deref(), Some("sha256:first"));
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
    async fn switching_models_hot_rebinds_existing_consumers_and_flushes_previous_generation() {
        use lattice_core::shared::hash::sha256_hex;
        use lattice_core::shared::persistence::embedding_encoding;

        let pool = database().await;
        let dir = tempfile::tempdir().unwrap();
        let runtime = Arc::new(RuntimeVectorIndex::new(
            Arc::new(USearchVectorIndex::new(384, None).unwrap()),
            None,
            dir.path().join("usearch-unconfigured.usearch"),
            VectorIndexCompression::None,
            None,
            Database::new(pool.clone()),
            Arc::default(),
        ));
        let indexing: Arc<dyn VectorSearchPort> = runtime.clone();
        let searching: Arc<dyn SearchServiceTrait> = runtime.clone();

        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
        let first_persistence = runtime.persistence().unwrap();
        indexing
            .add_embedding_with_content(
                "v1".into(),
                vec![1., 0.],
                "Chicago".into(),
                "c1".into(),
                "doc1".into(),
            )
            .unwrap();
        assert!(runtime.current().is_dirty());

        // Seed prepared generation vectors for the second model (with a different dimension).
        sqlx::query(
            "INSERT INTO text_chunks (id, content, document_id) VALUES ('c2', 'Denver', 'doc2')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO embedding_generation_vectors (model_identity, chunk_id, content_hash, embedding, dimension) VALUES ('sha256:second', 'c2', ?, ?, 4)",
        )
        .bind(sha256_hex("Denver"))
        .bind(embedding_encoding::encode_embedding(&[0., 1., 0., 0.]))
        .execute(&pool)
        .await
        .unwrap();

        runtime
            .bind_first(&Model("sha256:second", 4))
            .await
            .unwrap();
        assert_eq!(runtime.identity().as_deref(), Some("sha256:second"));
        assert_eq!(indexing.dimension(), 4);
        assert_eq!(indexing.count(), 1);
        assert_eq!(searching.search(&[0., 1., 0., 0.], 1).unwrap().len(), 1);

        // Old generation was flushed to disk and its persistence task was stopped on rebind.
        let reopened_first =
            USearchVectorIndex::open_or_create(2, dir.path().join("usearch-sha256-first.usearch"))
                .unwrap();
        assert_eq!(reopened_first.count(), 1);
        assert!(!first_persistence.flush_if_dirty().await.unwrap());

        // Switching back reuses the persisted generation for sha256:first.
        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
        assert_eq!(runtime.identity().as_deref(), Some("sha256:first"));
        assert_eq!(indexing.dimension(), 2);
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
            Database::new(pool.clone()),
            Arc::default(),
        );
        sqlx::query("DROP TABLE text_embeddings")
            .execute(&pool)
            .await
            .unwrap();
        assert!(runtime.bind_first(&Model("sha256:first", 2)).await.is_err());
        assert_eq!(runtime.identity(), None);
        assert_eq!(runtime.dimension(), 384);
        sqlx::query("CREATE TABLE text_embeddings (chunk_id TEXT, model_name TEXT, dimension INTEGER, embedding BLOB)").execute(&pool).await.unwrap();
        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
        assert_eq!(runtime.dimension(), 2);
    }

    #[tokio::test]
    async fn failed_rebind_preserves_currently_bound_model() {
        let pool = database().await;
        let dir = tempfile::tempdir().unwrap();
        let runtime = RuntimeVectorIndex::new(
            Arc::new(USearchVectorIndex::new(384, None).unwrap()),
            None,
            dir.path().join("usearch-unconfigured.usearch"),
            VectorIndexCompression::None,
            None,
            Database::new(pool.clone()),
            Arc::default(),
        );
        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
        runtime
            .add_embedding_with_content(
                "vector".into(),
                vec![1., 0.],
                "Chicago".into(),
                "chunk".into(),
                "doc".into(),
            )
            .unwrap();

        sqlx::query("DROP TABLE text_embeddings")
            .execute(&pool)
            .await
            .unwrap();
        assert!(runtime
            .bind_first(&Model("sha256:second", 4))
            .await
            .is_err());
        assert_eq!(runtime.identity().as_deref(), Some("sha256:first"));
        assert_eq!(runtime.dimension(), 2);
        assert_eq!(runtime.count(), 1);
    }

    #[tokio::test]
    async fn first_bind_opens_the_index_as_startup_would() {
        use crate::infrastructure::vector_index::{manifest_path_for, VectorQuantization};
        let pool = database().await;
        let dir = tempfile::tempdir().unwrap();
        let compression = VectorIndexCompression::truncated(1, VectorQuantization::F32);
        let runtime = RuntimeVectorIndex::new(
            Arc::new(USearchVectorIndex::new(384, None).unwrap()),
            None,
            dir.path().join("usearch-unconfigured-mrl1f32.usearch"),
            compression.clone(),
            None,
            Database::new(pool.clone()),
            Arc::default(),
        );
        runtime.bind_first(&Model("sha256:first", 2)).await.unwrap();
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
