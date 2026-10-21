//! The bridge between the embedding adapter and the process manager.
//!
//! The embedding feature may not name the LLM feature, which owns the sidecar
//! manager, the GGUF reader, device detection and GPU admission. The
//! composition root sees both, so the embedding server is started here: as a
//! bundled llama-server through `SidecarManager::start_shared`, admitted
//! beside the chat models first, and offered to their admission as a server
//! it may stop while idle.

use std::path::Path;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};

use crate::features::embedding::llama_cpp::{
    EmbeddingHeader, EmbeddingLaunch, EmbeddingServerStarter, RunningEmbeddingServer,
};
use crate::features::llm::engine::gguf_metadata::read_model_info;
use crate::features::llm::engine::sidecar_manager::{
    resident_bytes_of, EmbeddingServing, SidecarConfig, SidecarHandle, SidecarManager,
    SpeculativeDecoding,
};
use crate::features::llm::instances::{IdleServer, LlmInstances, SidecarBudget};
use crate::features::llm::loading::LlmRuntime;

/// The embedding server running now, as admission sees it: what it holds,
/// whether it is busy, and how to stop it.
#[derive(Default)]
pub(crate) struct CurrentEmbeddingServer {
    current: parking_lot::Mutex<Option<Current>>,
}

struct Current {
    name: String,
    server: Weak<RunningEmbeddingServer>,
    handle: Weak<SidecarHandle>,
}

impl CurrentEmbeddingServer {
    fn record(
        &self,
        name: String,
        server: &Arc<RunningEmbeddingServer>,
        handle: &Arc<SidecarHandle>,
    ) {
        *self.current.lock() = Some(Current {
            name,
            server: Arc::downgrade(server),
            handle: Arc::downgrade(handle),
        });
    }

    fn running(&self) -> Option<(String, Arc<RunningEmbeddingServer>, Arc<SidecarHandle>)> {
        let current = self.current.lock();
        let current = current.as_ref()?;
        let handle = current
            .handle
            .upgrade()
            .filter(|handle| handle.is_running())?;
        Some((current.name.clone(), current.server.upgrade()?, handle))
    }
}

impl IdleServer for CurrentEmbeddingServer {
    fn name(&self) -> String {
        self.running()
            .map_or_else(|| "embedding server".to_owned(), |(name, ..)| name)
    }

    fn idle_gpu_bytes(&self) -> Option<u64> {
        let (_, server, _) = self.running()?;
        if server.in_flight() > 0 {
            return None;
        }
        server.gpu_bytes().filter(|bytes| *bytes > 0)
    }

    fn stop(&self) {
        if let Some((name, _, handle)) = self.running() {
            tracing::info!(model = %name, "Stopping the idle embedding server to make room");
            handle.stop();
        }
    }
}

/// Starts embedding servers on the bundled llama-server.
pub(crate) struct SidecarEmbeddingStarter {
    /// `None` where no server can start: tests, and tools that build a
    /// container without the app.
    app: Option<tauri::AppHandle>,
    instances: Arc<LlmInstances>,
    runtime: LlmRuntime,
    current: Arc<CurrentEmbeddingServer>,
}

impl SidecarEmbeddingStarter {
    pub(crate) fn new(
        app: Option<tauri::AppHandle>,
        instances: Arc<LlmInstances>,
        runtime: LlmRuntime,
        current: Arc<CurrentEmbeddingServer>,
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

/// The sidecar configuration of the server `launch` describes: its window
/// and slots, every layer offloaded or none, and nothing a chat server sets.
fn sidecar_config(launch: &EmbeddingLaunch) -> SidecarConfig {
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
        embedding: Some(EmbeddingServing {
            ubatch: launch.window_tokens,
        }),
    }
}

#[async_trait]
impl EmbeddingServerStarter for SidecarEmbeddingStarter {
    async fn header(&self, gguf: &Path) -> Option<EmbeddingHeader> {
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

    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningEmbeddingServer>> {
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
        let server = Arc::new(RunningEmbeddingServer::new(
            handle.endpoint(),
            handle.api_token(),
            gpu_bytes,
            Arc::new(move || watched.is_running()),
        ));
        self.current.record(name, &server, &handle);
        Ok(server)
    }
}
