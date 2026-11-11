use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use super::EmbeddingLoader;
use crate::features::embedding::llama_cpp::{
    EmbeddingHeader, EmbeddingLaunch, EmbeddingServerStarter,
};
use crate::infrastructure::llama_server::RunningLlamaServer;
use crate::infrastructure::security::SecurityContext;
use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::application::ports::{EmbeddingIndexBinding, EmbeddingPort};
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};

struct ActiveEmbedding(Option<DownloadedModel>, bool);

#[async_trait]
impl ActiveModels for ActiveEmbedding {
    async fn get_active_chat_model(&self) -> Result<Option<DownloadedModel>> {
        Ok(None)
    }
    async fn get_active_embedding_model(&self) -> Result<Option<DownloadedModel>> {
        if self.1 {
            return Err(AppError::Database("active model query failed".into()));
        }
        Ok(self.0.clone())
    }
    async fn get_active_utility_model(&self) -> Result<Option<DownloadedModel>> {
        Ok(None)
    }
    async fn set_active_chat_model(&self, _: &str) -> Result<()> {
        Ok(())
    }
    async fn set_active_embedding_model(
        &self,
        _: &str,
        _: Option<&ArtifactIdentity>,
    ) -> Result<()> {
        Ok(())
    }
    async fn set_active_utility_model(&self, _: &str) -> Result<()> {
        Ok(())
    }
    async fn clear_active_chat_model(&self) -> Result<()> {
        Ok(())
    }
    async fn clear_active_embedding_model(&self) -> Result<()> {
        Ok(())
    }
    async fn clear_active_utility_model(&self) -> Result<()> {
        Ok(())
    }
}

struct NoBinding;

#[async_trait]
impl EmbeddingIndexBinding for NoBinding {
    async fn bind_first(&self, _: &dyn EmbeddingPort) -> Result<()> {
        Ok(())
    }
}

struct NoServer;

#[async_trait]
impl EmbeddingServerStarter for NoServer {
    async fn header(&self, _: &Path) -> Option<EmbeddingHeader> {
        None
    }
    async fn accelerated(&self) -> bool {
        false
    }
    async fn start(&self, _: &EmbeddingLaunch) -> Result<Arc<RunningLlamaServer>> {
        Err(AppError::ModelLoadFailed("no server in this test".into()))
    }
}

fn loader_for(model: Option<DownloadedModel>) -> EmbeddingLoader {
    loader_with(ActiveEmbedding(model, false))
}

fn loader_with(active: ActiveEmbedding) -> EmbeddingLoader {
    EmbeddingLoader {
        active_models: Arc::new(active),
        security: Arc::new(SecurityContext::new()),
        index: Arc::new(NoBinding),
        starter: Arc::new(NoServer),
    }
}

#[tokio::test]
async fn no_active_model_reads_as_not_installed() {
    let port = loader_for(None).load().await;

    let error = port.embed_single("hello").await.unwrap_err();

    assert!(matches!(error, AppError::AiModelsNotInstalled(_)));
}

#[tokio::test]
async fn an_active_model_whose_file_is_missing_is_not_reported_as_not_installed() {
    let path = std::env::temp_dir().join("lattice-missing-embedder-9f3a.gguf");
    let model = DownloadedModel::new(
        "id".into(),
        "Active embedder".into(),
        "active-embedder".into(),
        ModelLocation::LocalFile { path },
        1,
        "bert".into(),
        None,
    )
    .unwrap();

    let port = loader_for(Some(model)).load().await;
    let error = port.embed_single("hello").await.unwrap_err();

    assert!(
        !matches!(error, AppError::AiModelsNotInstalled(_)),
        "a model whose file is missing was reported as not installed: {error}"
    );
    assert!(
        error
            .to_string()
            .contains("The file of the active embedding model 'Active embedder' is missing: "),
        "the model and its missing file are not named in: {error}"
    );
}

#[tokio::test]
async fn a_failed_active_model_query_is_not_reported_as_not_installed() {
    let port = loader_with(ActiveEmbedding(None, true)).load().await;
    let error = port.embed_single("hello").await.unwrap_err();

    assert!(
        !matches!(error, AppError::AiModelsNotInstalled(_)),
        "a failed query was reported as not installed: {error}"
    );
    assert!(
        error.to_string().contains("active model query failed"),
        "the query failure is missing from: {error}"
    );
}
