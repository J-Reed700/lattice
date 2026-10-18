//! The search module: embeddings, the vector index and the search services.

use super::CoreModule;
use sqlx::SqlitePool;
use std::sync::Arc;

// Application Use Cases - Search
use crate::features::search::use_cases::{HybridSearchUseCase, SemanticSearchUseCase};

// Application Ports
use lattice_core::application::ports::{DocumentRepository, VectorSearchPort};

// Service Traits
use crate::features::search::trait_def::SearchEnrichmentServiceTrait;
use lattice_core::application::ports::{BM25SearchTrait, SearchServiceTrait};

// The active embedding model, which names the index's vector space
use crate::features::embedding::candle_service::{prefixes_for_model_dir, CandleEmbeddingService};
use crate::features::embedding::late_chunking::{vector_identity, EmbeddingStrategy};
use crate::features::embedding::service::DynamicEmbedding;
use crate::features::search::di::{BackfillModelLoader, VectorSpace};
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::models::checkpoint::has_loadable_weights;
use lattice_core::domain::models::downloaded::DownloadedModel;

/// Search module for semantic, hybrid, and file search
///
/// **Fields**: ~15
/// - 4 search use cases
/// - 3 search services
/// - Vector and text search ports
/// - Embedding cache (read-only)
/// - Document and recent docs repositories
#[derive(Clone)]
pub struct SearchModule {
    search: crate::features::search::di::SearchDi,
    /// The strategy the index was opened for. The embedding loader reads it so
    /// the model it publishes produces vectors in the same space the index was
    /// keyed on — anything else silently mixes two generations.
    embedding_strategy: crate::features::embedding::late_chunking::EmbeddingStrategy,
}

impl SearchModule {
    /// Build SearchModule by composing the search feature builder.
    ///
    /// Resolves the active embedding model's expected dimension (from its
    /// `config.json::hidden_size`) before opening the USearch index, so the
    /// dimension sidecar can wipe a stale index *once*, ahead of the
    /// container's model load — avoiding a startup loop where each restart
    /// wipes-then-fails.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        model_provider: Arc<dyn lattice_core::application::ports::LoadedEmbeddingModelPort>,
    ) -> lattice_core::shared::error::Result<Self> {
        let usearch_index_path = core.data_dir().join("usearch_index.usearch");
        let models_path = core.data_dir().join("models");
        let active_dim = resolve_active_embedding_dimension(&db_pool).await;
        let search_settings = read_search_settings(core.data_dir()).await;
        let embedding_strategy = resolve_embedding_strategy(&search_settings);
        let compression = resolve_vector_compression(&db_pool, &search_settings, active_dim).await;
        let active = DownloadedModelRepository::new(db_pool.clone())
            .get_active_embedding_model()
            .await?;
        let search = crate::features::search::di::build_with_compression(
            db_pool,
            usearch_index_path,
            models_path,
            Arc::new(DynamicEmbedding::new(model_provider)),
            active_dim,
            compression,
            vector_space(active.as_ref(), embedding_strategy),
            Arc::clone(core.background()),
        )
        .await?;
        Ok(Self {
            search,
            embedding_strategy,
        })
    }

    /// The embedding strategy this index's vector space was keyed on.
    pub fn embedding_strategy(
        &self,
    ) -> crate::features::embedding::late_chunking::EmbeddingStrategy {
        self.embedding_strategy
    }

    pub(crate) fn runtime_index(
        &self,
    ) -> Arc<crate::features::search::engine::vector_search::runtime_index::RuntimeVectorIndex>
    {
        self.search.runtime_index.clone()
    }

    pub fn embedding_identity(&self) -> Option<&str> {
        self.search.runtime_index.identity()
    }

    pub fn semantic_search_use_case(&self) -> &Arc<SemanticSearchUseCase> {
        &self.search.semantic_search_use_case
    }

    pub fn hybrid_search_use_case(&self) -> &Arc<HybridSearchUseCase> {
        &self.search.hybrid_search_use_case
    }

    pub fn search_service(&self) -> &Arc<dyn SearchServiceTrait> {
        &self.search.search_service
    }

    pub fn bm25_search_service(&self) -> &Arc<dyn BM25SearchTrait> {
        &self.search.bm25_search
    }

    pub fn search_enrichment_service(&self) -> &Arc<dyn SearchEnrichmentServiceTrait> {
        &self.search.search_enrichment_service
    }

    pub fn vector_search(&self) -> &Arc<dyn VectorSearchPort> {
        &self.search.vector_search
    }

    /// Writes the vector index and its manifest. `None` before a model is
    /// active. Shutdown uses it so the coalesced saves are not lost.
    pub fn index_persistence(
        &self,
    ) -> Option<&Arc<crate::features::search::engine::vector_search::IndexPersistence>> {
        self.search.runtime_index.persistence()
    }

    pub fn document_repo(&self) -> &Arc<dyn DocumentRepository> {
        &self.search.document_repo
    }
}

/// Read the stored search settings, or fall back to the defaults.
///
/// Opening the index is a startup step that must not be blocked by a settings
/// file this process cannot read: the defaults reproduce the historical
/// behaviour exactly, so falling back costs nothing but a log line.
async fn read_search_settings(
    data_dir: &std::path::Path,
) -> crate::features::settings::dto::SearchSettingsDto {
    use crate::features::settings::repository::SettingsRepository;
    use lattice_core::application::ports::SettingsRepositoryPort;

    match SettingsRepository::new(data_dir.to_path_buf()).await {
        Ok(repository) => match repository.get_all().await {
            Ok(settings) => settings.search,
            Err(e) => {
                tracing::warn!(error = %e, "Could not read search settings; opening the vector index with defaults");
                Default::default()
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "Could not open the settings repository; opening the vector index with defaults");
            Default::default()
        }
    }
}

fn resolve_embedding_strategy(
    settings: &crate::features::settings::dto::SearchSettingsDto,
) -> crate::features::embedding::late_chunking::EmbeddingStrategy {
    use crate::features::embedding::late_chunking::EmbeddingStrategy;
    use crate::features::settings::dto::EmbeddingStrategySettingDto;

    match settings.embedding_strategy {
        EmbeddingStrategySettingDto::ChunkFirst => EmbeddingStrategy::ChunkFirst,
        EmbeddingStrategySettingDto::LateChunking => EmbeddingStrategy::LateChunking,
    }
}

/// Map the stored compression setting onto the index configuration, refusing
/// truncation the active model cannot survive.
///
/// Matryoshka truncation is only meaningful for a model trained for it. Asking
/// for it on any other model does not fail loudly — it quietly produces a worse
/// index — so the request is logged and dropped rather than honoured. Same for
/// a `dims` the model cannot satisfy: the setting is a preference, and a
/// preference must never leave the app unable to open its index.
async fn resolve_vector_compression(
    db_pool: &SqlitePool,
    settings: &crate::features::settings::dto::SearchSettingsDto,
    active_embedding_dimension: Option<usize>,
) -> crate::infrastructure::vector_index::VectorIndexCompression {
    use crate::features::settings::dto::{VectorIndexCompressionModeDto, VectorQuantizationDto};
    use crate::infrastructure::vector_index::{VectorIndexCompression, VectorQuantization};

    let requested = settings.vector_index_compression;
    if requested.mode == VectorIndexCompressionModeDto::None {
        return VectorIndexCompression::None;
    }

    let Some(model_id) = active_embedding_model_id(db_pool).await else {
        tracing::info!(
            "Vector compression is set to truncated but no embedding model is active; \
             storing full-precision vectors until one is"
        );
        return VectorIndexCompression::None;
    };
    if !lattice_core::domain::models::curated::embedding_model_supports_matryoshka(&model_id) {
        tracing::warn!(
            model_id = %model_id,
            "Ignoring truncated vector compression: this embedding model does not advertise \
             Matryoshka support, and truncating it would degrade search without any error"
        );
        return VectorIndexCompression::None;
    }

    let quantization = match requested.quantization {
        VectorQuantizationDto::F32 => VectorQuantization::F32,
        VectorQuantizationDto::I8 => VectorQuantization::I8,
    };
    let compression = VectorIndexCompression::truncated(requested.dims as usize, quantization);
    let Some(dimension) = active_embedding_dimension else {
        // Without a known dimension there is nothing to validate against, and
        // guessing one would either wipe a good index or accept a bad config.
        tracing::info!(
            "Vector compression is set to truncated but the active model's dimension is unknown; \
             storing full-precision vectors until it is"
        );
        return VectorIndexCompression::None;
    };
    if let Err(e) = compression.validate(dimension) {
        tracing::warn!(error = %e, "Ignoring truncated vector compression: invalid for the active model");
        return VectorIndexCompression::None;
    }
    compression
}

/// The catalog id of the active embedding model, when there is one.
async fn active_embedding_model_id(db_pool: &SqlitePool) -> Option<String> {
    let repo = DownloadedModelRepository::new(db_pool.clone());
    let active = repo.get_active_embedding_model().await.ok().flatten()?;
    Some(active.model_id().to_owned())
}

/// Read the active embedding model's expected output dimension from disk.
///
/// Path: DB → active downloaded embedding model → file_path's parent dir
/// → `config.json::hidden_size`. Returns `None` if any step is missing
/// (no active model, file moved, malformed config) — the caller treats
/// `None` as "leave the existing index dimension alone".
///
/// Why this matters: the USearch index sidecar (`.dim`) is checked at
/// SearchDi::build() time. If we passed the legacy `DEFAULT_EMBEDDING_DIM`
/// here while the active model was actually 1024-dim, every restart would
/// wipe the user's index back to 384, then the container would fail to
/// load the model at the wrong dim, cycle forever.
async fn resolve_active_embedding_dimension(db_pool: &SqlitePool) -> Option<usize> {
    let repo = DownloadedModelRepository::new(db_pool.clone());
    let active = repo.get_active_embedding_model().await.ok().flatten()?;
    let model_dir = active.location().enclosing_dir()?;
    let config_path = model_dir.join("config.json");
    let bytes = std::fs::read(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json.get("hidden_size")?.as_u64().map(|n| n as usize)
}

/// The vector space the active embedding model writes under `strategy`, and
/// how to open that model if the index needs its chunk-first backfill.
fn vector_space(active: Option<&DownloadedModel>, strategy: EmbeddingStrategy) -> VectorSpace {
    let backfill_model = active
        .and_then(|m| {
            Some((
                m.location().enclosing_dir()?,
                m.embedding_artifact_identity()?.clone(),
            ))
        })
        .map(|(dir, artifact)| -> BackfillModelLoader {
            Box::new(move || {
                CandleEmbeddingService::open(&dir, artifact)
                    .map(|s| Box::new(s.with_strategy(strategy)) as Box<dyn EmbeddingPort>)
            })
        });
    VectorSpace {
        identity: index_identity(active, strategy),
        late_chunking: strategy.is_late_chunking(),
        backfill_model,
    }
}

/// The vector-space identity of the active embedding model, read from its
/// row: the recorded artifact identity plus the strategy. `None` means there
/// is no usable local model — nothing active, a remote model, missing files,
/// or a row without an identity. Hashes nothing.
fn index_identity(active: Option<&DownloadedModel>, strategy: EmbeddingStrategy) -> Option<String> {
    let model = active?;
    let dir = model.location().enclosing_dir()?;
    let Some(artifact) = model.embedding_artifact_identity() else {
        // The repository refuses to activate a local model without an
        // identity. Startup does not fail over it; the embedding loader
        // refuses the model and asks the user to activate it again.
        tracing::error!(
            model_id = %model.model_id(),
            "Active embedding model has no recorded artifact identity; treating it as unconfigured"
        );
        return None;
    };
    // Weights may be safetensors or, for a checkpoint that never
    // published a conversion, the `pytorch_model.bin` pickle.
    let files_present = ["config.json", "tokenizer.json"]
        .iter()
        .all(|name| dir.join(name).is_file())
        && has_loadable_weights(&dir);
    // The document prefix is part of the key the loaded model reports, so it
    // is resolved here exactly as the loader resolves it.
    files_present
        .then(|| vector_identity(artifact.as_str(), strategy, prefixes_for_model_dir(&dir)))
}

#[cfg(test)]
mod index_identity_tests {
    use super::index_identity;
    use crate::features::embedding::late_chunking::{strategy_identity, EmbeddingStrategy};
    use lattice_core::domain::models::checkpoint::WEIGHTS_SAFETENSORS;
    use lattice_core::domain::models::downloaded::{
        DownloadedModel, ModelLocation, StoredDownloadedModel,
    };
    use lattice_core::domain::models::metadata::ModelType;
    use lattice_core::domain::value_objects::ArtifactIdentity;
    use std::path::Path;

    fn recorded() -> ArtifactIdentity {
        ArtifactIdentity::from_digest(&[3; 32])
    }

    fn model(location: ModelLocation, identity: Option<ArtifactIdentity>) -> DownloadedModel {
        DownloadedModel::from_db(StoredDownloadedModel {
            id: "row-1".into(),
            model_name: "embedder".into(),
            model_id: "org/embedder".into(),
            location,
            file_size_bytes: 0,
            model_type: ModelType::TextEmbeddings,
            architecture: "bert".into(),
            downloaded_at: chrono::Utc::now(),
            last_used_at: None,
            use_count: 0,
            is_active_for_chat: false,
            is_active_for_embedding: true,
            metadata: None,
            is_active_for_utility: false,
            embedding_artifact_identity: identity,
        })
    }

    fn local(dir: &Path) -> ModelLocation {
        ModelLocation::LocalDirectory {
            path: dir.to_path_buf(),
        }
    }

    fn model_dir(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in files {
            std::fs::write(dir.path().join(name), b"{}").unwrap();
        }
        dir
    }

    const COMPLETE: [&str; 3] = ["config.json", "tokenizer.json", WEIGHTS_SAFETENSORS];

    #[test]
    fn no_active_model_is_unconfigured() {
        assert_eq!(index_identity(None, EmbeddingStrategy::ChunkFirst), None);
    }

    #[test]
    fn local_model_is_named_by_its_recorded_identity_and_strategy() {
        let dir = model_dir(&COMPLETE);
        let active = model(local(dir.path()), Some(recorded()));
        for strategy in [
            EmbeddingStrategy::ChunkFirst,
            EmbeddingStrategy::LateChunking,
        ] {
            assert_eq!(
                index_identity(Some(&active), strategy),
                Some(strategy_identity(recorded().as_str(), strategy))
            );
        }
    }

    #[test]
    fn a_prefixed_model_is_named_as_the_loaded_service_names_itself() {
        use crate::features::embedding::candle_service::CandleEmbeddingService;
        use lattice_core::application::ports::EmbeddingPort;
        let root = tempfile::tempdir().unwrap();
        // Tiny real checkpoint; the directory name selects E5's `passage: `.
        let dir = root.path().join("multilingual-e5-small");
        std::fs::create_dir(&dir).unwrap();
        crate::features::embedding::candle_service::tests::write_tiny_bert(&dir);
        let active = model(local(&dir), Some(recorded()));
        for strategy in [
            EmbeddingStrategy::ChunkFirst,
            EmbeddingStrategy::LateChunking,
        ] {
            let loaded = CandleEmbeddingService::open(&dir, recorded())
                .unwrap()
                .with_strategy(strategy);
            let expected = EmbeddingPort::model_identity(&loaded);
            assert!(expected.ends_with("+prefix-e5"), "{expected}");
            assert_eq!(index_identity(Some(&active), strategy), Some(expected));
        }
    }

    #[test]
    fn local_model_without_a_recorded_identity_is_unconfigured() {
        let dir = model_dir(&COMPLETE);
        let active = model(local(dir.path()), None);
        assert_eq!(
            index_identity(Some(&active), EmbeddingStrategy::ChunkFirst),
            None
        );
    }

    #[test]
    fn remote_model_is_unconfigured() {
        let active = model(ModelLocation::RemoteOllama, None);
        assert_eq!(
            index_identity(Some(&active), EmbeddingStrategy::ChunkFirst),
            None
        );
    }

    #[test]
    fn local_model_with_missing_files_is_unconfigured() {
        for files in [
            &["config.json", WEIGHTS_SAFETENSORS][..],
            &["config.json", "tokenizer.json"][..],
        ] {
            let dir = model_dir(files);
            let active = model(local(dir.path()), Some(recorded()));
            assert_eq!(
                index_identity(Some(&active), EmbeddingStrategy::ChunkFirst),
                None,
                "files present: {files:?}"
            );
        }
    }
}
