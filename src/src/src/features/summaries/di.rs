//! Summaries feature dependency injection.
//!
//! The tier is opt-in and off by default, so nothing here is built until
//! [`register`] is called with `enabled: true`. Until then the only cost is an
//! atomic read in `trigger::notify_document_indexed` and a `None` from
//! [`Container::summary_search`].
//!
//! ## Index files
//!
//! Summary vectors live in `summaries-<identity>.usearch` beside the chunk
//! index's `usearch-<identity>.usearch`, using the same identity key. A model
//! switch therefore lands on a fresh, empty summary index for free, and the
//! rows written under the old identity are pruned when the new one registers —
//! the summary tier turns itself off until the documents are re-summarized,
//! which is the honest state rather than a stale one.
//!
//! One `USearchVectorIndex` per path is shared process-wide, so the generator
//! writing a summary and the chat turn reading it are looking at the same
//! in-memory HNSW graph rather than two snapshots of one file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;

use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::features::summaries::repository::{SqliteSummaryRepository, SummaryRepositoryPort};
use crate::features::summaries::runtime::SummaryRuntimePort;
use crate::features::summaries::search::SummarySearch;
use crate::features::summaries::source::SqliteSummarySource;
use crate::features::summaries::trigger;
use crate::features::summaries::use_cases::GenerateDocumentSummariesUseCase;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

/// Summary index file for one embedding identity.
pub fn summary_index_path(data_dir: &Path, identity: &str) -> PathBuf {
    data_dir.join(format!("summaries-{}.usearch", identity.replace(':', "-")))
}

type IndexCache = Mutex<HashMap<PathBuf, Arc<USearchVectorIndex>>>;

fn index_cache() -> &'static IndexCache {
    static CACHE: OnceLock<IndexCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The one summary index for `path`, opened on first use.
fn shared_index(path: PathBuf, dimension: usize) -> Result<Arc<dyn VectorSearchPort>> {
    let mut cache = index_cache()
        .lock()
        .map_err(|_| AppError::InternalError("summary index cache poisoned".into()))?;
    if let Some(index) = cache.get(&path) {
        return Ok(Arc::clone(index) as Arc<dyn VectorSearchPort>);
    }
    let index = Arc::new(USearchVectorIndex::open_or_create(dimension, path.clone())?);
    cache.insert(path, Arc::clone(&index));
    Ok(index as Arc<dyn VectorSearchPort>)
}

/// Resolves the utility model and embedder through the container in Tauri
/// state, which is where their caches and load-once semantics live.
struct StateRuntime {
    app: tauri::AppHandle,
}

#[async_trait]
impl SummaryRuntimePort for StateRuntime {
    async fn utility_llm(&self) -> Option<Arc<dyn LLMPort>> {
        let container = tauri::Manager::try_state::<Container>(&self.app)?;
        container.get_or_load_utility_llm().await.ok().flatten()
    }

    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        let container = tauri::Manager::try_state::<Container>(&self.app)?;
        container.get_or_load_embedding().await.ok()
    }
}

/// Wire the document/section summary tier.
///
/// Call once, after the container has been placed in Tauri state. With
/// `enabled: false` (the default) it clears any previously installed hook and
/// returns; nothing is read, opened, or generated.
pub async fn register(app: tauri::AppHandle, enabled: bool) -> Result<()> {
    if !enabled {
        trigger::clear_post_index_hook();
        return Ok(());
    }
    let Some(container) = tauri::Manager::try_state::<Container>(&app) else {
        return Err(AppError::InvalidState(
            "Summary tier registered before the container reached Tauri state".into(),
        ));
    };
    let Some(identity) = container.search.embedding_identity().map(str::to_owned) else {
        tracing::info!("Summary tier idle: no active embedding model to key its index by");
        return Ok(());
    };
    let repository: Arc<dyn SummaryRepositoryPort> =
        Arc::new(SqliteSummaryRepository::new(container.db_pool().clone()));
    // Rows under a superseded identity point into a vector index that no longer
    // exists. Dropping them is the same "rebuild rather than mix vector spaces"
    // rule the chunk index follows.
    let pruned = repository.prune_other_identities(&identity).await?;
    if pruned > 0 {
        tracing::info!(pruned, "Dropped summaries from a previous embedding model");
    }
    let index = shared_index(
        summary_index_path(container.core.data_dir(), &identity),
        container.search.vector_search().dimension(),
    )?;
    trigger::register_post_index_hook(Arc::new(GenerateDocumentSummariesUseCase::new(
        true,
        Arc::new(SqliteSummarySource::new(container.db_pool().clone())),
        repository,
        index,
        Arc::new(StateRuntime { app: app.clone() }),
    )));
    tracing::info!(%identity, "Summary tier enabled");
    Ok(())
}

/// Summaries' registrar surface on `Container`.
impl Container {
    /// The summary retrieval tier, or `None` when it has nothing to offer.
    ///
    /// `None` is the default and the common case: no active embedding model,
    /// or no summaries stored under it. The check is one indexed `COUNT`, so a
    /// vault with the tier switched off pays nothing per turn.
    pub async fn summary_search(&self) -> Option<Arc<SummarySearch>> {
        let identity = self.search.embedding_identity()?.to_owned();
        let repository: Arc<dyn SummaryRepositoryPort> =
            Arc::new(SqliteSummaryRepository::new(self.db_pool().clone()));
        match repository.count_for_identity(&identity).await {
            Ok(0) => return None,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "Could not read summary counts; skipping summary tier");
                return None;
            }
        }
        let index = shared_index(
            summary_index_path(self.core.data_dir(), &identity),
            self.search.vector_search().dimension(),
        )
        .map_err(|error| tracing::warn!(%error, "Could not open the summary index"))
        .ok()?;
        let embedder = self
            .get_or_load_embedding()
            .await
            .map_err(|error| tracing::warn!(%error, "Summary tier has no embedder"))
            .ok()?;
        Some(Arc::new(SummarySearch::new(index, repository, embedder)))
    }
}
