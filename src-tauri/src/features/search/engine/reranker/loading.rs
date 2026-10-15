//! Loading a reranker by architecture, lazily on first use.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::shared::error::{AppError, Result, ResultExt};

use super::{resolve_model_dir, Qwen3RerankerService, RerankResult, Reranker, RerankerService};

const DEFAULT_INIT_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct LazyRerankerState {
    instance: Option<Arc<dyn Reranker>>,
    last_init_failure: Option<Instant>,
}

/// Lazily loads one shared reranker and notices artifacts downloaded after boot.
pub struct LazyReranker {
    model_path: PathBuf,
    state: tokio::sync::Mutex<LazyRerankerState>,
    retry_delay: Duration,
}

impl std::fmt::Debug for LazyReranker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyReranker")
            .field("model_path", &self.model_path)
            .field("retry_delay", &self.retry_delay)
            .finish_non_exhaustive()
    }
}

impl LazyReranker {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            state: tokio::sync::Mutex::new(LazyRerankerState::default()),
            retry_delay: DEFAULT_INIT_RETRY_DELAY,
        }
    }

    fn artifacts_available(&self) -> bool {
        let Some(model_dir) = self.model_path.parent() else {
            return false;
        };
        self.model_path.is_file()
            && model_dir.join("tokenizer.json").is_file()
            && model_dir.join("config.json").is_file()
    }

    async fn get_or_init(&self) -> Result<Arc<dyn Reranker>> {
        if !self.artifacts_available() {
            return Err(AppError::InvalidState(format!(
                "Reranker artifacts are not available at {}",
                self.model_path.display()
            )));
        }

        // Keep initialization under one lock so concurrent searches cannot load
        // duplicate model copies. Inference itself uses RerankerService's own lock.
        let mut state = self.state.lock().await;
        if let Some(service) = state.instance.as_ref() {
            return Ok(Arc::clone(service));
        }
        if state
            .last_init_failure
            .is_some_and(|failed_at| failed_at.elapsed() < self.retry_delay)
        {
            return Err(AppError::InvalidState(
                "Reranker initialization is cooling down after a load failure".to_string(),
            ));
        }

        match load_reranker(&self.model_path).await {
            Ok(service) => {
                state.instance = Some(Arc::clone(&service));
                state.last_init_failure = None;
                Ok(service)
            }
            Err(error) => {
                state.last_init_failure = Some(Instant::now());
                Err(error)
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct RerankerModelConfig {
    pub(super) model_type: String,
    #[serde(default)]
    pub(super) architectures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RerankerArchitecture {
    BertSequenceClassifier,
    Qwen3CausalClassifier,
}

pub(super) fn detect_reranker_architecture(
    config: &RerankerModelConfig,
) -> Result<RerankerArchitecture> {
    match config.model_type.to_ascii_lowercase().as_str() {
        "bert" => Ok(RerankerArchitecture::BertSequenceClassifier),
        "qwen3"
            if config
                .architectures
                .iter()
                .any(|architecture| architecture == "Qwen3ForCausalLM") =>
        {
            Ok(RerankerArchitecture::Qwen3CausalClassifier)
        }
        "qwen3" => Err(AppError::InvalidConfig(
            "Qwen3 reranker artifacts must declare Qwen3ForCausalLM; an embedding checkpoint cannot be used as a reranker"
                .to_string(),
        )),
        other => Err(AppError::InvalidConfig(format!(
            "Unsupported reranker architecture `{other}`"
        ))),
    }
}

/// Load the correct local reranker implementation from its checkpoint metadata.
///
/// Both chat retrieval and direct search call through the same [`Reranker`]
/// contract. Model selection belongs here so callers cannot accidentally apply
/// different prompt, scoring, or fallback policies.
pub async fn load_reranker(model_path: impl AsRef<Path>) -> Result<Arc<dyn Reranker>> {
    let resolved = resolve_model_dir(model_path.as_ref())?;
    let config_path = resolved.dir.join("config.json");
    let config_json =
        std::fs::read_to_string(&config_path).context("Failed to read reranker config.json")?;
    let config: RerankerModelConfig = serde_json::from_str(&config_json).map_err(|error| {
        AppError::InvalidConfig(format!("Invalid reranker config.json: {error}"))
    })?;

    match detect_reranker_architecture(&config)? {
        RerankerArchitecture::BertSequenceClassifier => {
            Ok(Arc::new(RerankerService::new(&resolved.dir).await?))
        }
        RerankerArchitecture::Qwen3CausalClassifier => {
            Ok(Arc::new(Qwen3RerankerService::new(&resolved.dir).await?))
        }
    }
}

#[async_trait::async_trait]
impl Reranker for LazyReranker {
    fn is_available(&self) -> bool {
        self.artifacts_available()
    }

    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        self.get_or_init()
            .await?
            .rerank(query, documents, top_k)
            .await
    }
}
