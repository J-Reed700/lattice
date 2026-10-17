//! Whether a new bundled server fits in the GPU memory the others leave.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use crate::features::llm::engine::factory::{sidecar_config, LocalServerOptions};
use crate::features::llm::engine::gguf_metadata::read_model_info;
use crate::features::llm::engine::sidecar_manager::{Admission, SidecarConfig, SidecarRegistry};
use crate::features::llm::engine::system::{
    detect_capabilities_with_app, BackendDevices, SystemCapabilities,
};

/// What starting a bundled server would ask of the GPU.
#[async_trait]
pub(crate) trait GpuBudget: Send + Sync {
    /// Whether a server for `path` launched with `launch` fits now, its cache
    /// priced at the larger of the window it would get and `floor`.
    async fn admission(
        &self,
        path: &Path,
        launch: LocalServerOptions,
        floor: Option<u32>,
    ) -> Admission;

    /// The window Auto would give `path` on this machine now, or `None` when
    /// the GPU does not report its memory.
    async fn auto_window(&self, path: &Path, launch: LocalServerOptions) -> Option<u32>;

    /// The window `path` was trained for, which Auto never goes past however
    /// much memory is free; `None` when its header does not say.
    fn trained_window(&self, path: &Path) -> Option<u32>;
}

/// The budget of the app's bundled servers: the GPU's reported memory less
/// what the running servers hold, by the same arithmetic their windows are
/// sized with.
pub(crate) struct SidecarBudget {
    app: Option<tauri::AppHandle>,
    devices: Arc<BackendDevices>,
}

impl SidecarBudget {
    /// `app` is `None` where no server can start (tests), which leaves
    /// nothing to check.
    pub(crate) fn new(app: Option<tauri::AppHandle>, devices: Arc<BackendDevices>) -> Self {
        Self { app, devices }
    }

    /// The configuration the factory would start `path` with now, and the
    /// machine it was sized for, or `None` when no servers are tracked.
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
        let capabilities = detect_capabilities_with_app(app, &self.devices).await;
        let config = sidecar_config(
            path,
            -1,
            launch,
            &capabilities,
            registry.other_models_gpu_bytes(path),
            read_model_info(path).as_ref(),
        );
        Some((config, capabilities, registry))
    }
}

#[async_trait]
impl GpuBudget for SidecarBudget {
    async fn admission(
        &self,
        path: &Path,
        launch: LocalServerOptions,
        floor: Option<u32>,
    ) -> Admission {
        let Some((config, capabilities, registry)) = self.plan(path, launch).await else {
            return Admission::Unmetered;
        };
        let window = floor.map_or(config.context_size, |floor| floor.max(config.context_size));
        registry.admission(&config, window, &capabilities)
    }

    async fn auto_window(&self, path: &Path, launch: LocalServerOptions) -> Option<u32> {
        let auto = LocalServerOptions {
            context_window: None,
            ..launch
        };
        let (config, capabilities, _) = self.plan(path, auto).await?;
        capabilities.gpu_vram_gb().map(|_| config.context_size)
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
                    "refused"
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
