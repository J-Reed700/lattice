//! The bridge between the reranking adapter and the process manager.
//!
//! Search may not name the LLM feature, which owns the sidecar manager,
//! device detection and GPU admission, so the reranking server is started
//! here, through [`PoolingServers`]. Scoring passages is too slow anywhere but
//! a GPU, so it may stop an idle embedding server for a place there, which
//! runs well elsewhere, but never a chat model.

use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::Result;

use super::current_server::CurrentServer;
use super::pooling_servers::PoolingServers;
use crate::features::llm::engine::sidecar_manager::{
    PoolingServing, PoolingTask, SidecarConfig, SpeculativeDecoding,
};
use crate::features::llm::instances::{GpuClaim, LlmInstances};
use crate::features::llm::loading::LlmRuntime;
use crate::features::search::engine::llama_reranker::{RerankLaunch, RerankServerStarter};
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Starts reranking servers on the bundled llama-server.
pub(crate) struct SidecarRerankStarter {
    servers: PoolingServers,
}

impl SidecarRerankStarter {
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

/// The sidecar configuration of the server `launch` describes: its window and
/// slots, every layer offloaded or none, and nothing a chat server sets.
pub(crate) fn sidecar_config(launch: &RerankLaunch) -> SidecarConfig {
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
            PoolingTask::Reranking,
        )),
    }
}

#[async_trait]
impl RerankServerStarter for SidecarRerankStarter {
    async fn accelerated(&self) -> bool {
        self.servers.accelerated().await
    }

    async fn start(&self, launch: &RerankLaunch) -> Result<Arc<RunningLlamaServer>> {
        let claim = launch.gpu.then_some(GpuClaim::IdleServers);
        self.servers
            .start(launch.name.clone(), sidecar_config(launch), claim)
            .await
    }
}
