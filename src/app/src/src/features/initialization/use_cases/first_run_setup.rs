//! First Run Setup Use Case
//!
//! Detects if this is the first run and if a default embedding model needs to be downloaded.
//!
//! # Purpose
//!
//! Checks the model registry (DB) for any completed embedding model. If none exist,
//! recommends downloading the default model (DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME).
//!
//! # Business Logic
//!
//! 1. Query the model registry for any embedding model with status='completed'.
//! 2. If none found, return needs_setup=true with recommendation.
//! 3. If at least one exists, return needs_setup=false.
//!
//! # Example
//!
//! ```rust,ignore
//! let use_case = CheckFirstRunStatusUseCase::new(repository);
//! let response = use_case.execute().await?;
//!
//! if response.needs_setup {
//!     println!("First run detected. Recommended model: {}", response.recommended_model_id.unwrap());
//! }
//! ```

use crate::application::ports::system_info::SystemInfoPort;
use crate::domain::curated_models::{get_curated_llm_models, recommend_chat_model_for_ram};
use crate::domain::embedding_constants::{
    DEFAULT_EMBEDDING_MODEL_CURATED_ID, DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME,
};
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

// =============================================================================
// DTOs
// =============================================================================

/// One model the first-run modal will install. Embedding + chat are bundled
/// into a single "Install Recommended AI" click; the user doesn't pick from
/// a catalog by default. Power users can still override via "More options".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModel {
    /// Curated catalog id (the key the download use case expects).
    pub model_id: String,
    /// Human-friendly display name for the modal.
    pub display_name: String,
    /// Estimated bytes on disk after download. Drives the "(4.2 GB)"
    /// callout next to the install button.
    pub estimated_size_bytes: u64,
}

/// Response DTO for first-run status check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRunStatusResponse {
    /// Whether first-run setup is needed (no embedding model in registry).
    pub needs_setup: bool,
    /// LEGACY: kept for transitional frontend compatibility — points at the
    /// embedding model id. New callers should read `embedding_model.model_id`.
    pub recommended_model_id: Option<String>,
    /// LEGACY: see `recommended_model_id`.
    pub recommended_model_name: Option<String>,
    /// LEGACY: see `recommended_model_id`. Total of embedding + chat now lives
    /// in the per-recommendation entries below.
    pub estimated_size_bytes: Option<u64>,

    /// Embedding recommendation. Always present when `needs_setup` is true —
    /// every install path needs an embedding model for indexing.
    pub embedding_model: Option<RecommendedModel>,
    /// Chat model recommendation, sized to the user's hardware. Present
    /// when `needs_setup` is true. Frontend installs both in parallel via
    /// the existing batch-download path.
    pub chat_model: Option<RecommendedModel>,
    /// Sum of embedding + chat estimated bytes. Drives the install
    /// button's "(N.N GB)" affordance.
    pub total_estimated_size_bytes: Option<u64>,
}

// =============================================================================
// Use Case
// =============================================================================

/// Check first-run status use case
pub struct CheckFirstRunStatusUseCase {
    /// Repository used to query the model registry
    repository: DownloadedModelRepository,
    /// Hardware probe used to size the recommended chat model. Optional
    /// because legacy call sites (and tests) construct without it; when
    /// absent, falls back to the safe small-tier recommendation.
    system_info: Option<Arc<dyn SystemInfoPort>>,
}

impl CheckFirstRunStatusUseCase {
    /// Construct without hardware detection. Recommends the small-tier
    /// chat model as a safe default. Prefer `with_system_info` for
    /// production callers so users with capable machines aren't given
    /// an underpowered first-impression model.
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self {
            repository,
            system_info: None,
        }
    }

    /// Construct with a hardware probe so the chat model recommendation
    /// fits the user's actual RAM/VRAM budget.
    pub fn with_system_info(
        repository: DownloadedModelRepository,
        system_info: Arc<dyn SystemInfoPort>,
    ) -> Self {
        Self {
            repository,
            system_info: Some(system_info),
        }
    }

    /// Execute the use case
    pub async fn execute(&self) -> Result<FirstRunStatusResponse> {
        // Registry is the source of truth — filesystem heuristics drift and miss
        // models stored in subdirs (see Phase 3 fix: the welcome modal never
        // dismissed because a non-recursive top-level .onnx scan never matched
        // the per-model subdirectory layout).
        debug!("Checking first-run status via model registry");
        let has_embedding = self.repository.has_any_embedding_model().await?;
        if has_embedding {
            debug!("Embedding model present — not first run");
            return Ok(FirstRunStatusResponse {
                needs_setup: false,
                recommended_model_id: None,
                recommended_model_name: None,
                estimated_size_bytes: None,
                embedding_model: None,
                chat_model: None,
                total_estimated_size_bytes: None,
            });
        }

        info!("No embedding model in registry — first run detected");
        Ok(self.build_recommendation().await)
    }

    /// Build the recommendation bundle. Probes hardware (if available) to
    /// pick a chat model that comfortably fits the user's RAM, then pairs
    /// it with the default embedding model.
    async fn build_recommendation(&self) -> FirstRunStatusResponse {
        let embedding = embedding_recommendation();
        let chat = self.chat_recommendation().await;

        let total_size = embedding
            .as_ref()
            .map(|m| m.estimated_size_bytes)
            .unwrap_or(0)
            + chat.as_ref().map(|m| m.estimated_size_bytes).unwrap_or(0);

        FirstRunStatusResponse {
            needs_setup: true,
            // Legacy fields point at the embedding model so old frontends
            // keep working through the rollout.
            recommended_model_id: embedding.as_ref().map(|m| m.model_id.clone()),
            recommended_model_name: embedding.as_ref().map(|m| m.display_name.clone()),
            estimated_size_bytes: embedding.as_ref().map(|m| m.estimated_size_bytes),
            embedding_model: embedding,
            chat_model: chat,
            total_estimated_size_bytes: if total_size > 0 { Some(total_size) } else { None },
        }
    }

    async fn chat_recommendation(&self) -> Option<RecommendedModel> {
        // Detect effective RAM. Includes discrete VRAM when present so
        // GPU-equipped boxes get the bigger model. Falls back to small-
        // tier if detection fails — better to under-recommend than crash
        // a 6 GB laptop loading a 7 B model.
        let effective_ram_gb = match &self.system_info {
            Some(probe) => match probe.get_system_info().await {
                Ok(info) => {
                    let vram = info.gpu_info.as_ref().and_then(|g| g.vram_gb).unwrap_or(0.0);
                    info.total_ram_gb + vram
                }
                Err(e) => {
                    warn!(error = %e, "First-run hardware probe failed; using small-tier default");
                    0.0
                }
            },
            None => 0.0,
        };

        let chat_id = recommend_chat_model_for_ram(effective_ram_gb);
        debug!(
            effective_ram_gb,
            chat_id, "Chat model recommendation resolved"
        );

        get_curated_llm_models()
            .into_iter()
            .find(|m| m.id == chat_id)
            .map(|m| RecommendedModel {
                model_id: m.id,
                display_name: m.name,
                // Catalog stores `size_gb` as a float; convert to bytes
                // for the frontend's progress + drawer rendering.
                estimated_size_bytes: (m.size_gb * 1_073_741_824.0) as u64,
            })
    }
}

fn embedding_recommendation() -> Option<RecommendedModel> {
    let catalog_size = crate::domain::curated_models::get_curated_embedding_models()
        .into_iter()
        .find(|m| m.name == DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME)
        .map(|m| m.total_size_bytes);

    Some(RecommendedModel {
        model_id: DEFAULT_EMBEDDING_MODEL_CURATED_ID.to_string(),
        display_name: format!("{} (Embedding Model)", DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME),
        // 91.7 MB sane fallback — observed real-world download size for
        // all-MiniLM-L6-v2 safetensors + tokenizer.json + config.json.
        estimated_size_bytes: catalog_size.unwrap_or(91_700_000),
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_repo() -> DownloadedModelRepository {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        DownloadedModelRepository::new(pool)
    }

    fn make_local_embedding_model(model_id: &str) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Embed {}", model_id),
            model_id.to_string(),
            ModelLocation::LocalDirectory {
                path: std::path::PathBuf::from(format!("/tmp/{}", model_id)),
            },
            512,
            "bge".to_string(),
            None,
        )
        .expect("create local embedding model")
    }

    fn make_local_chat_model(model_id: &str) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Chat {}", model_id),
            model_id.to_string(),
            ModelLocation::LocalFile {
                path: std::path::PathBuf::from(format!("/tmp/{}.gguf", model_id)),
            },
            1024,
            "llama".to_string(),
            None,
        )
        .expect("create local chat model")
    }

    #[tokio::test]
    async fn test_first_run_when_registry_empty_needs_setup() {
        let repo = setup_repo().await;
        let use_case = CheckFirstRunStatusUseCase::new(repo);

        let response = use_case.execute().await.expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(DEFAULT_EMBEDDING_MODEL_CURATED_ID)
        );
        assert!(response.recommended_model_name.is_some());
        assert!(response.estimated_size_bytes.is_some());
        // New shape — both bundle entries populated
        assert!(response.embedding_model.is_some());
        assert!(response.chat_model.is_some());
        assert!(response.total_estimated_size_bytes.is_some());
    }

    #[tokio::test]
    async fn test_not_first_run_when_embedding_model_in_registry() {
        let repo = setup_repo().await;
        let embedding = make_local_embedding_model("test-embed-1");
        repo.save(&embedding).await.expect("save embedding model");

        let use_case = CheckFirstRunStatusUseCase::new(repo);
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(!response.needs_setup);
        assert!(response.recommended_model_id.is_none());
        assert!(response.recommended_model_name.is_none());
        assert!(response.estimated_size_bytes.is_none());
    }

    #[tokio::test]
    async fn test_first_run_when_only_non_embedding_models_in_registry() {
        let repo = setup_repo().await;
        let chat = make_local_chat_model("test-chat-1");
        repo.save(&chat).await.expect("save chat model");

        let use_case = CheckFirstRunStatusUseCase::new(repo);
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(DEFAULT_EMBEDDING_MODEL_CURATED_ID)
        );
    }

    #[tokio::test]
    async fn test_first_run_when_embedding_model_present_but_status_not_completed() {
        // Validates that has_any_embedding_model filters by status='completed'.
        // We bypass save() (which forces 'completed') and insert a row with status='downloading'.
        let repo = setup_repo().await;

        let pool = {
            let p = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(":memory:")
                .await
                .expect("create in-memory pool");
            sqlx::migrate!("./migrations")
                .run(&p)
                .await
                .expect("run migrations");
            p
        };

        sqlx::query(
            r#"
            INSERT INTO models (
                id, model_name, model_id, base_path, total_size_bytes, status,
                model_type, architecture
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'downloading', 'embedding', 'bge')
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind("Pending Embed")
        .bind("pending-embed-1")
        .bind("/tmp/pending-embed-1")
        .bind(512_i64)
        .execute(&pool)
        .await
        .expect("insert pending embedding row");

        let pending_repo = DownloadedModelRepository::new(pool);
        let use_case = CheckFirstRunStatusUseCase::new(pending_repo);
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(
            response.needs_setup,
            "embedding row with status='downloading' must not satisfy first-run check"
        );
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(DEFAULT_EMBEDDING_MODEL_CURATED_ID)
        );

        // Sanity: empty `repo` still also reports needs_setup.
        let baseline = CheckFirstRunStatusUseCase::new(repo)
            .execute()
            .await
            .expect("baseline execute");
        assert!(baseline.needs_setup);
    }

    #[tokio::test]
    async fn test_chat_recommendation_uses_hardware_probe_when_available() {
        use crate::application::ports::system_info::MockSystemInfoPort;

        let repo = setup_repo().await;

        // Low-end probe (4 GB RAM, no GPU) → small-tier Phi-3 mini.
        let low_end_probe = std::sync::Arc::new(MockSystemInfoPort::low_end());
        let low_end_use_case =
            CheckFirstRunStatusUseCase::with_system_info(repo.clone(), low_end_probe);
        let low_end_response = low_end_use_case
            .execute()
            .await
            .expect("low-end execute");
        assert_eq!(
            low_end_response.chat_model.as_ref().map(|m| m.model_id.as_str()),
            Some("phi-3-mini-4k-instruct-q4_k_m"),
            "4 GB box should get the small-tier chat model"
        );

        // Default probe (16 GB RAM, no GPU) → Qwen 2.5 7B (>= 16 GB tier).
        let default_probe = std::sync::Arc::new(MockSystemInfoPort::new());
        let default_use_case =
            CheckFirstRunStatusUseCase::with_system_info(repo.clone(), default_probe);
        let default_response = default_use_case
            .execute()
            .await
            .expect("default execute");
        assert_eq!(
            default_response.chat_model.as_ref().map(|m| m.model_id.as_str()),
            Some("qwen2.5-7b-instruct-q4_k_m"),
            "16 GB box should get the high-tier chat model"
        );
    }
}
