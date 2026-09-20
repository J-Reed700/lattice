use crate::application::ports::system_info::SystemInfoPort;
use crate::domain::curated_models::{get_curated_llm_models, recommend_chat_model_for_ram};
use crate::domain::embedding_constants::default_embedding_model;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;
use crate::shared::utils::gpu_acceleration_available;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModel {
    pub model_id: String,
    pub display_name: String,
    pub estimated_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRunStatusResponse {
    pub needs_setup: bool,
    /// Legacy: point at embedding model id. New callers read `embedding_model`.
    pub recommended_model_id: Option<String>,
    pub recommended_model_name: Option<String>,
    pub estimated_size_bytes: Option<u64>,

    pub embedding_model: Option<RecommendedModel>,
    pub chat_model: Option<RecommendedModel>,
    pub total_estimated_size_bytes: Option<u64>,
}

pub struct CheckFirstRunStatusUseCase {
    repository: DownloadedModelRepository,
    /// Without a probe, falls back to the small-tier chat recommendation.
    system_info: Option<Arc<dyn SystemInfoPort>>,
}

impl CheckFirstRunStatusUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self {
            repository,
            system_info: None,
        }
    }

    pub fn with_system_info(
        repository: DownloadedModelRepository,
        system_info: Arc<dyn SystemInfoPort>,
    ) -> Self {
        Self {
            repository,
            system_info: Some(system_info),
        }
    }

    pub async fn execute(&self) -> Result<FirstRunStatusResponse> {
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
            // Legacy fields point at the embedding model.
            recommended_model_id: embedding.as_ref().map(|m| m.model_id.clone()),
            recommended_model_name: embedding.as_ref().map(|m| m.display_name.clone()),
            estimated_size_bytes: embedding.as_ref().map(|m| m.estimated_size_bytes),
            embedding_model: embedding,
            chat_model: chat,
            total_estimated_size_bytes: if total_size > 0 {
                Some(total_size)
            } else {
                None
            },
        }
    }

    async fn chat_recommendation(&self) -> Option<RecommendedModel> {
        // These tiers describe system RAM. Backend/vendor identity does not
        // establish whether GPU memory is dedicated: AMD APUs, for example,
        // expose ROCm while sharing RAM. Do not add an unverified GPU budget.
        // Probe failure falls through to the small-tier recommendation.
        let system_ram_gb = match &self.system_info {
            Some(probe) => match probe.get_system_info().await {
                Ok(info) => info.total_ram_gb,
                Err(e) => {
                    warn!(error = %e, "First-run hardware probe failed; using small-tier default");
                    0.0
                }
            },
            None => 0.0,
        };

        let chat_id = recommend_chat_model_for_ram(system_ram_gb);
        debug!(system_ram_gb, chat_id, "Chat model recommendation resolved");

        get_curated_llm_models()
            .into_iter()
            .find(|m| m.id == chat_id)
            .map(|m| RecommendedModel {
                model_id: m.id,
                display_name: m.name,
                estimated_size_bytes: (m.size_gb * 1_073_741_824.0) as u64,
            })
    }
}

fn embedding_recommendation() -> Option<RecommendedModel> {
    // Qwen3 where there is a GPU to run it, MiniLM where there is not.
    let default = default_embedding_model(gpu_acceleration_available());
    let entry = crate::domain::curated_models::get_curated_embedding_models()
        .into_iter()
        .find(|m| m.id == default.curated_id);
    if entry.is_none() {
        warn!(
            model_id = default.curated_id,
            "Default embedding model is not in the curated catalog; the download will fail"
        );
    }
    debug!(
        model_id = default.curated_id,
        dimension = default.dimension,
        "Embedding model recommendation resolved"
    );

    Some(RecommendedModel {
        model_id: default.curated_id.to_string(),
        // The setup screen labels the row "Embedding" already.
        display_name: default.display_name.to_string(),
        // The catalog's own figure. The fallback is MiniLM's observed size, and
        // under-reporting a 1.2 GB download would under-warn the disk check, so
        // a missing entry is logged above rather than passed off as small.
        estimated_size_bytes: entry.map(|m| m.total_size_bytes).unwrap_or(91_700_000),
    })
}

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
            Some(default_embedding_model(gpu_acceleration_available()).curated_id)
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
            Some(default_embedding_model(gpu_acceleration_available()).curated_id)
        );
    }

    #[tokio::test]
    async fn test_first_run_when_embedding_model_present_but_status_not_completed() {
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
            Some(default_embedding_model(gpu_acceleration_available()).curated_id)
        );

        // Sanity: empty `repo` still also reports needs_setup.
        let baseline = CheckFirstRunStatusUseCase::new(repo)
            .execute()
            .await
            .expect("baseline execute");
        assert!(baseline.needs_setup);
    }

    #[tokio::test]
    async fn gpu_memory_never_promotes_a_system_ram_tier() {
        use crate::application::ports::system_info::{ComputeType, GpuInfo, MockSystemInfoPort};

        let repo = setup_repo().await;
        // Exercise the actual onboarding response, including the AMD APU case
        // the old vendor-based test mislabeled as a discrete GPU.
        for (ram, expected) in [
            (4.0, "qwen3.5-2b-q4_k_m"),
            (8.0, "qwen3.5-4b-q4_k_m"),
            (16.0, "qwen3.5-9b-q4_k_m"),
        ] {
            for (name, compute_type) in [
                ("Apple M3", ComputeType::Metal),
                ("AMD Radeon 780M integrated", ComputeType::Rocm),
                ("AMD Radeon RX 7900", ComputeType::Rocm),
                ("Intel integrated graphics", ComputeType::None),
                ("NVIDIA RTX", ComputeType::Cuda),
            ] {
                let probe = Arc::new(MockSystemInfoPort::new());
                probe.set_ram(ram);
                probe.set_gpu(Some(GpuInfo {
                    name: name.into(),
                    vram_gb: Some(24.0),
                    compute_type,
                }));
                let result = CheckFirstRunStatusUseCase::with_system_info(repo.clone(), probe)
                    .execute()
                    .await
                    .unwrap();
                assert_eq!(
                    result.chat_model.unwrap().model_id,
                    expected,
                    "{name}, {ram} GB"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_chat_recommendation_uses_hardware_probe_when_available() {
        use crate::application::ports::system_info::MockSystemInfoPort;

        let repo = setup_repo().await;

        // Low-end probe (4 GB RAM, no GPU) → the smallest tier.
        let low_end_probe = std::sync::Arc::new(MockSystemInfoPort::low_end());
        let low_end_use_case =
            CheckFirstRunStatusUseCase::with_system_info(repo.clone(), low_end_probe);
        let low_end_response = low_end_use_case.execute().await.expect("low-end execute");
        assert_eq!(
            low_end_response
                .chat_model
                .as_ref()
                .map(|m| m.model_id.as_str()),
            Some("qwen3.5-2b-q4_k_m"),
            "4 GB box should get the small-tier chat model"
        );

        // Default probe (16 GB RAM, no GPU) → the >= 16 GB tier.
        let default_probe = std::sync::Arc::new(MockSystemInfoPort::new());
        let default_use_case =
            CheckFirstRunStatusUseCase::with_system_info(repo.clone(), default_probe);
        let default_response = default_use_case.execute().await.expect("default execute");
        assert_eq!(
            default_response
                .chat_model
                .as_ref()
                .map(|m| m.model_id.as_str()),
            Some("qwen3.5-9b-q4_k_m"),
            "16 GB box should get the high-tier chat model"
        );
    }
}
