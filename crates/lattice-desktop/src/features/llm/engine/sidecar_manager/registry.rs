use super::*;
use crate::features::llm::engine::gguf_metadata::read_model_info;
use lattice_core::domain::models::fit::Footprint;

/// Process-wide registry of live sidecars, held as Tauri-managed state so the
/// `RunEvent::ExitRequested` handler can drain it synchronously before the
/// tokio runtime tears down. Each entry holds a strong reference to a child,
/// so one whose only other owner is a cancelled startup future is reachable.
///
/// On every OS the kernel ends a sidecar when Lattice dies, however it dies:
/// `BoundCommand` binds each child to this process. `Drop` on a handle and the
/// exit sweep are the orderly path. The mutex is `parking_lot`, not tokio's,
/// because the shutdown consumer is a synchronous callback where `block_on`
/// can deadlock.
pub struct SidecarRegistry {
    pub(super) entries: SyncMutex<RegistryState>,

    /// Live servers keyed by the configuration that started them, so roles
    /// pointing at one GGUF share a process. Orthogonal to `entries`, the kill
    /// list: a shared server holds one `entries` slot however many roles use it.
    pub(super) shared: SharedProcesses<SidecarConfig, SidecarHandle>,

    /// Ports reserved for this app's sidecars, so two concurrent spawns are
    /// never handed the same number.
    pub(super) ports: Arc<ReservedPorts>,

    /// What `--version` said about each bundled build, so a model load does
    /// not run the preflight again within its TTL.
    pub(super) preflight: PreflightCache,

    /// The id the next registered child gets; ids only need to be unique
    /// within this registry, which is where their owners release them.
    next_id: AtomicU64,
}

pub(super) struct RegistryState {
    pub(super) entries: Vec<RegistryEntry>,
    pub(super) closed: bool,
}

pub(super) struct RegistryEntry {
    /// Identifies the slot so its owner can release it, even when its only strong
    /// reference is a cancelled startup future.
    pub(super) id: u64,
    pub(super) child: Arc<SyncMutex<Option<ServerProcess>>>,
    pub(super) endpoint: String,
}

impl SidecarRegistry {
    pub fn new() -> Self {
        Self {
            entries: SyncMutex::new(RegistryState {
                entries: Vec::new(),
                closed: false,
            }),
            shared: SharedProcesses::new(),
            ports: Arc::default(),
            preflight: PreflightCache::new(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a live sidecar. Called by `SidecarManager::start_attempt`
    /// after readiness confirmation. Cleans up dead weak refs from
    /// previous spawns at the same time.
    pub(super) fn register(
        &self,
        child: &Arc<SyncMutex<Option<ServerProcess>>>,
        endpoint: &str,
    ) -> Option<u64> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let admitted = self.with_open_entries(|entries| {
            // Admission and insertion share the state lock with `kill_all`.
            entries.retain(|entry| entry.child.lock().is_some());
            entries.push(RegistryEntry {
                id,
                child: Arc::clone(child),
                endpoint: endpoint.to_string(),
            });
            tracing::debug!(
                registry_size = entries.len(),
                endpoint = endpoint,
                "Registered sidecar in process registry"
            );
            id
        });
        let Some(id) = admitted else {
            if let Some(child) = child.lock().take() {
                child.kill();
            }
            return None;
        };
        Some(id)
    }

    pub(super) fn with_open_entries<T>(
        &self,
        register: impl FnOnce(&mut Vec<RegistryEntry>) -> T,
    ) -> Option<T> {
        let mut state = self.entries.lock();
        if state.closed {
            return None;
        }
        Some(register(&mut state.entries))
    }

    /// Release the slot `id` owns. Called by whoever owns the child — the
    /// handle, or the spawn guard on a cancelled startup.
    pub(super) fn unregister(&self, id: u64) {
        self.entries.lock().entries.retain(|e| e.id != id);
    }

    /// Ask every registered sidecar to die, without waiting. Idempotent — call
    /// from the Tauri RunEvent::ExitRequested handler. Returns the
    /// number of children asked to die (for logging).
    pub fn kill_all(&self) -> usize {
        // Drain into a local Vec, then release entries lock immediately.
        let drained: Vec<RegistryEntry> = {
            let mut state = self.entries.lock();
            state.closed = true;
            std::mem::take(&mut state.entries)
        };

        let mut killed = 0;
        for entry in drained {
            // Lock the per-child mutex briefly: take the ServerProcess
            // out, then release before calling kill so a concurrent
            // SidecarHandle::stop sees an empty slot and no-ops.
            let child_opt = entry.child.lock().take();
            if let Some(child) = child_opt {
                child.kill();
                killed += 1;
                tracing::info!(
                    endpoint = %entry.endpoint,
                    "Killed sidecar via registry shutdown"
                );
            }
        }
        killed
    }

    /// GPU memory held by running, GPU-offloaded servers of other models:
    /// what the memory hint for `model_path` counts as already taken. A
    /// server of the same GGUF is left out, because a role asking for it is
    /// handed that server rather than a second one.
    pub fn other_models_gpu_bytes(&self, model_path: &std::path::Path) -> u64 {
        gpu_bytes_where(&self.running_servers(), |running| {
            running.model_path != model_path
        })
    }

    /// GPU memory held by running, GPU-offloaded servers other than the one
    /// started for `config`. Another window of the same GGUF counts: it is a
    /// second process with its own copy of the weights and its own cache.
    pub fn resident_gpu_bytes(&self, config: &SidecarConfig) -> u64 {
        gpu_bytes_where(&self.running_servers(), |running| running != config)
    }

    /// The configuration of the running server a role asking for `config` is
    /// handed: the one that differs from it in nothing but its fitted windows.
    /// Fitting again reads available RAM live, and other models load and
    /// unload, so it can pick another window: a new key, and a second copy of
    /// the model.
    pub fn running_alike(&self, config: &SidecarConfig) -> Option<SidecarConfig> {
        running_alike(&self.running_servers(), config)
    }

    /// Whether a server for `config` fits beside the servers running now. See
    /// [`admission`].
    pub fn admission(
        &self,
        config: &SidecarConfig,
        capabilities: &SystemCapabilities,
    ) -> Admission {
        admission(&self.running_servers(), config, capabilities)
    }

    fn running_servers(&self) -> Vec<RunningServer> {
        self.shared
            .running()
            .into_iter()
            .map(|(config, handle)| RunningServer {
                n_gpu_layers: handle.n_gpu_layers(),
                context_size: handle.context_size(),
                config,
            })
            .collect()
    }

    /// Number of currently-tracked live entries. For diagnostics.
    pub fn live_count(&self) -> usize {
        let entries = self.entries.lock();
        entries
            .entries
            .iter()
            .filter(|e| e.child.lock().is_some())
            .count()
    }
}

/// A running server as GPU arithmetic sees it: started for `config`, it may
/// have come up with fewer layers or a smaller window than that asked for.
pub(super) struct RunningServer {
    pub(super) config: SidecarConfig,
    pub(super) n_gpu_layers: u32,
    pub(super) context_size: u32,
}

/// The configuration among `servers` that is `config` but for its windows,
/// when the user saved neither's.
pub(super) fn running_alike(
    servers: &[RunningServer],
    config: &SidecarConfig,
) -> Option<SidecarConfig> {
    if !config.auto_window {
        return None;
    }
    servers
        .iter()
        .map(|server| &server.config)
        .filter(|running| running.auto_window)
        .find(|running| {
            let windows = SidecarConfig {
                context_size: running.context_size,
                cpu_context_size: running.cpu_context_size,
                ..config.clone()
            };
            windows == **running
        })
        .cloned()
}

/// GPU memory held by the GPU-offloaded servers among `servers` whose
/// configuration is `counted`.
pub(super) fn gpu_bytes_where(
    servers: &[RunningServer],
    counted: impl Fn(&SidecarConfig) -> bool,
) -> u64 {
    servers
        .iter()
        .filter(|server| {
            counted(&server.config) && server.n_gpu_layers > 0 && server.config.on_offload_gpu()
        })
        .map(|server| {
            resident_bytes_of(
                &server.config.model_path,
                server.context_size,
                server.config.buffers(),
            )
        })
        .fold(0, u64::saturating_add)
}

/// Whether a server for `config` fits beside `servers`, by the memory hint's
/// arithmetic ([`memory_hint`]). The embedding model is one of `servers`.
///
/// A chat model that does not fit unloads idle models to make room and still
/// starts, at the window the user chose; if it will not allocate, the start
/// fails with llama-server's own error and these numbers beside it. A pooling
/// server goes where its claim seats it (`LlmInstances::claim_gpu`).
pub(super) fn admission(
    servers: &[RunningServer],
    config: &SidecarConfig,
    capabilities: &SystemCapabilities,
) -> Admission {
    if servers.iter().any(|server| server.config == *config) {
        return Admission::Shared;
    }
    if !config.on_offload_gpu() {
        return Admission::NoOffload;
    }
    let held = gpu_bytes_where(servers, |running| running != config);
    let info = read_model_info(&config.model_path);
    let Some(hint) = memory_hint(config, capabilities, held, info.as_ref()) else {
        return Admission::Unmetered;
    };
    let (needed, spare) = (hint.needed, hint.room.spare());
    if hint.fits() {
        Admission::Fits { needed, spare }
    } else if hint.fits_in_margin() {
        Admission::Tight { needed, spare }
    } else {
        Admission::Short { needed, spare }
    }
}

/// What starting a server would ask of the GPU. Nothing here stops a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// A server for exactly this configuration is running and is shared;
    /// nothing new is loaded.
    Shared,
    /// It would run on the CPU, which asks nothing of the GPU.
    NoOffload,
    /// The GPU does not report its memory, so nothing can be checked.
    Unmetered,
    /// It fits: `needed` of the `spare` bytes the running servers leave.
    Fits { needed: Footprint, spare: u64 },
    /// It fits only in the margin kept free beyond the `spare` bytes.
    Tight { needed: Footprint, spare: u64 },
    /// It does not fit, margin and all.
    Short { needed: Footprint, spare: u64 },
}

impl Admission {
    /// Bytes short of the `spare` room, when it comes up short of that.
    pub fn shortfall(self) -> Option<u64> {
        match self {
            Self::Tight { needed, spare } | Self::Short { needed, spare } => {
                Some(needed.total().saturating_sub(spare))
            }
            Self::Shared | Self::NoOffload | Self::Unmetered | Self::Fits { .. } => None,
        }
    }
}

/// Shutdown's sidecar sweep: kills every child in the app's registry and
/// returns how many, or `None` when no registry is managed.
///
/// The composition root hands this to shutdown as its `SidecarSweep`, so the
/// shutdown path never has to name this feature.
pub fn kill_registered_sidecars(app: &AppHandle) -> Option<usize> {
    app.try_state::<SidecarRegistry>()
        .map(|registry| registry.kill_all())
}

impl Default for SidecarRegistry {
    fn default() -> Self {
        Self::new()
    }
}
