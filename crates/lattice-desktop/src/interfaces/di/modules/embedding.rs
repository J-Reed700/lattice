//! The bridge between the embedding adapter and the process manager.
//!
//! The embedding feature may not name the LLM feature, which owns the sidecar
//! manager, the GGUF reader, device detection and GPU admission. The
//! composition root sees both, so the embedding server is started here: as a
//! bundled llama-server through `SidecarManager::start_shared`, admitted
//! beside the chat models first, and offered to their admission as a server
//! it may stop while idle.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};

use super::current_server::CurrentServer;
use crate::features::embedding::llama_cpp::{
    EmbeddingHeader, EmbeddingLaunch, EmbeddingServerStarter,
};
use crate::features::llm::engine::gguf_metadata::read_model_info;
use crate::features::llm::engine::sidecar_manager::{
    resident_bytes_of, PoolingServing, PoolingTask, SidecarConfig, SidecarManager,
    SpeculativeDecoding,
};
use crate::features::llm::instances::{LlmInstances, SidecarBudget};
use crate::features::llm::loading::LlmRuntime;
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Starts embedding servers on the bundled llama-server.
pub(crate) struct SidecarEmbeddingStarter {
    /// `None` where no server can start: tests, and tools that build a
    /// container without the app.
    app: Option<tauri::AppHandle>,
    instances: Arc<LlmInstances>,
    runtime: LlmRuntime,
    current: Arc<CurrentServer>,
}

impl SidecarEmbeddingStarter {
    pub(crate) fn new(
        app: Option<tauri::AppHandle>,
        instances: Arc<LlmInstances>,
        runtime: LlmRuntime,
        current: Arc<CurrentServer>,
    ) -> Self {
        Self {
            app,
            instances,
            runtime,
            current,
        }
    }

    fn app(&self) -> Result<&tauri::AppHandle> {
        self.app.as_ref().ok_or_else(|| {
            AppError::ModelLoadFailed(
                "The embedding server can only start inside the running app".into(),
            )
        })
    }
}

/// The header facts of the GGUF at `gguf` an embedding server needs; `None`
/// when it cannot be read.
pub(super) async fn read_header(gguf: &Path) -> Option<EmbeddingHeader> {
    let gguf = gguf.to_path_buf();
    let info = tokio::task::spawn_blocking(move || read_model_info(&gguf))
        .await
        .ok()??;
    Some(EmbeddingHeader {
        architecture: info.architecture,
        trained_tokens: info.trained_context_length,
        dimension: info.embedding_length,
        pooling_type: info.pooling_type,
        file_type: info.file_type,
    })
}

/// The sidecar configuration of the server `launch` describes: its window
/// and slots, every layer offloaded or none, and nothing a chat server sets.
pub(super) fn sidecar_config(launch: &EmbeddingLaunch) -> SidecarConfig {
    SidecarConfig {
        model_path: launch.gguf.clone(),
        n_gpu_layers: if launch.gpu { 99 } else { 0 },
        context_size: launch.context_tokens(),
        mtp_capable: false,
        speculative_decoding: SpeculativeDecoding::Off,
        parallel_slots: Some(launch.slots),
        seed: None,
        kv_cache_quantization: false,
        projector_path: None,
        pooling: Some(PoolingServing {
            ubatch: launch.window_tokens,
            task: PoolingTask::Embedding,
        }),
    }
}

#[async_trait]
impl EmbeddingServerStarter for SidecarEmbeddingStarter {
    async fn header(&self, gguf: &Path) -> Option<EmbeddingHeader> {
        read_header(gguf).await
    }

    async fn accelerated(&self) -> bool {
        let Some(app) = self.app.as_ref() else {
            return false;
        };
        self.runtime
            .devices
            .detect(app)
            .await
            .iter()
            .any(|device| device.is_accelerator())
    }

    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningLlamaServer>> {
        let app = self.app()?;
        let config = sidecar_config(launch);
        let name = launch.gguf.file_name().map_or_else(
            || "embedding server".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let budget = SidecarBudget::new(Some(app.clone()), self.runtime.clone());
        let turn = self.instances.make_room_for(&name, &config, &budget).await;
        let handle = SidecarManager::start_shared(app, config.clone()).await;
        drop(turn);
        let handle = handle.map_err(|e| {
            AppError::ModelLoadFailed(format!("The embedding server did not start: {e}"))
        })?;
        if let Some(reason) = handle.degraded() {
            tracing::warn!(model = %name, reason, "embedding.server.degraded");
        }
        let gpu_bytes = (handle.n_gpu_layers() > 0).then(|| {
            resident_bytes_of(
                &config.model_path,
                handle.context_size(),
                config.kv_cache_quantization,
            )
        });
        let watched = Arc::clone(&handle);
        let server = Arc::new(RunningLlamaServer::new(
            handle.endpoint(),
            handle.api_token(),
            gpu_bytes,
            Arc::new(move || watched.is_running()),
        ));
        self.current.record(name, &server, &handle);
        Ok(server)
    }
}
