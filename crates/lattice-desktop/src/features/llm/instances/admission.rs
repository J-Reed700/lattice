//! What a new bundled server asks of the GPU memory the others leave.

use std::path::Path;

use async_trait::async_trait;

use crate::features::llm::engine::factory::{launch_window, sidecar_config, LocalServerOptions};
use crate::features::llm::engine::gguf_metadata::read_model_info;
use crate::features::llm::engine::sidecar_manager::{
    memory_hint, Admission, SidecarConfig, SidecarRegistry,
};
use crate::features::llm::engine::system::{
    detect_capabilities_with_app, BackendDevices, SystemCapabilities,
};
use crate::features::llm::loading::LlmRuntime;

/// A bundled server outside the pool of loaded LLMs that admission may stop
/// to make room, while it is idle: the embedding server. It is started again
/// by the next request that needs it.
pub(crate) trait IdleServer: Send + Sync {
    /// What the logs call it.
    fn name(&self) -> String;
    /// The GPU memory stopping it would free, while it runs on the GPU with
    /// nothing in flight; `None` while it is busy, stopped, or on the CPU.
    fn idle_gpu_bytes(&self) -> Option<u64>;
    /// Stop it now.
    fn stop(&self);
}

/// What starting a bundled server would ask of the machine's memory. Only a
/// hint: no load is ever refused on it.
#[async_trait]
pub(crate) trait GpuBudget: Send + Sync {
    /// Whether a server for `path` launched with `launch` fits now. It only
    /// decides whether idle models are unloaded to make room.
    async fn admission(&self, path: &Path, launch: LocalServerOptions) -> Admission;

    /// Whether a server started for `config` fits now: what [`Self::admission`]
    /// answers, for a server whose configuration is already decided, such as
    /// the embedding server.
    async fn server_admission(&self, config: &SidecarConfig) -> Admission;

    /// The largest window, up to the one it was trained for, at which `path`
    /// fits this machine's memory beside the models loaded now, its KV cache
    /// quantized when `kv_cache_quantization`. `None` when the platform does
    /// not report its memory or the header cannot say.
    async fn fits_window(&self, path: &Path, kv_cache_quantization: bool) -> Option<u32>;

    /// The window `path` was trained for, which it launches with unless the
    /// user saved another; `None` when its header does not say.
    fn trained_window(&self, path: &Path) -> Option<u32>;
}

/// The budget of the app's bundled servers: what the platform reports for
/// the device they run on, less what the running servers and the app's own
/// models hold, by the arithmetic of the memory hint.
pub(crate) struct SidecarBudget {
    app: Option<tauri::AppHandle>,
    runtime: LlmRuntime,
}

impl SidecarBudget {
    /// `app` is `None` where no server can start (tests), which leaves
    /// nothing to check.
    pub(crate) fn new(app: Option<tauri::AppHandle>, runtime: LlmRuntime) -> Self {
        Self { app, runtime }
    }

    fn devices(&self) -> &BackendDevices {
        &self.runtime.devices
    }

    /// The configuration the factory would start `path` with now, and the
    /// machine it is priced on, or `None` when no servers are tracked or the
    /// window is unknown.
    async fn plan(
        &self,
        path: &Path,
        launch: LocalServerOptions,
    ) -> Option<(
        SidecarConfig,
        SystemCapabilities,
        tauri::State<'_, SidecarRegistry>,
    )> {
        use tauri::Manager;
        let app = self.app.as_ref()?;
        let registry = app.try_state::<SidecarRegistry>()?;
        let capabilities = detect_capabilities_with_app(app, self.devices()).await;
        let info = read_model_info(path);
        let window = launch_window(launch, info.as_ref())?;
        let config = sidecar_config(path, -1, window, launch, &capabilities, info.as_ref());
        Some((config, capabilities, registry))
    }
}

#[async_trait]
impl GpuBudget for SidecarBudget {
    async fn admission(&self, path: &Path, launch: LocalServerOptions) -> Admission {
        let Some((config, capabilities, registry)) = self.plan(path, launch).await else {
            return Admission::Unmetered;
        };
        registry.admission(&config, &capabilities)
    }

    async fn server_admission(&self, config: &SidecarConfig) -> Admission {
        use tauri::Manager;
        let Some(app) = self.app.as_ref() else {
            return Admission::Unmetered;
        };
        let Some(registry) = app.try_state::<SidecarRegistry>() else {
            return Admission::Unmetered;
        };
        let capabilities = detect_capabilities_with_app(app, self.devices()).await;
        registry.admission(config, &capabilities)
    }

    async fn fits_window(&self, path: &Path, kv_cache_quantization: bool) -> Option<u32> {
        let launch = LocalServerOptions {
            kv_cache_quantization,
            ..LocalServerOptions::default()
        };
        let (config, capabilities, registry) = self.plan(path, launch).await?;
        let held = registry.other_models_gpu_bytes(path);
        memory_hint(&config, &capabilities, held)?.fits_window
    }

    fn trained_window(&self, path: &Path) -> Option<u32> {
        read_model_info(path).and_then(|info| info.trained_context_length)
    }
}

/// Log an admission decision for `model` as `model.admission`, with the
/// idle models evicted to reach it.
pub(super) fn log(model: &str, outcome: Admission, evicted: &[(String, u64)]) {
    match outcome {
        Admission::Fits { needed, spare } | Admission::Short { needed, spare } => {
            tracing::info!(
                model,
                needed_bytes = needed.total(),
                spare_bytes = spare,
                window = needed.window,
                ?evicted,
                outcome = if matches!(outcome, Admission::Fits { .. }) {
                    "fits"
                } else {
                    "short; starting anyway"
                },
                "model.admission"
            );
        }
        Admission::Shared => {
            tracing::debug!(model, outcome = "shared", "model.admission");
        }
        Admission::NoOffload => {
            tracing::debug!(model, outcome = "cpu", "model.admission");
        }
        Admission::Unmetered => {
            tracing::info!(
                model,
                outcome = "skipped",
                reason = "the GPU does not report its memory",
                "model.admission"
            );
        }
    }
}
