use crate::features::model_management::recommend::{recommend_chat_models, MachineProfileProbe};
use lattice_core::domain::models::embedding_defaults::default_embedding_model;
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::Result;
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
    repository: Arc<dyn DownloadedModelRepository>,
    /// Builds this computer's profile, which the chat model is recommended for.
    probe: Arc<dyn MachineProfileProbe>,
}

impl CheckFirstRunStatusUseCase {
    pub fn new(
        repository: Arc<dyn DownloadedModelRepository>,
        probe: Arc<dyn MachineProfileProbe>,
    ) -> Self {
        Self { repository, probe }
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

    /// The model the recommender puts first for this computer, with a fresh install's KV cache
    /// setting; none when nothing fits.
    async fn chat_recommendation(&self) -> Option<RecommendedModel> {
        let top = recommend_chat_models(self.probe.as_ref(), false)
            .await
            .into_iter()
            .next();
        match &top {
            Some(top) => debug!(chat_id = %top.model_id, "Chat model recommendation resolved"),
            None => warn!("No chat model fits this computer; recommending none"),
        }
        top.map(|top| RecommendedModel {
            model_id: top.model_id,
            display_name: top.name,
            // The pinned file size, not `size_gb` read back as GiB, which
            // overstated every download by 7%.
            estimated_size_bytes: top.download_bytes,
        })
    }
}

fn embedding_recommendation() -> Option<RecommendedModel> {
    // One model on every machine; the device decides only GPU offload.
    let default = default_embedding_model();
    let Some(entry) = lattice_core::domain::models::curated::get_curated_embedding_models()
        .into_iter()
        .find(|m| m.id == default.curated_id)
    else {
        warn!(
            model_id = default.curated_id,
            "Default embedding model is not in the curated catalog; nothing to recommend"
        );
        return None;
    };
    debug!(
        model_id = default.curated_id,
        dimension = default.dimension,
        "Embedding model recommendation resolved"
    );

    Some(RecommendedModel {
        model_id: default.curated_id.to_string(),
        // The setup screen labels the row "Embedding" already.
        display_name: default.display_name.to_string(),
        // The catalog's own figure: the pinned files' sizes.
        estimated_size_bytes: entry.total_size_bytes,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
    use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_repo() -> Arc<DownloadedModelRepository> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        Arc::new(DownloadedModelRepository::new(pool))
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
        let use_case = CheckFirstRunStatusUseCase::new(repo, processor_only_probe());

        let response = use_case.execute().await.expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(default_embedding_model().curated_id)
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

        let use_case = CheckFirstRunStatusUseCase::new(repo, processor_only_probe());
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

        let use_case = CheckFirstRunStatusUseCase::new(repo, processor_only_probe());
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(default_embedding_model().curated_id)
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

        let pending_repo = Arc::new(DownloadedModelRepository::new(pool));
        let use_case = CheckFirstRunStatusUseCase::new(pending_repo, processor_only_probe());
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(
            response.needs_setup,
            "embedding row with status='downloading' must not satisfy first-run check"
        );
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(default_embedding_model().curated_id)
        );

        // Sanity: empty `repo` still also reports needs_setup.
        let baseline = CheckFirstRunStatusUseCase::new(repo, processor_only_probe())
            .execute()
            .await
            .expect("baseline execute");
        assert!(baseline.needs_setup);
    }

    fn processor_only_probe() -> Arc<dyn MachineProfileProbe> {
        Arc::new(
            crate::features::model_management::recommend::FixedMachineProfile(machine(Vec::new())),
        )
    }

    fn machine(
        accelerators: Vec<lattice_core::domain::models::fit::Accelerator>,
    ) -> lattice_core::domain::models::fit::MachineProfile {
        lattice_core::domain::models::fit::MachineProfile {
            accelerators,
            system_ram_bytes: 32 << 30,
            models_volume_free_bytes: 500 << 30,
            os: "windows".into(),
        }
    }

    fn rtx_4080_profile() -> lattice_core::domain::models::fit::MachineProfile {
        use lattice_core::domain::models::fit::{Accelerator, AcceleratorKind, Backend};
        machine(vec![Accelerator {
            name: "NVIDIA GeForce RTX 4080".into(),
            total_bytes: 16 << 30,
            kind: AcceleratorKind::Dedicated,
            backend: Backend::Cuda,
        }])
    }

    fn top_recommendation(profile: &lattice_core::domain::models::fit::MachineProfile) -> String {
        use lattice_core::domain::models::curated::curated_chat_models;
        use lattice_core::domain::models::fit::{Candidate, Companions, Recommender};
        let catalog: Vec<Candidate> = curated_chat_models()
            .iter()
            .map(Candidate::from_curated)
            .collect();
        Recommender::new(Companions::default_pair(), false)
            .recommend(profile, &catalog)
            .remove(0)
            .id
    }

    #[tokio::test]
    async fn first_run_names_the_top_recommendation_as_the_chat_model() {
        use crate::features::model_management::recommend::FixedMachineProfile;
        let profile = rtx_4080_profile();
        let probe = Arc::new(FixedMachineProfile(profile.clone()));

        let response = CheckFirstRunStatusUseCase::new(setup_repo().await, probe)
            .execute()
            .await
            .expect("execute should succeed");

        let chat = response.chat_model.expect("a chat model is offered");
        assert_eq!(chat.model_id, top_recommendation(&profile));
    }

    #[tokio::test]
    async fn first_run_with_a_machine_probe_still_offers_the_embedding_model() {
        use crate::features::model_management::recommend::FixedMachineProfile;
        let probe = Arc::new(FixedMachineProfile(rtx_4080_profile()));

        let response = CheckFirstRunStatusUseCase::new(setup_repo().await, probe)
            .execute()
            .await
            .expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(default_embedding_model().curated_id)
        );
        assert!(response.embedding_model.is_some());
        let chat = response.chat_model.as_ref().expect("chat model");
        let embedding = response.embedding_model.as_ref().expect("embedding model");
        assert_eq!(
            response.total_estimated_size_bytes,
            Some(chat.estimated_size_bytes + embedding.estimated_size_bytes)
        );
    }

    #[tokio::test]
    async fn first_run_without_a_usable_card_still_recommends_a_chat_model() {
        use crate::features::model_management::recommend::FixedMachineProfile;
        let profile = machine(Vec::new());
        let probe = Arc::new(FixedMachineProfile(profile.clone()));

        let response = CheckFirstRunStatusUseCase::new(setup_repo().await, probe)
            .execute()
            .await
            .expect("a failed device probe is not an error");

        assert!(response.needs_setup);
        let chat = response.chat_model.expect("a chat model is offered");
        assert_eq!(chat.model_id, top_recommendation(&profile));
        assert!(response.embedding_model.is_some());
    }
}
