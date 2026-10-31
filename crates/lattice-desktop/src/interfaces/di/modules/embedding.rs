//! The bridge between the embedding adapter and the process manager.
//!
//! The embedding feature may not name the LLM feature, which owns the sidecar
//! manager, the GGUF reader, device detection and GPU admission. The
//! composition root sees both, so the embedding server is started here,
//! through [`PoolingServers`]. A chat model and the reranker come first on the
//! GPU: a query takes only what they leave, and only indexing, too slow
//! anywhere else, may unload an idle model.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::Result;

use super::current_server::CurrentServer;
use super::pooling_servers::PoolingServers;
use crate::features::embedding::llama_cpp::{
    EmbeddingHeader, EmbeddingLaunch, EmbeddingServerStarter, EmbeddingWork,
};
use crate::features::llm::engine::gguf_metadata::read_model_info;
use crate::features::llm::engine::sidecar_manager::{
    PoolingServing, PoolingTask, SidecarConfig, SpeculativeDecoding,
};
use crate::features::llm::instances::{GpuClaim, LlmInstances};
use crate::features::llm::loading::LlmRuntime;
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Starts embedding servers on the bundled llama-server.
pub(crate) struct SidecarEmbeddingStarter {
    servers: PoolingServers,
}

impl SidecarEmbeddingStarter {
    pub(crate) fn new(
        app: Option<tauri::AppHandle>,
        instances: Arc<LlmInstances>,
        runtime: LlmRuntime,
        current: Arc<CurrentServer>,
    ) -> Self {
        Self {
            servers: PoolingServers::new(app, instances, runtime, current),
        }
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
        pooling: Some(PoolingServing::new(
            launch.window_tokens,
            PoolingTask::Embedding,
        )),
    }
}

#[async_trait]
impl EmbeddingServerStarter for SidecarEmbeddingStarter {
    async fn header(&self, gguf: &Path) -> Option<EmbeddingHeader> {
        read_header(gguf).await
    }

    async fn accelerated(&self) -> bool {
        self.servers.accelerated().await
    }

    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningLlamaServer>> {
        let name = launch.gguf.file_name().map_or_else(
            || "embedding server".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let claim = launch.gpu.then_some(match launch.work {
            EmbeddingWork::Light => GpuClaim::Spare,
            EmbeddingWork::Indexing => GpuClaim::IdleModels,
        });
        self.servers
            .start(name, sidecar_config(launch), claim)
            .await
    }
}
