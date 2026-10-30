//! The bridge between the reranking adapter and the process manager.
//!
//! Search may not name the LLM feature, which owns the sidecar manager,
//! device detection and GPU admission, so the reranking server is started
//! here: a bundled llama-server through `SidecarManager::start_shared`, on the
//! GPU only when it fits in what the models already loaded leave free, and
//! otherwise on the CPU with no device at all. Nothing is unloaded for it, so
//! it never pushes a chat model off the GPU; once there, it is a server their
//! admission may stop while it is idle.

use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};

use super::current_server::CurrentServer;
use crate::features::llm::engine::sidecar_manager::{
    resident_bytes_of, PoolingServing, PoolingTask, SidecarConfig, SidecarManager,
    SpeculativeDecoding,
};
use crate::features::llm::instances::{LlmInstances, SidecarBudget};
use crate::features::llm::loading::LlmRuntime;
use crate::features::search::engine::llama_reranker::{RerankLaunch, RerankServerStarter};
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Starts reranking servers on the bundled llama-server.
pub(crate) struct SidecarRerankStarter {
    /// `None` where no server can start: tests, and tools that build a
    /// container without the app.
    app: Option<tauri::AppHandle>,
    instances: Arc<LlmInstances>,
    runtime: LlmRuntime,
    current: Arc<CurrentServer>,
}

impl SidecarRerankStarter {
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
        pooling: Some(PoolingServing {
            ubatch: launch.window_tokens,
            task: PoolingTask::Reranking,
        }),
    }
}

#[async_trait]
impl RerankServerStarter for SidecarRerankStarter {
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

    async fn start(&self, launch: &RerankLaunch) -> Result<Arc<RunningLlamaServer>> {
        let app = self.app.as_ref().ok_or_else(|| {
            AppError::ModelLoadFailed(
                "The reranking server can only start inside the running app".into(),
            )
        })?;
        let mut config = sidecar_config(launch);
        let turn = if launch.gpu {
            let budget = SidecarBudget::new(Some(app.clone()), self.runtime.clone());
            let (fits, turn) = self
                .instances
                .admit_without_eviction(&launch.name, &config, &budget)
                .await;
            if !fits {
                config = config.without_gpu_offload();
            }
            turn
        } else {
            None
        };
        let handle = SidecarManager::start_shared(app, config.clone()).await;
        drop(turn);
        let handle = handle.map_err(|e| {
            AppError::ModelLoadFailed(format!("The reranking server did not start: {e}"))
        })?;
        if let Some(reason) = handle.degraded() {
            tracing::warn!(model = %launch.name, reason, "reranking.server.degraded");
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
        self.current.record(launch.name.clone(), &server, &handle);
        Ok(server)
    }
}
