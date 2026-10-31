//! Where the embedding and reranking servers run, and starting them there.
//!
//! The GPU models are priced against goes to a chat model first, then the
//! reranker, then the embedder, so neither of the two ever costs a chat its
//! speed: three on one card that cannot hold them page each other out. A
//! server its [`GpuClaim`] does not seat there runs on the integrated GPU
//! beside the card when there is one, else on the CPU.

use std::sync::Arc;

use lattice_core::shared::error::{AppError, Result};

use super::current_server::CurrentServer;
use crate::features::llm::engine::sidecar_manager::{
    resident_bytes_of, SidecarConfig, SidecarManager,
};
use crate::features::llm::engine::system::backend_devices::spare_device;
use crate::features::llm::instances::{GpuClaim, LlmInstances, SidecarBudget};
use crate::features::llm::loading::LlmRuntime;
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Starts the servers of one role on the bundled llama-server.
pub(super) struct PoolingServers {
    /// `None` where no server can start: tests, and tools that build a
    /// container without the app.
    app: Option<tauri::AppHandle>,
    instances: Arc<LlmInstances>,
    runtime: LlmRuntime,
    current: Arc<CurrentServer>,
}

impl PoolingServers {
    pub(super) fn new(
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

    /// Whether this machine has an accelerator a server can offload to.
    pub(super) async fn accelerated(&self) -> bool {
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

    /// Start `config`, or hand back the server running for it, named `name`.
    /// `claim` is what it may take for a place on the GPU; `None` starts it
    /// as it is.
    pub(super) async fn start(
        &self,
        name: String,
        config: SidecarConfig,
        claim: Option<GpuClaim>,
    ) -> Result<Arc<RunningLlamaServer>> {
        let role = self.current.role();
        let app = self.app.as_ref().ok_or_else(|| {
            AppError::ModelLoadFailed(format!("The {role} can only start inside the running app"))
        })?;
        let (config, turn) = match claim {
            Some(claim) => self.place(app, &name, config, claim).await,
            None => (config, None),
        };
        let handle = SidecarManager::start_shared(app, config.clone()).await;
        drop(turn);
        let handle = handle
            .map_err(|e| AppError::ModelLoadFailed(format!("The {role} did not start: {e}")))?;
        if let Some(reason) = handle.degraded() {
            tracing::warn!(model = %name, role, reason, "pooling.server.degraded");
        }
        let gpu_bytes = (handle.n_gpu_layers() > 0 && config.on_offload_gpu()).then(|| {
            resident_bytes_of(&config.model_path, handle.context_size(), config.buffers())
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

    /// `config` where `claim` seats it, with the admission turn to hold until
    /// it has started when it takes the GPU.
    async fn place(
        &self,
        app: &tauri::AppHandle,
        name: &str,
        config: SidecarConfig,
        claim: GpuClaim,
    ) -> (SidecarConfig, Option<tokio::sync::MutexGuard<'_, ()>>) {
        let budget = SidecarBudget::new(Some(app.clone()), self.runtime.clone());
        let (seated, turn) = self
            .instances
            .claim_gpu(name, &config, claim, &budget)
            .await;
        if seated {
            return (config, turn);
        }
        let devices = self.runtime.devices.detect(app).await;
        let spare = spare_device(devices).map(|device| device.id.as_str());
        (config.off_the_gpu(spare), None)
    }
}
