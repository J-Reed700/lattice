//! Sidecar process manager for the bundled `llama-server` binary.
//!
//! Owns the lifecycle of the local-LLM inference engine: port allocation,
//! spawn, readiness detection, kill-on-drop. Built behind a clean
//! HTTP boundary so the rest of the app sees only an `endpoint: String`
//! and never deals with process internals.
//!
//! # Why a sidecar
//!
//! Lattice uses the bundled `llama-server` binary rather than an in-process
//! inference runtime. The
//! binary speaks an OpenAI-compatible HTTP/SSE API on `127.0.0.1:<port>`.
//! This module spawns it; `SidecarLLMClient` is its HTTP client.
//!
//! # Lifecycle
//!
//! 1. `SidecarManager::start_with_fallback(app, config)`
//!    - Preflights the bundled binary once per process (`--version`), so
//!      an executable that cannot run fails in milliseconds with a
//!      `LLMError::SidecarBinaryUnusable` instead of after a readiness wait.
//!    - Picks a free ephemeral port via `TcpListener::bind("127.0.0.1:0")`.
//!    - Spawns `llama-server` via `tauri-plugin-shell` (the bundled
//!      sidecar binary, not a system-installed one).
//!    - Waits for readiness: `GET /health` answering 200 is the
//!      authoritative signal, with the `"server is listening on"` log
//!      line as a fast path. The wait bounds silence, not duration, so a
//!      large model on a cold page cache is not killed for being slow.
//!      A bounded tail of the output is kept for the error report.
//!    - On failure, walks the fallback policy (`next_attempt`): GPU off,
//!      then the CPU-only build on Windows/Linux.
//!    - Returns a handle whose `endpoint()` is ready to receive requests.
//!
//! 2. `SidecarManager::stop()` (or `Drop`)
//!    - Sends `kill()` to the child process. Tauri-plugin-shell's
//!      `CommandChild` uses `shared_child::SharedChild` which calls
//!      the OS kill primitive (TerminateProcess on Windows, SIGKILL
//!      on Unix), with Windows Job Objects providing process-tree cleanup.
//!
//! # What this module does NOT do
//!
//! - It does not implement the `LLMClient` trait; `SidecarLLMClient` does.
//! - It does not download model files — that's the existing model
//!   storage layer, untouched by this migration.

use crate::features::llm::engine::system::SystemCapabilities;
use crate::features::llm::engine::types::LLMError;
use parking_lot::Mutex as SyncMutex;
use std::collections::{BTreeSet, VecDeque};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

// Sidecar names are INSTALLED file names, not `externalBin` source paths.
//
// `tauri*.conf.json > bundle > externalBin` lists source paths
// (`binaries/llama-server`, resolved to `binaries/llama-server-<triple>`).
// tauri-build (dev) and the bundler (release) copy each one next to the app
// executable under its bare file stem, triple stripped: `<exe dir>/llama-server`.
// `tauri-plugin-shell`'s `sidecar(name)` looks for `<exe dir>/<name>` (plus
// `.exe` on Windows), so the name passed to it must be that stem. A test
// below pins these to the conf files.

/// The primary llama-server build (Metal on macOS, Vulkan elsewhere).
pub const SIDECAR_BIN: &str = "llama-server";
/// CPU-only llama-server build. Bundled on Windows and Linux only, where
/// the primary build is Vulkan and cannot even start without a Vulkan
/// loader (`vulkan-1.dll` / `libvulkan.so.1`). macOS ships one Metal
/// build. Always reached through [`SidecarBinary::Cpu`].
pub const SIDECAR_CPU_BIN: &str = "llama-server-cpu";
/// llama-server's readiness line, as the pinned build prints it on stderr:
/// `main: server is listening on http://127.0.0.1:<port>`. The pre-b4000
/// string was `HTTP server listening`, which appears nowhere in the shipped
/// binaries — the wait never matched and every model load timed out. It is a
/// fast path only: `/health` decides, so the next upstream rewording costs a
/// second of startup rather than the whole feature. `ready_needle_is_present`
/// pins it to the bundled binaries.
const READY_NEEDLE: &str = "server is listening on";

/// How long the sidecar may go completely silent — no output line, no
/// `/health` answer — before an attempt is abandoned. Bounding silence
/// instead of total duration is what lets a 20 GB model take the minutes it
/// needs to come off a cold page cache without being killed as "corrupt".
const READINESS_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Absolute bound for one attempt, however talkative it stays.
const READINESS_ATTEMPT_CAP: Duration = Duration::from_secs(600);

/// One budget shared by every attempt of a single start, so the fallback
/// ladder bounds the worst case once rather than once per rung.
const STARTUP_TOTAL_BUDGET: Duration = Duration::from_secs(900);

/// How often readiness polls `/health` while waiting.
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Same-configuration retries for a failure that a different configuration
/// cannot fix (a port taken between allocation and llama-server's own bind).
const MAX_SAME_CONFIG_RETRIES: u32 = 3;

/// How long a preflight verdict is trusted before `--version` runs again.
const PREFLIGHT_CACHE_TTL: Duration = Duration::from_secs(600);

/// A healthy `--version` returns in well under a second (it initializes
/// the GPU backend and exits). The bound only matters for a binary that
/// hangs, which is then treated as inconclusive rather than broken.
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(20);

/// Startup output kept for error reports: llama.cpp's backend and
/// model-loader preamble plus whatever it printed when it failed.
const OUTPUT_TAIL_MAX_LINES: usize = 40;
const OUTPUT_TAIL_MAX_BYTES: usize = 8 * 1024;
/// A single line is clipped so one runaway line cannot evict the rest.
const OUTPUT_LINE_MAX_BYTES: usize = 2 * 1024;
/// How much of that tail an error message carries. Errors reach the UI,
/// so they get the end of the output, not all of it.
const ERROR_TAIL_LINES: usize = 12;
const ERROR_LINE_MAX_BYTES: usize = 400;

/// Why a bundled build cannot run, at the granularity the user can act on.
/// A missing Vulkan runtime and a truncated download both used to be reported
/// as "reinstall Lattice", which fixes only one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnusableReason {
    /// The executable loaded nothing because the machine has no GPU runtime
    /// (`vulkan-1.dll`, `libvulkan.so.1`). Expected on plenty of machines;
    /// the CPU build is the answer, not a reinstall.
    MissingGpuRuntime,
    /// The file itself is wrong: missing dylib, bad architecture, truncated.
    BrokenInstall,
    /// The CPU lacks an instruction set this build was compiled for.
    UnsupportedCpu,
}

impl UnusableReason {
    fn advice(self) -> &'static str {
        match self {
            Self::MissingGpuRuntime => {
                "Update your graphics driver to get a Vulkan runtime; until then Lattice runs \
                 local models on the CPU."
            }
            Self::BrokenInstall => "Reinstall Lattice.",
            Self::UnsupportedCpu => {
                "This CPU is missing an instruction set that build needs, so Lattice uses its \
                 compatibility build instead."
            }
        }
    }

    /// Ranks how useful this is as *the* reported error. A missing GPU runtime
    /// on the accelerated build is the least actionable: it is expected, and
    /// when a later attempt fails for its own reason that reason is what the
    /// user needs to read.
    fn report_rank(self) -> u8 {
        match self {
            Self::MissingGpuRuntime => 1,
            Self::BrokenInstall | Self::UnsupportedCpu => 3,
        }
    }
}

/// A bundled llama-server build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarBinary {
    /// The accelerated build: Metal on macOS, Vulkan on Windows and Linux.
    Primary,
    /// The CPU-only build, for machines where the primary build cannot load
    /// or cannot bring up its GPU backend. Windows and Linux only.
    Cpu,
}

impl SidecarBinary {
    /// Whether this platform bundles [`SidecarBinary::Cpu`].
    pub const CPU_BUILD_BUNDLED: bool = cfg!(any(target_os = "windows", target_os = "linux"));

    /// Installed sidecar name: what `sidecar()` spawns, and what logs,
    /// diagnostics and error messages call this build.
    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => SIDECAR_BIN,
            Self::Cpu => SIDECAR_CPU_BIN,
        }
    }

    fn bundled() -> &'static [SidecarBinary] {
        if Self::CPU_BUILD_BUNDLED {
            &[Self::Primary, Self::Cpu]
        } else {
            &[Self::Primary]
        }
    }
}

/// Configuration for spawning a llama-server sidecar.
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    /// Path to the GGUF model file.
    pub model_path: PathBuf,

    /// Number of model layers to offload to GPU. `99` means "all";
    /// `0` means CPU-only. Maps to llama-server's `-ngl` flag.
    pub n_gpu_layers: u32,

    /// Context window in tokens. Maps to `--ctx-size`. Default is
    /// constrained by the model's training context; we cap at 8192
    /// to avoid OOM on entry-level hardware.
    pub context_size: u32,
}

/// RAM threshold (GB) below which we reduce the context window to keep
/// memory pressure manageable. 8 GB is the entry point for "comfortable
/// 7B Q4_K_M with 8K context"; below that we squeeze ctx down.
const LOW_RAM_THRESHOLD_GB: f64 = 8.0;

/// Context size for low-RAM machines. 2048 tokens is enough for most
/// chat turns and plays well with 4 GB systems running 3B/4B models.
const LOW_RAM_CONTEXT_SIZE: u32 = 2048;

/// Default context for accelerated configurations. Most modern open
/// models (Llama 3, Qwen 2.5) train at >=8K natively; pushing past
/// that on consumer hardware is risky.
const DEFAULT_GPU_CONTEXT_SIZE: u32 = 8192;

/// Default context for CPU-only configurations. CPU prefill scales
/// with context-squared, so 4K is a more honest UX target than 8K.
const DEFAULT_CPU_CONTEXT_SIZE: u32 = 4096;

impl SidecarConfig {
    /// A sensible default: GPU-offload everything, 8K context.
    /// Use `from_capabilities()` instead in production code paths so
    /// the config matches the user's hardware.
    pub fn for_model(model_path: PathBuf) -> Self {
        Self {
            model_path,
            n_gpu_layers: 99,
            context_size: DEFAULT_GPU_CONTEXT_SIZE,
        }
    }

    /// The same model with GPU offload disabled, for fallback attempts.
    /// Caps the context at the CPU default but keeps a smaller low-RAM
    /// window rather than growing it.
    pub fn without_gpu_offload(&self) -> Self {
        Self {
            model_path: self.model_path.clone(),
            n_gpu_layers: 0,
            context_size: self.context_size.min(DEFAULT_CPU_CONTEXT_SIZE),
        }
    }

    pub fn from_capabilities(model_path: PathBuf, capabilities: &SystemCapabilities) -> Self {
        let has_accelerator = capabilities
            .gpu
            .as_ref()
            .map(|gpu| gpu.vendor.is_accelerated())
            .unwrap_or(false);

        let n_gpu_layers = if has_accelerator { 99 } else { 0 };

        let base_context = if has_accelerator {
            DEFAULT_GPU_CONTEXT_SIZE
        } else {
            DEFAULT_CPU_CONTEXT_SIZE
        };

        let context_size = if capabilities.total_ram_gb < LOW_RAM_THRESHOLD_GB {
            tracing::info!(
                total_ram_gb = capabilities.total_ram_gb,
                "Low-RAM system detected; reducing sidecar context window to {}",
                LOW_RAM_CONTEXT_SIZE
            );
            LOW_RAM_CONTEXT_SIZE
        } else {
            base_context
        };

        Self {
            model_path,
            n_gpu_layers,
            context_size,
        }
    }
}

/// Live sidecar process handle.
///
/// Holds the OS process and the channel of events from it. Dropping
/// the handle terminates the child via `kill()`. The inner mutex uses
/// `parking_lot::Mutex` so the registry's synchronous kill_all path
/// can lock without an async runtime.
pub struct SidecarHandle {
    /// `127.0.0.1:<port>` — the HTTP base URL the LLM client posts to.
    endpoint: String,

    /// Bearer token this server was launched with. Loopback is not a trust
    /// boundary: without it any local process — including a page in the user's
    /// browser, which can reach 127.0.0.1 — could generate on the user's GPU
    /// and read back the model path. Never logged.
    api_token: String,

    /// Process handle. Sync `Mutex` because `CommandChild::kill()` is
    /// itself synchronous (a syscall, no `.await`) and the shutdown
    /// path runs from a sync Tauri callback. `Option` so the kill
    /// owner takes the child exactly once.
    child: Arc<SyncMutex<Option<CommandChild>>>,

    /// The `--ctx-size` this server was actually launched with.
    ///
    /// Not a constant: it is 8192 on GPU machines, 4096 CPU-only, and 2048
    /// under the low-RAM threshold. Callers that budget a prompt must use
    /// this value — reporting a fixed 8192 upstream meant every budget
    /// overshot the real window by 2–4x on smaller machines, so llama-server
    /// silently truncated the prompt server-side and the system prompt and
    /// oldest history simply vanished.
    context_size: u32,

    /// The build that actually started, after any fallback.
    binary: SidecarBinary,

    /// The `-ngl` this server was actually launched with, after any fallback.
    n_gpu_layers: u32,

    /// The registry slot this handle owns, released when it dies.
    registration: Option<Registration>,

    /// Held for the life of the process so no other spawn is handed this port.
    _port: PortReservation,

    /// Set when the server that finally ran is not the one that was asked
    /// for — a GPU machine running CPU-only, or a halved context. Callers
    /// log it; without it a degradation is invisible outside a warn line.
    degraded: Option<String>,
}

/// A handle's slot in the process registry. The registry lives in Tauri
/// managed state, so releasing the slot means looking it up again.
struct Registration {
    app: AppHandle,
    id: u64,
}

impl Registration {
    /// Never called while a child mutex is held: `register` locks the registry
    /// and then a child, so the reverse order would be a lock cycle.
    fn release(&self) {
        if let Some(registry) = self.app.try_state::<SidecarRegistry>() {
            registry.unregister(self.id);
        }
    }
}

impl SidecarHandle {
    /// Base URL of the local llama-server HTTP API, e.g.
    /// `http://127.0.0.1:53412`. Pass to `SidecarLLMClient`.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Bearer token every request to this sidecar must carry, `/health`
    /// included. Treat as a secret: it must not reach a log or an error body.
    pub fn api_token(&self) -> &str {
        &self.api_token
    }

    /// Context window, in tokens, that this sidecar was launched with.
    pub fn context_size(&self) -> u32 {
        self.context_size
    }

    /// Which bundled build is serving this handle.
    pub fn binary(&self) -> SidecarBinary {
        self.binary
    }

    /// GPU layers offloaded by this server; `0` means it runs CPU-only.
    pub fn n_gpu_layers(&self) -> u32 {
        self.n_gpu_layers
    }

    /// How this server differs from the configuration that was requested,
    /// if it does. `None` means the request was honoured exactly.
    pub fn degraded(&self) -> Option<&str> {
        self.degraded.as_deref()
    }

    /// Explicitly kill the sidecar. After this returns, the handle is
    /// inert; `Drop` becomes a no-op. Idempotent — second call is fine.
    pub fn stop(&self) {
        let child = self.child.lock().take();
        if let Some(registration) = self.registration.as_ref() {
            registration.release();
        }
        if let Some(child) = child {
            if let Err(err) = child.kill() {
                tracing::warn!("Failed to kill llama-server sidecar: {err}");
            } else {
                tracing::info!("Stopped llama-server sidecar at {}", self.endpoint);
            }
        }
    }
}

impl Drop for SidecarHandle {
    fn drop(&mut self) {
        // Best-effort synchronous kill. The registry is the primary cleanup
        // mechanism (driven off
        // `RunEvent::ExitRequested`, synchronous, runs before the
        // tokio runtime stops). This Drop remains as a safety net for
        // handles dropped outside app shutdown — e.g. when the user
        // swaps active models and the old SidecarLLMClient goes out
        // of scope mid-session. No holder awaits while holding the child
        // mutex, so a plain lock cannot stall here; `try_lock` used to
        // drop the kill silently whenever it lost the race.
        self.stop();
        tracing::debug!(
            "SidecarHandle dropped; sidecar at {} stopped",
            self.endpoint
        );
    }
}

/// A spawned sidecar that kills itself if it is dropped before it becomes a
/// [`SidecarHandle`].
///
/// Cancellation is why this type exists. Between `spawn()` and the handle, the
/// only owner of a multi-gigabyte llama-server is the future doing the
/// readiness wait, and `CommandChild::Drop` does not kill the OS process. A
/// future aborted while parked in `await_ready` — a cancelled model switch, a
/// superseded load, a runtime teardown that bypasses the shutdown sweep —
/// therefore left a full-weight server holding its RAM, its VRAM and its port
/// with no owner and nothing in the log. Arming a guard at spawn makes
/// cancellation safety a property of the type instead of of every error path.
struct SpawnedChild {
    child: Arc<SyncMutex<Option<CommandChild>>>,
    registration: Option<Registration>,
    endpoint: String,
    armed: bool,
}

impl SpawnedChild {
    /// Spawn `binary` and register it. Registration happens before readiness
    /// so the shutdown sweep can find a child that never became ready.
    fn spawn(
        app: &AppHandle,
        binary: SidecarBinary,
        args: Vec<String>,
        endpoint: &str,
        api_token: &str,
    ) -> Result<(Receiver<CommandEvent>, Self), AttemptError> {
        // The key goes in the environment, not `--api-key`: argv is world-
        // readable through `ps` to every process on the machine, which is the
        // same set of processes the key exists to keep off the port.
        let (rx, child) = app
            .shell()
            .sidecar(binary.label())
            .map_err(|err| spawn_failure(binary, &err))?
            .args(args)
            .env("LLAMA_API_KEY", api_token)
            .spawn()
            .map_err(|err| spawn_failure(binary, &err))?;

        // It executed, so whatever a stale preflight says about this build is
        // out of date — including an `Unusable` verdict reached from a bare
        // signal, which is what a jetsam kill looks like.
        preflight_cache().forget(binary);

        let child = Arc::new(SyncMutex::new(Some(child)));
        let registration = match app.try_state::<SidecarRegistry>() {
            Some(registry) => Some(Registration {
                app: app.clone(),
                id: registry.register(&child, endpoint),
            }),
            None => {
                tracing::debug!(
                    "SidecarRegistry not managed by AppHandle; falling back to Drop-only cleanup"
                );
                None
            }
        };

        Ok((
            rx,
            Self {
                child,
                registration,
                endpoint: endpoint.to_string(),
                armed: true,
            },
        ))
    }

    /// The child slot, for the drain task to clear when the process exits.
    fn child(&self) -> Arc<SyncMutex<Option<CommandChild>>> {
        Arc::clone(&self.child)
    }

    /// Disarm and hand ownership to the long-lived handle.
    fn into_handle(
        mut self,
        config: &SidecarConfig,
        binary: SidecarBinary,
        port: PortReservation,
        api_token: String,
    ) -> SidecarHandle {
        self.armed = false;
        SidecarHandle {
            endpoint: self.endpoint.clone(),
            api_token,
            child: Arc::clone(&self.child),
            context_size: config.context_size,
            binary,
            n_gpu_layers: config.n_gpu_layers,
            registration: self.registration.take(),
            _port: port,
            degraded: None,
        }
    }
}

impl Drop for SpawnedChild {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let child = self.child.lock().take();
        if let Some(registration) = self.registration.take() {
            registration.release();
        }
        let Some(child) = child else { return };
        match child.kill() {
            Ok(()) => tracing::warn!(
                endpoint = %self.endpoint,
                "Killed llama-server: its startup was abandoned before it had an owner"
            ),
            Err(err) => tracing::warn!(
                endpoint = %self.endpoint,
                error = %err,
                "Failed to kill an abandoned llama-server startup"
            ),
        }
    }
}

/// Process-wide registry of live sidecars.
///
/// Held as Tauri-managed state via `app.manage()` so the app's
/// `RunEvent::ExitRequested` handler can synchronously drain it before
/// the tokio runtime tears down. Each entry holds a strong reference to a
/// child and an id its owner releases on drop, so a child whose only other
/// owner is a cancelled startup future is still reachable here.
///
/// # Four layers of zombie protection
///
/// 1. **Normal Drop.** Handle goes out of scope → `Drop` calls
///    `kill()`. Covers happy path (model swap, container teardown
///    while runtime is alive).
///
/// 2. **`RunEvent::ExitRequested` sweep.** Tauri's "user closed the
///    window" hook walks the registry and synchronously kills
///    everything before returning to shutdown. Covers normal app
///    quit where the tokio runtime is mid-teardown and Drop racing
///    against runtime shutdown is unreliable.
///
/// 3. **Windows Job Object.** On Windows, every sidecar is assigned
///    to a kernel Job Object configured with `KILL_ON_JOB_CLOSE`.
///    The OS guarantees they die when our process handle closes —
///    even if Lattice segfaults, gets `taskkill /f`'d, or otherwise
///    bypasses our shutdown hooks. This is the load-bearing layer:
///    layers 1+2 are best-effort, 3 is a hardware-level guarantee.
///    See `JobObjectGuard` below.
///
/// 4. **Boot-time orphan reap.** `reap_orphan_sidecars()` at app
///    startup catches anything that survived the previous run.
///    This is the load-bearing layer **on macOS and Linux** where
///    no Job Object equivalent exists: `kill -9` on Lattice itself
///    will leak the sidecar until Lattice is next started, at which
///    point the reaper finds and kills it. A future improvement on
///    Linux is to use `prctl(PR_SET_PDEATHSIG, SIGKILL)` as a
///    pre-exec hook on the spawn — but `tauri-plugin-shell` doesn't
///    expose a pre-exec hook, so doing that requires dropping to
///    raw `std::process::Command::pre_exec` and giving up the
///    plugin's bundled-binary resolution. Tracked as a follow-up;
///    the boot-time reap is good enough for v1.
///
/// # Why `parking_lot::Mutex` (sync) not `tokio::Mutex` (async)
///
/// The shutdown-time consumer is a synchronous Tauri callback. Using
/// a tokio::Mutex would force `Handle::current().block_on()`, which
/// can deadlock during runtime teardown. `CommandChild::kill()` itself
/// is synchronous (it just calls into `shared_child::SharedChild`),
/// so the entire kill path can stay sync.
pub struct SidecarRegistry {
    entries: SyncMutex<Vec<RegistryEntry>>,

    /// Windows Job Object (no-op on other platforms). Sidecar PIDs
    /// get assigned to this object on spawn so Windows kernel kills
    /// them automatically when our process handle closes — even on
    /// segfault.
    #[cfg(windows)]
    job: SyncMutex<Option<JobObjectGuard>>,
}

struct RegistryEntry {
    /// Identifies the slot so its owner can release it. A `Weak` used to
    /// stand in for ownership, but a `Weak` cannot keep alive the one case
    /// registering early exists for: a child whose only strong reference is
    /// a cancelled startup future.
    id: u64,
    child: Arc<SyncMutex<Option<CommandChild>>>,
    endpoint: String,
}

impl SidecarRegistry {
    pub fn new() -> Self {
        #[cfg(windows)]
        let job = match JobObjectGuard::create() {
            Ok(guard) => {
                tracing::debug!("Created Windows Job Object for sidecar lifecycle");
                Some(guard)
            }
            Err(err) => {
                // Non-fatal: layers 1+2+4 still apply. Job Object is
                // belt-and-suspenders. Most likely cause of failure
                // is running under another Job Object that doesn't
                // allow nesting (Tauri test harnesses, AppContainer
                // sandboxes), in which case we just warn and move on.
                tracing::warn!(
                    "Failed to create Windows Job Object; sidecar processes may \
                     leak on hard kill: {err}"
                );
                None
            }
        };

        Self {
            entries: SyncMutex::new(Vec::new()),
            #[cfg(windows)]
            job: SyncMutex::new(job),
        }
    }

    /// Register a live sidecar. Called by `SidecarManager::start_attempt`
    /// after readiness confirmation. Cleans up dead weak refs from
    /// previous spawns at the same time.
    ///
    /// On Windows, also assigns the sidecar PID to the registry's
    /// Job Object so the kernel kills it automatically when our
    /// process handle closes. The PID is read from the
    /// `CommandChild` while the registry holds it locked, so we know
    /// it hasn't been killed/swapped underneath us.
    fn register(&self, child: &Arc<SyncMutex<Option<CommandChild>>>, endpoint: &str) -> u64 {
        // Pull PID out for the Job Object assignment (Windows-only).
        // We do this before pushing to entries because the lock on
        // `child` is internal to the Arc — we just need to peek.
        #[cfg(windows)]
        {
            let pid_opt = child.lock().as_ref().map(|c| c.pid());
            if let Some(pid) = pid_opt {
                if let Some(job) = self.job.lock().as_ref() {
                    if let Err(err) = job.assign_pid(pid) {
                        tracing::warn!(
                            pid,
                            endpoint = endpoint,
                            "Failed to assign sidecar to Job Object: {err}"
                        );
                    } else {
                        tracing::debug!(
                            pid,
                            endpoint = endpoint,
                            "Assigned sidecar to Windows Job Object"
                        );
                    }
                }
            }
        }

        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        let mut entries = self.entries.lock();
        // An empty child slot is a process the drain task saw exit; nothing
        // else frees a slot now that entries hold strong references.
        entries.retain(|e| e.child.lock().is_some());
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
    }

    /// Release the slot `id` owns. Called by whoever owns the child — the
    /// handle, or the spawn guard on a cancelled startup.
    fn unregister(&self, id: u64) {
        self.entries.lock().retain(|e| e.id != id);
    }

    /// Synchronously kill every registered sidecar. Idempotent — call
    /// from the Tauri RunEvent::ExitRequested handler. Returns the
    /// number of children killed (for logging).
    ///
    /// Lock discipline: we drain `entries` into a local Vec first, then
    /// release the registry mutex before walking children. `child.kill()`
    /// is a blocking syscall (TerminateProcess on Windows, SIGKILL on
    /// Unix) — holding the registry lock across it would block any
    /// concurrent `register()` calls (e.g. a spawn-in-flight on another
    /// thread) for the full kill duration.
    pub fn kill_all(&self) -> usize {
        // Drain into a local Vec, then release entries lock immediately.
        let drained: Vec<RegistryEntry> = {
            let mut entries = self.entries.lock();
            entries.drain(..).collect()
        };

        let mut killed = 0;
        for entry in drained {
            // Lock the per-child mutex briefly: take the CommandChild
            // out, then release before calling kill so a concurrent
            // SidecarHandle::stop sees an empty slot and no-ops.
            let child_opt = entry.child.lock().take();
            if let Some(child) = child_opt {
                match child.kill() {
                    Ok(()) => {
                        killed += 1;
                        tracing::info!(
                            endpoint = %entry.endpoint,
                            "Killed sidecar via registry shutdown"
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            endpoint = %entry.endpoint,
                            error = %err,
                            "Failed to kill sidecar at shutdown"
                        );
                    }
                }
            }
        }
        killed
    }

    /// Number of currently-tracked live entries. For diagnostics.
    pub fn live_count(&self) -> usize {
        let entries = self.entries.lock();
        entries.iter().filter(|e| e.child.lock().is_some()).count()
    }
}

impl Default for SidecarRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard around a Windows Job Object configured to kill all
/// member processes when the job handle closes.
///
/// The Job Object uses `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
/// which means: when the OS handle to this job closes (because our
/// process exits — including segfault, taskkill /f, or panic),
/// every process assigned to it is terminated by the kernel.
///
/// Drop closes the handle. We rely on Drop running when the
/// `SidecarRegistry` is dropped (which happens at app shutdown OR
/// process abort, since Tauri-managed state lifetime ends with the
/// process). For abnormal exits where Drop doesn't run, the OS
/// closes orphaned handles automatically — same effect.
#[cfg(windows)]
struct JobObjectGuard {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl JobObjectGuard {
    fn create() -> Result<Self, String> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        // SAFETY: CreateJobObjectW with null `name` and null security
        // attributes returns a handle owned by the calling process.
        // Per Microsoft docs, the only failure modes are out-of-resources
        // and access denied, both of which surface as INVALID_HANDLE
        // and we can convert to a Rust error. No memory borrowed across
        // the FFI boundary.
        #[allow(unsafe_code)]
        let handle: HANDLE = unsafe { CreateJobObjectW(None, windows::core::PCWSTR::null()) }
            .map_err(|e| format!("CreateJobObjectW failed: {e}"))?;

        if handle.is_invalid() {
            return Err("CreateJobObjectW returned invalid handle".to_string());
        }

        // Configure: kill all child processes when the job handle
        // closes. This is the kill_on_close behavior; without this
        // flag, dead Lattice would leave llama-server running.
        let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };

        // SAFETY: SetInformationJobObject reads the `info` struct
        // through the void pointer for the size we specify
        // (`size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()`).
        // `info` is a stack-local of exactly that type, owned by us
        // for the duration of the call. The handle is the one we
        // just created and haven't closed.
        #[allow(unsafe_code)]
        let result = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )
        };

        if let Err(err) = result {
            // Best-effort: try to close the handle we just created.
            #[allow(unsafe_code)]
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(handle);
            }
            return Err(format!("SetInformationJobObject failed: {err}"));
        }

        Ok(Self { handle })
    }

    fn assign_pid(&self, pid: u32) -> Result<(), String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        // Open a handle to the child process with the rights needed
        // for AssignProcessToJobObject (set_quota + terminate).
        // SAFETY: OpenProcess returns a HANDLE we own and must close.
        // PID was just read from a CommandChild we control — the
        // process is alive and we have permission to inspect it
        // (we spawned it).
        #[allow(unsafe_code)]
        let child_handle =
            unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) }
                .map_err(|e| format!("OpenProcess(pid={pid}) failed: {e}"))?;

        // SAFETY: Both handles are valid — we created `self.handle`
        // in `create()` and just opened `child_handle`.
        #[allow(unsafe_code)]
        let assign_result = unsafe { AssignProcessToJobObject(self.handle, child_handle) };

        // Always close the child handle once assignment completes;
        // the process stays alive (Windows holds its own ref), the
        // job-object membership persists.
        #[allow(unsafe_code)]
        unsafe {
            let _ = CloseHandle(child_handle);
        }

        assign_result.map_err(|e| format!("AssignProcessToJobObject(pid={pid}) failed: {e}"))?;
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for JobObjectGuard {
    fn drop(&mut self) {
        // Closing the handle triggers KILL_ON_JOB_CLOSE. Every
        // process assigned to this job dies. This is the
        // shutdown-time guarantee.
        // SAFETY: Handle was returned from CreateJobObjectW and
        // never closed by anyone else (we own it exclusively).
        #[allow(unsafe_code)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

// `windows::Win32::Foundation::HANDLE` already implements `Send` +
// `Sync` (kernel handles are thread-safe by definition; HANDLE is
// just an index into a kernel table). The compiler auto-derives both
// for `JobObjectGuard` since its only field is HANDLE — no manual
// `unsafe impl` needed.

/// Kill orphan `llama-server` processes left by a crashed Lattice instance.
///
/// Called once from `main.rs` at app boot, after the registry is
/// managed but before the DI container spawns any new sidecars.
///
/// # Match strategy
///
/// A process is ours only if its executable is an installed sidecar file
/// (`llama-server[.exe]` or `llama-server-cpu[.exe]`) sitting in the same
/// directory as the running Lattice executable — the only place
/// `tauri-plugin-shell` launches sidecars from (`is_installed_sidecar`).
/// A user's own `~/code/llama.cpp/build/bin/llama-server`, or another
/// app's bundled copy, lives elsewhere and is never touched.
///
/// The executable path comes from the OS (`Process::exe`), falling back
/// to argv[0], which tauri-plugin-shell sets to the full resolved path.
/// An install whose directory changes between launches (a re-mounted
/// AppImage, a translocated macOS app) leaves its orphans alone; missing
/// one is better than killing someone else's server.
///
/// Best-effort: `sysinfo` failures collapse to "no orphans found" and
/// the scan continues. We never panic from here.
pub fn reap_orphan_sidecars() {
    let Some(lattice_dir) = tauri::utils::platform::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(canonical_or_raw))
    else {
        tracing::debug!("Skipping orphan sidecar scan: Lattice executable directory unknown");
        return;
    };

    // `refresh_processes()` never loads argv; ask for it (and the exe path)
    // explicitly.
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessRefreshKind::new()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cmd(sysinfo::UpdateKind::OnlyIfNotSet),
    );

    let mut reaped = 0usize;
    for (pid, process) in system.processes() {
        let name = process.name();
        // Cheap prefilter before touching the filesystem. Linux reports the
        // 15-byte `comm`, so `llama-server-cpu` shows up as `llama-server-cp`.
        if !name.to_ascii_lowercase().starts_with(SIDECAR_BIN) {
            continue;
        }

        let exe = process
            .exe()
            .map(std::path::Path::to_path_buf)
            .or_else(|| process.cmd().first().map(PathBuf::from));
        let is_ours = exe
            .as_deref()
            .is_some_and(|exe| is_installed_sidecar(&canonical_or_raw(exe), &lattice_dir));
        if !is_ours {
            tracing::debug!(
                pid = pid.as_u32(),
                process_name = name,
                exe = ?exe,
                "Skipping `llama-server` process — not a sidecar of this Lattice install"
            );
            continue;
        }

        let killed = process.kill();
        if killed {
            reaped += 1;
            tracing::warn!(
                pid = pid.as_u32(),
                process_name = name,
                "Reaped orphan llama-server process from previous Lattice run"
            );
        } else {
            // sysinfo returns false for both "permission denied" and
            // "process already terminated." The latter is success for
            // our purposes (the orphan is gone), the former leaves it
            // for the next-boot pass.
            tracing::debug!(
                pid = pid.as_u32(),
                process_name = name,
                "Found orphan llama-server but kill returned false (already dead or perms)"
            );
        }
    }

    if reaped > 0 {
        tracing::info!(
            count = reaped,
            "Reaped {} orphan llama-server process(es) on startup",
            reaped
        );
    }
}

/// Whether `exe` is one of the installed sidecar files in `lattice_dir`.
/// Both paths must already be in the same (canonical) form.
fn is_installed_sidecar(exe: &std::path::Path, lattice_dir: &std::path::Path) -> bool {
    let is_sidecar_file = exe
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            let stem = name.strip_suffix(".exe").unwrap_or(&name);
            stem == SIDECAR_BIN || stem == SIDECAR_CPU_BIN
        });
    is_sidecar_file && exe.parent() == Some(lattice_dir)
}

/// Canonical form for path comparison (symlinks resolved; `\\?\` form on
/// Windows), or the path as given when it can't be resolved.
fn canonical_or_raw(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Run the `--version` preflight for every bundled build in the background,
/// so the log records at startup whether local models can run at all. Never
/// blocks the caller; the result is cached for the first model load.
pub fn spawn_binary_preflight(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for &binary in SidecarBinary::bundled() {
            preflight(&app, binary, preflight_cache()).await;
        }
    });
}

/// Spawns and supervises a `llama-server` sidecar.
pub struct SidecarManager;

impl SidecarManager {
    /// Spawn a llama-server sidecar, falling back until something runs.
    ///
    /// The first attempt uses `config` on the primary build. Each failure
    /// is classified (`AttemptFailure`) and `next_attempt` decides what to
    /// try next:
    ///
    /// - The binary itself cannot run (missing dylib/DLL, wrong
    ///   architecture, unsupported CPU instructions): retrying the same
    ///   executable cannot help, so Windows/Linux go straight to the
    ///   CPU-only build and macOS fails immediately.
    /// - GPU or model startup failure with offload requested: retry the
    ///   same build with `n_gpu_layers = 0`. Covers Vulkan driver crashes,
    ///   Metal context-creation races on cold boot, and out-of-VRAM
    ///   during weight allocation.
    /// - That CPU-mode retry failing in a way that still points at the
    ///   binary or the GPU backend (a Vulkan build initializes its backend
    ///   even with no layers offloaded): try the CPU-only build on
    ///   Windows/Linux.
    ///
    /// When every attempt fails, the binary-unusable error wins (it names
    /// the fix); otherwise the first error, from the requested
    /// configuration, is reported. The others are summarized beneath it.
    ///
    /// The build that finally ran is logged under
    /// `target = "sidecar_fallback"` and recorded on the handle.
    pub async fn start_with_fallback(
        app: &AppHandle,
        config: SidecarConfig,
    ) -> Result<SidecarHandle, LLMError> {
        let requested = Attempt {
            binary: SidecarBinary::Primary,
            gpu_offload: config.n_gpu_layers > 0,
        };
        let fallback_config = config.without_gpu_offload();
        let deadline = Instant::now() + STARTUP_TOTAL_BUDGET;
        let mut attempt = requested;
        let mut retries = 0u32;
        let mut failures: Vec<(Attempt, AttemptError)> = Vec::new();

        loop {
            let attempt_config = if attempt == requested {
                config.clone()
            } else {
                fallback_config.clone()
            };
            match Self::start_attempt(app, attempt.binary, attempt_config, deadline).await {
                Ok(mut handle) => {
                    if attempt == requested {
                        tracing::info!(
                            target: "sidecar_fallback",
                            binary = attempt.binary.label(),
                            mode = attempt.mode(),
                            retries,
                            "llama-server running with the requested configuration"
                        );
                    } else {
                        let degraded = format!(
                            "requested {} ({}, {} ctx) but running {} ({}, {} ctx)",
                            requested.binary.label(),
                            requested.mode(),
                            config.context_size,
                            attempt.binary.label(),
                            attempt.mode(),
                            handle.context_size(),
                        );
                        tracing::warn!(
                            target: "sidecar_fallback",
                            binary = attempt.binary.label(),
                            mode = attempt.mode(),
                            failed_attempts = failures.len(),
                            degraded = %degraded,
                            "llama-server running in a degraded fallback configuration"
                        );
                        handle.degraded = Some(degraded);
                    }
                    return Ok(handle);
                }
                Err(err) => {
                    // A retryable failure is one the same configuration can
                    // still win: the port was taken between allocation and
                    // llama-server's own bind. Degrading the configuration
                    // for it is how a port collision used to pin a whole
                    // session to CPU-only at half context.
                    let retryable = err.kind == AttemptFailure::Retry
                        && retries < MAX_SAME_CONFIG_RETRIES
                        && Instant::now() < deadline;
                    let next = if retryable {
                        Some(attempt)
                    } else {
                        next_attempt(attempt, err.kind, SidecarBinary::CPU_BUILD_BUNDLED)
                    };
                    let expected = matches!(
                        err.kind,
                        AttemptFailure::BinaryUnusable(UnusableReason::MissingGpuRuntime)
                    ) && next.is_some();
                    if expected {
                        // No GPU runtime on this machine: the CPU build is the
                        // answer, and the user has nothing to fix.
                        tracing::info!(
                            target: "sidecar_fallback",
                            binary = attempt.binary.label(),
                            mode = attempt.mode(),
                            next_binary = next.map(|n| n.binary.label()),
                            "llama-server has no GPU runtime here; falling back"
                        );
                    } else {
                        tracing::warn!(
                            target: "sidecar_fallback",
                            binary = attempt.binary.label(),
                            mode = attempt.mode(),
                            failure = ?err.kind,
                            next_binary = next.map(|n| n.binary.label()),
                            next_mode = next.map(|n| n.mode()),
                            error = %err.message,
                            "llama-server startup attempt failed"
                        );
                    }
                    failures.push((attempt, err));
                    if retryable {
                        retries += 1;
                        continue;
                    }
                    match next {
                        Some(next) => attempt = next,
                        None => break,
                    }
                }
            }
        }

        tracing::error!(
            target: "sidecar_fallback",
            attempts = failures.len(),
            "Every llama-server startup attempt failed"
        );
        Err(reported_error(failures))
    }

    /// One spawn of one build, returning once the server prints its
    /// readiness needle on stderr, or a classified failure.
    async fn start_attempt(
        app: &AppHandle,
        binary: SidecarBinary,
        config: SidecarConfig,
        deadline: Instant,
    ) -> Result<SidecarHandle, AttemptError> {
        // 1. Validate the model file exists. llama-server's error path
        //    on a missing file is fine, but failing fast here gives a
        //    cleaner error surface to the UI.
        if !config.model_path.exists() {
            return Err(AttemptError::fatal(format!(
                "Model file not found: {}",
                config.model_path.display()
            )));
        }

        // 2. Refuse a binary that cannot run before waiting on it. The
        //    first call per build runs `--version`; later calls reuse it.
        if let PreflightOutcome::Unusable {
            message, reason, ..
        } = preflight(app, binary, preflight_cache()).await
        {
            return Err(AttemptError {
                kind: AttemptFailure::BinaryUnusable(reason),
                message,
            });
        }

        // 3. Reserve a free port. The reservation outlives the handle, so a
        //    concurrent spawn (chat and utility prewarm start together)
        //    cannot be handed the same number; llama-server still owns the
        //    actual bind, and losing that race is a `Retry`.
        let reservation = reserve_free_port().map_err(|err| {
            AttemptError::fatal(format!("Failed to allocate ephemeral port: {err}"))
        })?;
        let port = reservation.port();
        let endpoint = format!("http://127.0.0.1:{port}");

        // 4. Build the args. Order doesn't matter to llama-server. The token is
        //    minted per spawn, so a leaked one dies with its process.
        let api_token = new_api_token();
        let args = build_server_args(&config, port);
        tracing::info!(
            "Spawning llama-server sidecar: binary={} model={} port={} ngl={} ctx={}",
            binary.label(),
            config.model_path.display(),
            port,
            config.n_gpu_layers,
            config.context_size,
        );

        // 5. Spawn via tauri-plugin-shell. `.sidecar()` looks the binary
        //    up next to the app executable. A failure here (missing file,
        //    no execute permission, wrong executable format) means the
        //    binary cannot run at all. The guard returned owns the child:
        //    every path out of this function from here on either kills it
        //    or converts it into a handle.
        let (rx, spawned) = SpawnedChild::spawn(app, binary, args, &endpoint, &api_token)?;

        // 6. Spawn the long-lived event drain task.
        //    The drain owns the receiver for the rest of the sidecar's
        //    life — feeds stderr/stdout into tracing, keeps the startup
        //    output tail, records the sticky startup signals classification
        //    needs, detects unexpected termination, and signals readiness
        //    via a oneshot.
        let tail = Arc::new(SyncMutex::new(OutputTail::default()));
        let signals = Arc::new(StartupSignals::default());
        let ready_rx = spawn_event_drain(
            rx,
            endpoint.clone(),
            spawned.child(),
            Arc::clone(&tail),
            Arc::clone(&signals),
        );

        // 7. Await readiness: `/health` answering 200, or the readiness line.
        //    On failure the guard's Drop kills the child.
        if let Err(end) = await_ready(ready_rx, &endpoint, &api_token, &signals, deadline).await {
            tracing::warn!(
                endpoint = %endpoint,
                binary = binary.label(),
                outcome = ?end,
                "Sidecar startup failed; killing child to avoid zombie"
            );
            let output = tail.lock().snapshot();
            return Err(startup_failure(binary, &end, &output, signals.facts()));
        }

        // Recorded explicitly so a mismatch between what the sidecar runs and
        // what prompt budgeting assumes is visible in the log rather than
        // showing up as mysteriously truncated context on smaller machines.
        tracing::info!(
            binary = binary.label(),
            n_gpu_layers = config.n_gpu_layers,
            context_size = config.context_size,
            "llama-server ready; prompt budgeting will use this context window"
        );

        Ok(spawned.into_handle(&config, binary, reservation, api_token))
    }
}

/// One startup attempt: which build, with or without GPU offload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Attempt {
    binary: SidecarBinary,
    gpu_offload: bool,
}

impl Attempt {
    fn mode(self) -> &'static str {
        if self.gpu_offload {
            "gpu"
        } else {
            "cpu"
        }
    }
}

/// How an attempt failed, as far as the fallback policy cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptFailure {
    /// The executable cannot run on this machine at all.
    BinaryUnusable(UnusableReason),
    /// It ran, then died or hung while bringing up its GPU backend.
    GpuInit,
    /// It ran, then died or hung loading the model (or for no
    /// recognizable reason).
    Startup,
    /// The same configuration deserves another go — the port we handed it
    /// was taken. Changing the configuration cannot help, and treating this
    /// as an unrecognized startup failure is what silently dropped a whole
    /// session to CPU-only at half context.
    Retry,
    /// Bad input (missing model file, no free port); retrying won't help.
    Fatal,
}

#[derive(Debug)]
struct AttemptError {
    kind: AttemptFailure,
    message: String,
}

impl AttemptError {
    fn fatal(message: String) -> Self {
        Self {
            kind: AttemptFailure::Fatal,
            message,
        }
    }

    fn into_llm_error(self) -> LLMError {
        match self.kind {
            AttemptFailure::BinaryUnusable(_) => LLMError::SidecarBinaryUnusable(self.message),
            AttemptFailure::GpuInit | AttemptFailure::Startup | AttemptFailure::Retry => {
                LLMError::GenerationFailed(self.message)
            }
            AttemptFailure::Fatal => LLMError::Other(self.message),
        }
    }

    /// How useful this failure is as *the* reported error. See
    /// [`UnusableReason::report_rank`]; everything that is not an unusable
    /// binary ranks between the two unusable cases, so a missing GPU runtime
    /// never outranks the real reason a later attempt failed.
    fn report_rank(&self) -> u8 {
        match self.kind {
            AttemptFailure::BinaryUnusable(reason) => reason.report_rank(),
            _ => 2,
        }
    }
}

/// The fallback policy: what to try after `failed` ended with `failure`.
///
/// | failed attempt      | failure                    | CPU build bundled   | macOS               |
/// |---------------------|----------------------------|---------------------|---------------------|
/// | primary, gpu        | binary unusable            | CPU build           | stop                |
/// | primary, gpu        | GPU init / model startup   | primary, cpu        | primary, cpu        |
/// | primary, cpu        | binary unusable / GPU init | CPU build           | stop                |
/// | primary, cpu        | model startup              | stop                | stop                |
/// | CPU build           | anything                   | stop                | —                   |
/// | any                 | fatal                      | stop                | stop                |
fn next_attempt(
    failed: Attempt,
    failure: AttemptFailure,
    cpu_build_bundled: bool,
) -> Option<Attempt> {
    let cpu_build = cpu_build_bundled.then_some(Attempt {
        binary: SidecarBinary::Cpu,
        gpu_offload: false,
    });
    match (failed.binary, failure) {
        (SidecarBinary::Cpu, _) | (_, AttemptFailure::Fatal) => None,
        // A bind collision that survived its retries is not fixed by a
        // different build or a smaller context.
        (_, AttemptFailure::Retry) => None,
        (SidecarBinary::Primary, AttemptFailure::BinaryUnusable(_)) => cpu_build,
        (SidecarBinary::Primary, _) if failed.gpu_offload => Some(Attempt {
            binary: SidecarBinary::Primary,
            gpu_offload: false,
        }),
        (SidecarBinary::Primary, AttemptFailure::GpuInit) => cpu_build,
        (SidecarBinary::Primary, AttemptFailure::Startup) => None,
    }
}

/// The error to report once every attempt has failed: the most actionable
/// one, earliest first among equals — an unusable binary names its fix, but a
/// missing GPU runtime ranks below the reason the CPU build then failed.
/// The remaining attempts are summarized beneath it, one line each.
fn reported_error(mut failures: Vec<(Attempt, AttemptError)>) -> LLMError {
    if failures.is_empty() {
        return LLMError::Other("llama-server was never started".to_string());
    }
    let chosen = failures
        .iter()
        .enumerate()
        .max_by_key(|(index, (_, err))| (err.report_rank(), std::cmp::Reverse(*index)))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let (_, mut reported) = failures.remove(chosen);
    if !failures.is_empty() {
        reported.message.push_str("\n\nOther startup attempts:");
        for (attempt, err) in &failures {
            let summary = err.message.lines().next().unwrap_or_default();
            reported.message.push_str(&format!(
                "\n- {} ({}): {}",
                attempt.binary.label(),
                attempt.mode(),
                clip(summary, ERROR_LINE_MAX_BYTES)
            ));
        }
    }
    reported.into_llm_error()
}

/// How a startup attempt ended without reaching readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupEnd {
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    EventError(String),
    StreamClosed,
    TimedOut(TimeoutKind),
    /// The drain task went away without reporting.
    MonitorGone,
}

/// Which bound the readiness wait hit. The distinction is the difference
/// between "your model is broken" and "your machine needed longer than we
/// allow", so it is carried into the message rather than flattened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeoutKind {
    /// Went silent: no output line and no `/health` answer for this long.
    Silence(u64),
    /// Talked the whole time but never became ready.
    Cap(u64),
    /// The budget shared by every fallback attempt ran out.
    Budget(u64),
}

/// Classify a failed attempt from how it ended, the sticky signals seen while
/// it ran, and what it printed.
fn startup_failure(
    binary: SidecarBinary,
    end: &StartupEnd,
    output: &[String],
    facts: StartupFacts,
) -> AttemptError {
    let (code, signal) = match end {
        StartupEnd::Exited { code, signal } => (*code, *signal),
        _ => (None, None),
    };
    if let Some(evidence) = binary_unusable_reason(binary, code, signal, output) {
        return AttemptError {
            kind: AttemptFailure::BinaryUnusable(evidence.reason),
            message: binary_unusable_message(binary, &evidence, output),
        };
    }

    if facts.bind_failure {
        return AttemptError {
            kind: AttemptFailure::Retry,
            message: format!(
                "{} could not bind its HTTP port; another process claimed it first.{}",
                binary.label(),
                render_tail(output)
            ),
        };
    }

    // llama.cpp initializes its backends before touching the model, so a
    // crash or hang before the model loader ever spoke happened in backend
    // (GPU) initialization. A plain non-zero exit is an orderly error
    // (bad arguments, unreadable file) and says nothing about the GPU.
    let crashed =
        matches!(end, StartupEnd::TimedOut(_)) || signal.is_some() || code.is_some_and(|c| c < 0);
    let kind = if facts.gpu_failure || (crashed && !facts.model_loader) {
        AttemptFailure::GpuInit
    } else {
        AttemptFailure::Startup
    };

    let what = match end {
        StartupEnd::Exited { .. } => {
            format!("exited during startup ({})", describe_exit(code, signal))
        }
        StartupEnd::EventError(err) => format!("failed while starting: {err}"),
        StartupEnd::StreamClosed => "closed its output before it became ready".to_string(),
        StartupEnd::TimedOut(TimeoutKind::Silence(secs)) => {
            format!("went silent for {secs} s before it became ready")
        }
        StartupEnd::TimedOut(TimeoutKind::Cap(secs)) => {
            format!("did not become ready within {secs} s")
        }
        StartupEnd::TimedOut(TimeoutKind::Budget(secs)) => {
            format!("ran out of the {secs} s budget shared by every startup attempt")
        }
        StartupEnd::MonitorGone => "stopped reporting before it became ready".to_string(),
    };
    let hint = match kind {
        AttemptFailure::GpuInit => "Its GPU backend appears to have failed to initialize.",
        _ => {
            "The model file may be corrupt or unsupported by this llama-server build, \
             or too large for this machine's memory."
        }
    };
    AttemptError {
        kind,
        message: format!("{} {what}. {hint}{}", binary.label(), render_tail(output)),
    }
}

/// Evidence that the executable itself cannot run.
#[derive(Debug, Clone)]
struct UnusableEvidence {
    reason: UnusableReason,
    /// The specific finding, for the message.
    detail: String,
    /// Whether this will say the same thing next time. A loader diagnostic or
    /// an NTSTATUS loader code names the missing piece, so it will. A bare
    /// signal does not: macOS jetsam under memory pressure and an AV/EDR hook
    /// both look exactly like a rejected code signature, and neither survives
    /// a retry — so an inconclusive verdict must never be cached, or one
    /// transient kill disables local models until the app restarts.
    conclusive: bool,
}

/// Why the executable itself cannot run, if the evidence says so.
///
/// `code`/`signal` are the exit status (if it exited); `output` is what it
/// printed. Loader diagnostics are the most specific evidence because they
/// name the missing piece, so they are checked first.
fn binary_unusable_reason(
    binary: SidecarBinary,
    code: Option<i32>,
    signal: Option<i32>,
    output: &[String],
) -> Option<UnusableEvidence> {
    if let Some(line) = output
        .iter()
        .map(|line| line.trim())
        .find(|line| is_loader_failure_line(line))
    {
        let lower = line.to_ascii_lowercase();
        let reason = if mentions_gpu_runtime(&lower) {
            UnusableReason::MissingGpuRuntime
        } else if lower.contains("illegal instruction") {
            UnusableReason::UnsupportedCpu
        } else {
            UnusableReason::BrokenInstall
        };
        return Some(UnusableEvidence {
            reason,
            detail: clip(line, ERROR_LINE_MAX_BYTES).to_string(),
            conclusive: true,
        });
    }
    if let Some((reason, detail)) = code.and_then(|code| windows_loader_status(binary, code)) {
        return Some(UnusableEvidence {
            reason,
            detail: detail.to_string(),
            conclusive: true,
        });
    }
    match signal {
        Some(SIGILL) => Some(UnusableEvidence {
            reason: UnusableReason::UnsupportedCpu,
            detail: "it was stopped by SIGILL: the CPU lacks an instruction set this build \
                     requires (for example AVX2)"
                .to_string(),
            conclusive: true,
        }),
        // llama-server prints its build banner first thing, so a kill
        // before any output usually means the OS refused to run it (on
        // macOS, typically a rejected code signature) — but memory
        // pressure looks identical, so this verdict is not cached.
        Some(SIGKILL) if output.iter().all(|line| line.trim().is_empty()) => {
            Some(UnusableEvidence {
                reason: UnusableReason::BrokenInstall,
                detail: "the OS killed it before it printed anything (on macOS this usually \
                         means its code signature was rejected, though memory pressure can \
                         look the same)"
                    .to_string(),
                conclusive: false,
            })
        }
        _ => None,
    }
}

/// Whether a loader diagnostic names a GPU runtime rather than part of our
/// own install — a missing `libvulkan.so.1` is the user's driver, not our file.
fn mentions_gpu_runtime(lower: &str) -> bool {
    const NEEDLES: &[&str] = &["vulkan", "nvcuda", "libcuda", "amdvlk"];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

const SIGILL: i32 = 4;
const SIGKILL: i32 = 9;

/// Messages from the dynamic loader or the kernel's exec path. None of
/// them can come from llama-server's own code.
fn is_loader_failure_line(line: &str) -> bool {
    const NEEDLES: &[&str] = &[
        // macOS dyld
        "Library not loaded",
        "Symbol not found",
        "incompatible architecture",
        "Bad CPU type in executable",
        // glibc ld.so
        "error while loading shared libraries",
        "symbol lookup error",
        // exec / shells
        "cannot execute binary file",
        "Exec format error",
        "Illegal instruction",
    ];
    NEEDLES.iter().any(|needle| line.contains(needle))
        // ld.so: "version `GLIBC_2.38' not found", likewise GLIBCXX_ / CXXABI_.
        || (line.contains("not found") && (line.contains("GLIBC") || line.contains("CXXABI_")))
}

/// Windows NTSTATUS exit codes raised by the image loader or the CPU before
/// the program's own code runs. They arrive as the `i32` reinterpretation
/// of the `u32` status.
fn windows_loader_status(
    binary: SidecarBinary,
    code: i32,
) -> Option<(UnusableReason, &'static str)> {
    match code as u32 {
        // On the accelerated build the missing DLL is `vulkan-1.dll`, which
        // ships with the graphics driver; on the CPU build nothing external
        // is needed, so a missing DLL means our own files are wrong.
        0xC000_0135 => {
            let reason = match binary {
                SidecarBinary::Primary => UnusableReason::MissingGpuRuntime,
                SidecarBinary::Cpu => UnusableReason::BrokenInstall,
            };
            Some((
                reason,
                "a DLL it needs was not found (exit code 0xC0000135, STATUS_DLL_NOT_FOUND; \
                 for the GPU build this is usually a missing vulkan-1.dll)",
            ))
        }
        0xC000_0139 => Some((
            UnusableReason::BrokenInstall,
            "a DLL it needs lacks a required entry point \
             (exit code 0xC0000139, STATUS_ENTRYPOINT_NOT_FOUND)",
        )),
        0xC000_007B => Some((
            UnusableReason::BrokenInstall,
            "it or one of its DLLs is not a valid image for this system \
             (exit code 0xC000007B, STATUS_INVALID_IMAGE_FORMAT)",
        )),
        0xC000_001D => Some((
            UnusableReason::UnsupportedCpu,
            "the CPU lacks an instruction set this build requires, for example AVX2 \
             (exit code 0xC000001D, STATUS_ILLEGAL_INSTRUCTION)",
        )),
        _ => None,
    }
}

/// A line that names a GPU backend together with a failure.
fn line_is_gpu_failure(lower: &str) -> bool {
    const BACKENDS: &[&str] = &["vulkan", "vk::", "cuda", "ggml_metal"];
    const FAILURES: &[&str] = &[
        "error",
        "fail",
        "exception",
        "abort",
        "device lost",
        "insufficient",
    ];
    BACKENDS.iter().any(|backend| lower.contains(backend))
        && FAILURES.iter().any(|failure| lower.contains(failure))
}

/// A line saying llama-server could not take the port we gave it.
fn line_is_bind_failure(lower: &str) -> bool {
    const NEEDLES: &[&str] = &[
        // b8981: "couldn't bind HTTP server socket, hostname: 127.0.0.1, port: N"
        "couldn't bind",
        "could not bind",
        "failed to bind",
        "error binding",
        "address already in use",
        "address in use",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// A line from llama.cpp's model loader: proof it got past backend init.
fn line_is_model_loader(lower: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "llama_model_load",
        "load_tensors:",
        "loading model",
        "model loaded",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// Output lines that name a GPU backend together with a failure. Production
/// reads the sticky signal instead; this is the same predicate over a block.
#[cfg(test)]
fn looks_like_gpu_failure(output: &[String]) -> bool {
    output
        .iter()
        .any(|line| line_is_gpu_failure(&line.to_ascii_lowercase()))
}

fn binary_unusable_message(
    binary: SidecarBinary,
    evidence: &UnusableEvidence,
    output: &[String],
) -> String {
    // The fetch script is internal tooling; it belongs in a developer build's
    // message, never in a user's settings row.
    let developer_hint =
        if cfg!(debug_assertions) && evidence.reason == UnusableReason::BrokenInstall {
            " (developers: run src-tauri/scripts/fetch-llama-binaries.sh)"
        } else {
            ""
        };
    format!(
        "Lattice's bundled {} can't run on this machine: {}. {}{developer_hint}{}",
        binary.label(),
        evidence.detail,
        evidence.reason.advice(),
        render_tail(output)
    )
}

/// `tauri-plugin-shell` could not start the sidecar at all.
fn spawn_failure(binary: SidecarBinary, err: &tauri_plugin_shell::Error) -> AttemptError {
    let path = resolved_sidecar_path(binary);
    let missing = matches!(err, tauri_plugin_shell::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound)
        && !path.exists();
    let detail = if missing {
        format!("{} does not exist", path.display())
    } else {
        format!("starting {} failed: {err}", path.display())
    };
    let evidence = UnusableEvidence {
        reason: UnusableReason::BrokenInstall,
        detail,
        conclusive: true,
    };
    AttemptError {
        kind: AttemptFailure::BinaryUnusable(evidence.reason),
        message: binary_unusable_message(binary, &evidence, &[]),
    }
}

/// The preflight form of [`spawn_failure`], keeping the classification the
/// message was built from rather than dropping it.
fn spawn_failure_outcome(
    binary: SidecarBinary,
    err: &tauri_plugin_shell::Error,
) -> PreflightOutcome {
    let failure = spawn_failure(binary, err);
    let reason = match failure.kind {
        AttemptFailure::BinaryUnusable(reason) => reason,
        _ => UnusableReason::BrokenInstall,
    };
    PreflightOutcome::Unusable {
        message: failure.message,
        reason,
        conclusive: true,
    }
}

/// Where tauri-plugin-shell looks for a sidecar: next to the app
/// executable, with `.exe` on Windows.
fn resolved_sidecar_path(binary: SidecarBinary) -> PathBuf {
    let file_name = format!(
        "{}{}",
        binary.label(),
        if cfg!(windows) { ".exe" } else { "" }
    );
    tauri::utils::platform::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file_name)))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

fn describe_exit(code: Option<i32>, signal: Option<i32>) -> String {
    match (code, signal) {
        (_, Some(signal)) => format!("signal {signal}"),
        (Some(code), None) if code < 0 => format!("exit code 0x{:08X}", code as u32),
        (Some(code), None) => format!("exit code {code}"),
        (None, None) => "no exit status".to_string(),
    }
}

/// The end of the captured output, formatted to follow an error sentence.
fn render_tail(output: &[String]) -> String {
    let lines: Vec<&str> = output
        .iter()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let skip = lines.len().saturating_sub(ERROR_TAIL_LINES);
    let mut rendered = format!(
        "\n\nllama-server output (last {} lines):",
        lines.len() - skip
    );
    for line in lines.iter().skip(skip) {
        rendered.push_str("\n  ");
        rendered.push_str(clip(line, ERROR_LINE_MAX_BYTES));
    }
    rendered
}

/// `text` cut to at most `max_bytes`, on a char boundary.
fn clip(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

/// Bounded tail of a sidecar's startup output, stderr and stdout
/// interleaved in arrival order.
#[derive(Debug, Default)]
struct OutputTail {
    lines: VecDeque<String>,
    bytes: usize,
}

impl OutputTail {
    fn push(&mut self, line: &str) {
        let line = clip(line, OUTPUT_LINE_MAX_BYTES);
        self.bytes += line.len();
        self.lines.push_back(line.to_string());
        while self.lines.len() > OUTPUT_TAIL_MAX_LINES || self.bytes > OUTPUT_TAIL_MAX_BYTES {
            match self.lines.pop_front() {
                Some(dropped) => self.bytes -= dropped.len(),
                None => break,
            }
        }
    }

    fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

/// Facts about a starting sidecar, recorded as its output streams and never
/// forgotten.
///
/// Classification cannot read these off [`OutputTail`]: the tail keeps the
/// last 40 lines, and a large model prints dozens of `load_tensors:` lines
/// after the loader banner, so by the time an attempt fails the proof that
/// the model loader ran has rolled off — and a healthy GPU machine got walked
/// down the degradation ladder for it. Symmetrically, a Vulkan error in the
/// first seconds of a long wait was missed. The tail stays purely for the
/// human-readable report.
#[derive(Debug, Default)]
struct StartupSignals {
    /// Output lines seen. The readiness wait watches this to tell a slow
    /// start from a hung one.
    lines: AtomicU64,
    model_loader: AtomicBool,
    gpu_failure: AtomicBool,
    bind_failure: AtomicBool,
}

impl StartupSignals {
    fn observe(&self, line: &str) {
        self.lines.fetch_add(1, Ordering::Relaxed);
        let lower = line.to_ascii_lowercase();
        // Each flag is sticky: once seen, always true.
        if !self.model_loader.load(Ordering::Relaxed) && line_is_model_loader(&lower) {
            self.model_loader.store(true, Ordering::Relaxed);
        }
        if !self.gpu_failure.load(Ordering::Relaxed) && line_is_gpu_failure(&lower) {
            self.gpu_failure.store(true, Ordering::Relaxed);
        }
        if !self.bind_failure.load(Ordering::Relaxed) && line_is_bind_failure(&lower) {
            self.bind_failure.store(true, Ordering::Relaxed);
        }
    }

    fn line_count(&self) -> u64 {
        self.lines.load(Ordering::Relaxed)
    }

    fn facts(&self) -> StartupFacts {
        StartupFacts {
            model_loader: self.model_loader.load(Ordering::Relaxed),
            gpu_failure: self.gpu_failure.load(Ordering::Relaxed),
            bind_failure: self.bind_failure.load(Ordering::Relaxed),
        }
    }
}

/// The sticky signals, as classification reads them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct StartupFacts {
    model_loader: bool,
    gpu_failure: bool,
    bind_failure: bool,
}

impl StartupFacts {
    /// Derive the same facts from a captured block of output, so a test can
    /// classify without driving the drain task.
    #[cfg(test)]
    fn scan(output: &[String]) -> Self {
        let signals = StartupSignals::default();
        for line in output {
            signals.observe(line);
        }
        signals.facts()
    }
}

/// Result of running `<binary> --version`.
#[derive(Debug, Clone)]
enum PreflightOutcome {
    Runs {
        version: String,
    },
    /// The binary cannot run; `message` is the user-facing error.
    Unusable {
        message: String,
        reason: UnusableReason,
        /// Whether the verdict may be cached — see [`UnusableEvidence`].
        conclusive: bool,
    },
    /// It started but the check proved nothing (hung, or exited non-zero
    /// without a loader signature). The real spawn gets to decide.
    Inconclusive {
        detail: String,
    },
}

impl PreflightOutcome {
    /// Everything is cached for the TTL except an inconclusive `Unusable`.
    /// Caching that one forever is what turned a single signal — a macOS
    /// jetsam kill, an AV hook — into "reinstall Lattice" for every local
    /// model load until the app was restarted, with no retry path.
    fn cacheable(&self) -> bool {
        !matches!(
            self,
            Self::Unusable {
                conclusive: false,
                ..
            }
        )
    }
}

/// Preflight verdicts, per build, with a TTL and explicit invalidation.
struct PreflightCache {
    entries: SyncMutex<Vec<(SidecarBinary, Instant, PreflightOutcome)>>,
    /// Serializes the `--version` runs so two concurrent model loads (chat and
    /// utility prewarm start together) share one.
    running: tokio::sync::Mutex<()>,
}

impl PreflightCache {
    fn new() -> Self {
        Self {
            entries: SyncMutex::new(Vec::new()),
            running: tokio::sync::Mutex::new(()),
        }
    }

    fn get(&self, binary: SidecarBinary) -> Option<PreflightOutcome> {
        self.entries
            .lock()
            .iter()
            .find(|(cached, at, _)| *cached == binary && at.elapsed() < PREFLIGHT_CACHE_TTL)
            .map(|(_, _, outcome)| outcome.clone())
    }

    fn put(&self, binary: SidecarBinary, outcome: &PreflightOutcome) {
        let mut entries = self.entries.lock();
        entries.retain(|(cached, at, _)| *cached != binary && at.elapsed() < PREFLIGHT_CACHE_TTL);
        entries.push((binary, Instant::now(), outcome.clone()));
    }

    /// Drop what we know about `binary`, so the next load decides afresh.
    fn forget(&self, binary: SidecarBinary) {
        self.entries
            .lock()
            .retain(|(cached, _, _)| *cached != binary);
    }
}

/// The process-wide cache. Injectable so the policy is testable without a
/// process-global `OnceCell` that no test can reset.
fn preflight_cache() -> &'static PreflightCache {
    static CACHE: std::sync::OnceLock<PreflightCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(PreflightCache::new)
}

/// The cached preflight for `binary`, running it on first use.
async fn preflight(
    app: &AppHandle,
    binary: SidecarBinary,
    cache: &PreflightCache,
) -> PreflightOutcome {
    if let Some(outcome) = cache.get(binary) {
        return outcome;
    }
    let _running = cache.running.lock().await;
    if let Some(outcome) = cache.get(binary) {
        return outcome;
    }
    let outcome = run_preflight(app, binary).await;
    if outcome.cacheable() {
        cache.put(binary, &outcome);
    }
    outcome
}

/// Every sidecar spawn is preceded by this preflight (cached per build),
/// so process-wide spawn setup belongs here.
async fn run_preflight(app: &AppHandle, binary: SidecarBinary) -> PreflightOutcome {
    #[cfg(windows)]
    suppress_loader_error_dialogs();

    let started = Instant::now();
    let path = resolved_sidecar_path(binary);
    let outcome = match app.shell().sidecar(binary.label()) {
        Err(err) => spawn_failure_outcome(binary, &err),
        Ok(command) => match command.arg("--version").spawn() {
            Err(err) => spawn_failure_outcome(binary, &err),
            Ok((rx, child)) => collect_preflight(binary, rx, child).await,
        },
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match &outcome {
        PreflightOutcome::Runs { version } => tracing::info!(
            binary = binary.label(),
            path = %path.display(),
            elapsed_ms,
            "llama-server preflight: {version}"
        ),
        // A GPU build with no GPU runtime is an ordinary machine, not an
        // incident: the CPU build takes over and the user has nothing to do.
        PreflightOutcome::Unusable {
            message,
            reason: UnusableReason::MissingGpuRuntime,
            ..
        } if SidecarBinary::CPU_BUILD_BUNDLED && binary == SidecarBinary::Primary => {
            tracing::info!(
                binary = binary.label(),
                path = %path.display(),
                elapsed_ms,
                "llama-server preflight: no GPU runtime on this machine; the CPU build will \
                 be used: {message}"
            )
        }
        PreflightOutcome::Unusable { message, .. } => tracing::error!(
            binary = binary.label(),
            path = %path.display(),
            elapsed_ms,
            "llama-server preflight: binary cannot run: {message}"
        ),
        PreflightOutcome::Inconclusive { detail } => tracing::warn!(
            binary = binary.label(),
            path = %path.display(),
            elapsed_ms,
            "llama-server preflight inconclusive; model loads will still try it: {detail}"
        ),
    }
    outcome
}

/// Without this, a sidecar with a missing DLL (the Vulkan build without
/// `vulkan-1.dll`) makes Windows show a modal "System Error" box and holds
/// the child until someone dismisses it, so startup hangs instead of
/// failing. Children inherit the error mode; with `SEM_FAILCRITICALERRORS`
/// the loader exits at once with 0xC0000135, which is classified as an
/// unusable binary. For Lattice itself it only suppresses the same class
/// of critical-error boxes (e.g. "no disk in drive"), which then fail as
/// ordinary I/O errors.
#[cfg(windows)]
fn suppress_loader_error_dialogs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        const SEM_FAILCRITICALERRORS: u32 = 0x0001;
        // The `windows` crate exposes these behind a feature Lattice does
        // not enable; both are plain kernel32 exports.
        #[link(name = "kernel32")]
        extern "system" {
            fn GetErrorMode() -> u32;
            fn SetErrorMode(mode: u32) -> u32;
        }
        // SAFETY: both calls only read and set this process's error-mode
        // flags; no pointers cross the boundary.
        #[allow(unsafe_code)]
        unsafe {
            SetErrorMode(GetErrorMode() | SEM_FAILCRITICALERRORS);
        }
    });
}

async fn collect_preflight(
    binary: SidecarBinary,
    mut rx: Receiver<CommandEvent>,
    child: CommandChild,
) -> PreflightOutcome {
    let mut tail = OutputTail::default();
    let exit = timeout(PREFLIGHT_TIMEOUT, async {
        let mut exit = (None, None);
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(bytes) | CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim_end();
                    if !line.is_empty() {
                        tail.push(line);
                    }
                }
                CommandEvent::Error(err) => tail.push(&format!("(event error) {err}")),
                CommandEvent::Terminated(payload) => exit = (payload.code, payload.signal),
                _ => {}
            }
        }
        exit
    })
    .await;

    let Ok((code, signal)) = exit else {
        let _ = child.kill();
        return PreflightOutcome::Inconclusive {
            detail: format!(
                "`--version` did not exit within {} s",
                PREFLIGHT_TIMEOUT.as_secs()
            ),
        };
    };

    let output = tail.snapshot();
    if let Some(evidence) = binary_unusable_reason(binary, code, signal, &output) {
        return PreflightOutcome::Unusable {
            message: binary_unusable_message(binary, &evidence, &output),
            reason: evidence.reason,
            conclusive: evidence.conclusive,
        };
    }
    if code == Some(0) {
        let version = output
            .iter()
            .find(|line| line.starts_with("version:"))
            .or_else(|| output.last())
            .cloned()
            .unwrap_or_else(|| "version: unknown".to_string());
        return PreflightOutcome::Runs { version };
    }
    PreflightOutcome::Inconclusive {
        detail: format!(
            "`--version` ended with {}{}",
            describe_exit(code, signal),
            render_tail(&output)
        ),
    }
}

/// Ports handed out and not yet released.
///
/// Binding `127.0.0.1:0` reports a port and immediately gives it back, so two
/// spawns racing — chat and utility prewarm start together — were handed the
/// same number, and the loser died with a bind error that used to be read as
/// a GPU fault and "fixed" by dropping the whole session to CPU-only.
static RESERVED_PORTS: SyncMutex<BTreeSet<u16>> = SyncMutex::new(BTreeSet::new());

/// A port claimed for one sidecar, released when the reservation drops.
/// Held by the handle, so a live server's port is never handed out again.
struct PortReservation {
    port: u16,
}

impl PortReservation {
    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for PortReservation {
    fn drop(&mut self) {
        RESERVED_PORTS.lock().remove(&self.port);
    }
}

/// Reserve an ephemeral loopback port: bind `127.0.0.1:0`, read the number
/// back, drop the listener, and claim the number process-wide. llama-server
/// still owns the real bind, so losing that race remains possible — it is
/// classified [`AttemptFailure::Retry`] and retried with a fresh port.
fn reserve_free_port() -> std::io::Result<PortReservation> {
    const ATTEMPTS: usize = 32;
    for _ in 0..ATTEMPTS {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        if RESERVED_PORTS.lock().insert(port) {
            return Ok(PortReservation { port });
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!("no unreserved loopback port after {ATTEMPTS} attempts"),
    ))
}

fn build_server_args(config: &SidecarConfig, port: u16) -> Vec<String> {
    vec![
        "-m".to_string(),
        config.model_path.to_string_lossy().to_string(),
        "--port".to_string(),
        port.to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(), // explicit — never bind public iface
        // Binding loopback keeps the port off the network but not away from
        // other software on the machine: any local process, and any page the
        // user has open, can reach 127.0.0.1. The key makes the port ours
        // (passed as `LLAMA_API_KEY`, never argv — see `SpawnedChild::spawn`),
        // and dropping the bundled web UI removes a whole HTML surface we
        // neither ship on purpose nor audit.
        "--no-webui".to_string(),
        "-ngl".to_string(),
        config.n_gpu_layers.to_string(),
        "--ctx-size".to_string(),
        config.context_size.to_string(),
    ]
}

/// Mint a bearer token for one sidecar process.
///
/// 128 bits from the OS entropy source, hex-encoded so it can be spliced into
/// an `Authorization` header and an environment variable without escaping. A new
/// one per spawn means the window in which a leaked token is worth anything
/// closes when the process does.
fn new_api_token() -> String {
    use std::fmt::Write;

    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    bytes.iter().fold(String::with_capacity(32), |mut out, b| {
        // Writing into a String cannot fail.
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Spawn a long-lived event-drain task for a sidecar. Returns once the
/// readiness needle appears on stderr (or hits a timeout/crash) and
/// keeps draining for the rest of the sidecar's lifetime.
///
/// # Why not just `wait_for_ready` and drop the receiver
///
/// Consuming and dropping the `Receiver` after startup loses later output.
/// Tauri-plugin-shell's stderr/stdout pipes are bounded mpsc channels
/// — once nobody's reading the receiver, the channel fills up and
/// the spawn-side task that produces events stops. Losing that side
/// channel meant runtime errors, slow-token warnings, GGUF metadata,
/// and crash logs from llama-server all silently disappeared from
/// our `tracing` pipeline.
///
/// The drain task:
/// - Reads every `CommandEvent` for the lifetime of the sidecar.
/// - Pipes each stderr/stdout line into `tracing` with
///   `target = "llama_server"` so user diagnostic exports include
///   them.
/// - Until readiness, also appends each line to `tail`, which the
///   caller turns into the startup error if readiness never comes.
/// - Detects `Terminated` mid-flight and clears the child slot in
///   `child_arc` so `SidecarRegistry::kill_all` doesn't try to kill
///   a dead PID at shutdown.
///
/// # Readiness handshake
///
/// Returns a `oneshot::Receiver<Result<(), StartupEnd>>` so the caller
/// can `await` readiness while the drain task owns the
/// `CommandEvent` stream. The oneshot fires exactly once:
///
/// - `Ok(())` when stderr emits the readiness needle, or
/// - `Err(...)` saying how the sidecar ended before readiness.
///
/// tauri-plugin-shell only sends `Terminated` after both pipe readers
/// have finished, so the tail is complete when an exit is reported.
///
/// After that signal, the drain keeps running for stderr capture
/// even though the caller has moved on.
fn spawn_event_drain(
    rx: Receiver<CommandEvent>,
    endpoint: String,
    child_arc: Arc<SyncMutex<Option<CommandChild>>>,
    tail: Arc<SyncMutex<OutputTail>>,
    signals: Arc<StartupSignals>,
) -> tokio::sync::oneshot::Receiver<Result<(), StartupEnd>> {
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let mut rx = rx;
        let mut ready_tx = Some(ready_tx);

        // Fires the readiness oneshot exactly once. After readiness is
        // signaled we keep draining; the caller has moved on and ignores
        // further outcomes from this task.
        let signal_ready =
            |ready_tx: &mut Option<tokio::sync::oneshot::Sender<Result<(), StartupEnd>>>,
             result: Result<(), StartupEnd>| {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(result);
                }
            };

        while let Some(event) = rx.recv().await {
            let readiness_pending = ready_tx.is_some();
            // Both streams get the same treatment. The pinned build prints
            // everything, readiness line included, on stderr and nothing on
            // stdout — but that is an upstream choice, not a contract.
            let on_line = |trimmed: &str, prefix: &str| {
                if trimmed.is_empty() {
                    return false;
                }
                tracing::debug!(target: "llama_server", "{prefix}{trimmed}");
                if readiness_pending {
                    tail.lock().push(trimmed);
                    signals.observe(trimmed);
                    return trimmed.contains(READY_NEEDLE);
                }
                false
            };
            match event {
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    if on_line(line.trim_end(), "") {
                        tracing::info!("llama-server sidecar ready on {endpoint}");
                        signal_ready(&mut ready_tx, Ok(()));
                    }
                }
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    if on_line(line.trim_end(), "stdout: ") {
                        tracing::info!("llama-server sidecar ready on {endpoint}");
                        signal_ready(&mut ready_tx, Ok(()));
                    }
                }
                CommandEvent::Error(err) => {
                    if readiness_pending {
                        signal_ready(&mut ready_tx, Err(StartupEnd::EventError(err)));
                    } else {
                        // Mid-flight error post-readiness. Log it; the
                        // request layer will surface its own HTTP error
                        // when the next call fails.
                        tracing::error!(
                            target: "llama_server",
                            endpoint = %endpoint,
                            "llama-server sidecar emitted error event: {err}"
                        );
                    }
                }
                CommandEvent::Terminated(payload) => {
                    if readiness_pending {
                        signal_ready(
                            &mut ready_tx,
                            Err(StartupEnd::Exited {
                                code: payload.code,
                                signal: payload.signal,
                            }),
                        );
                    } else {
                        tracing::error!(
                            target: "llama_server",
                            endpoint = %endpoint,
                            code = ?payload.code,
                            signal = ?payload.signal,
                            "llama-server sidecar terminated unexpectedly"
                        );
                    }
                    // Clear the child slot so kill_all at shutdown
                    // doesn't try to kill a dead PID. The Arc itself
                    // stays alive — owned by SidecarHandle — but the
                    // CommandChild inside is gone.
                    let _ = child_arc.lock().take();
                    // Channel will close after this; loop exits.
                }
                _ => {}
            }
        }

        // Receiver closed without us seeing Terminated — the spawn
        // side task ended without emitting a final event. Treat that
        // as a silent crash if we were still waiting on readiness.
        signal_ready(&mut ready_tx, Err(StartupEnd::StreamClosed));

        tracing::debug!(
            target: "llama_server",
            endpoint = %endpoint,
            "Drain task exiting (channel closed)"
        );
    });

    ready_rx
}

/// One `GET /health` answer. llama-server answers 503 while it is still
/// reading weights and 200 once it can serve, which makes it the only
/// readiness signal that cannot be invalidated by a log reformat upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HealthProbe {
    Ready,
    /// Answered, but not ready yet — progress all the same.
    Loading,
    /// Nothing listening yet, or the probe timed out.
    Unreachable,
}

async fn probe_health(http: &reqwest::Client, endpoint: &str, api_token: &str) -> HealthProbe {
    // Current llama.cpp exempts `/health` from the API-key check, but an
    // unauthenticated probe would turn any future tightening of that list into a
    // model load that never finishes starting.
    let request = http
        .get(format!("{endpoint}/health"))
        .bearer_auth(api_token)
        .send();
    match timeout(HEALTH_PROBE_TIMEOUT, request).await {
        Ok(Ok(response)) if response.status().is_success() => HealthProbe::Ready,
        Ok(Ok(_)) => HealthProbe::Loading,
        _ => HealthProbe::Unreachable,
    }
}

/// Wait until the sidecar can serve: `/health` answering 200, or the readiness
/// line on its output. The drain task is unaffected by this wait — it keeps
/// running regardless.
///
/// What is bounded is *silence*, not duration. A flat per-attempt timeout was
/// simultaneously too short (a 20 GB model needs minutes just to be read off a
/// cold page cache, and was killed and blamed on the model file) and, times
/// the fallback ladder, far too long (three attempts of spinner before any
/// error). So: the idle bound resets on every output line and every 503, an
/// absolute cap bounds one attempt, and `deadline` bounds the whole start
/// across every fallback attempt.
async fn await_ready(
    ready_rx: tokio::sync::oneshot::Receiver<Result<(), StartupEnd>>,
    endpoint: &str,
    api_token: &str,
    signals: &StartupSignals,
    deadline: Instant,
) -> Result<(), StartupEnd> {
    let mut ready_rx = ready_rx;
    // Without a client we still have the log line and the exit signal; a
    // failure to build one must not become a panic on a model load.
    let http = match reqwest::Client::builder().build() {
        Ok(client) => Some(client),
        Err(err) => {
            tracing::warn!(
                endpoint = %endpoint,
                error = %err,
                "No HTTP client for readiness polling; falling back to the log line alone"
            );
            None
        }
    };

    let cap = Instant::now() + READINESS_ATTEMPT_CAP;
    let mut idle_deadline = Instant::now() + READINESS_IDLE_TIMEOUT;
    let mut seen_lines = signals.line_count();

    loop {
        tokio::select! {
            outcome = &mut ready_rx => {
                return match outcome {
                    Ok(result) => result,
                    // Sender dropped without sending — drain task panicked
                    // or exited unexpectedly.
                    Err(_) => Err(StartupEnd::MonitorGone),
                };
            }
            _ = tokio::time::sleep(HEALTH_POLL_INTERVAL) => {}
        }

        if let Some(http) = http.as_ref() {
            match probe_health(http, endpoint, api_token).await {
                HealthProbe::Ready => {
                    tracing::info!("llama-server sidecar answered /health on {endpoint}");
                    return Ok(());
                }
                HealthProbe::Loading => idle_deadline = Instant::now() + READINESS_IDLE_TIMEOUT,
                HealthProbe::Unreachable => {}
            }
        }

        let lines = signals.line_count();
        if lines != seen_lines {
            seen_lines = lines;
            idle_deadline = Instant::now() + READINESS_IDLE_TIMEOUT;
        }

        let now = Instant::now();
        if now >= deadline {
            return Err(StartupEnd::TimedOut(TimeoutKind::Budget(
                STARTUP_TOTAL_BUDGET.as_secs(),
            )));
        }
        if now >= cap {
            return Err(StartupEnd::TimedOut(TimeoutKind::Cap(
                READINESS_ATTEMPT_CAP.as_secs(),
            )));
        }
        if now >= idle_deadline {
            return Err(StartupEnd::TimedOut(TimeoutKind::Silence(
                READINESS_IDLE_TIMEOUT.as_secs(),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_shell::process::TerminatedPayload;

    #[test]
    fn reserve_free_port_returns_nonzero_loopback_port() {
        let reservation = reserve_free_port().expect("port allocation failed");
        assert!(
            reservation.port() > 0,
            "expected a non-zero port, got {}",
            reservation.port()
        );
        let port = reservation.port();
        assert!(RESERVED_PORTS.lock().contains(&port));
        drop(reservation);
        assert!(!RESERVED_PORTS.lock().contains(&port));
    }

    #[test]
    fn reserve_free_port_never_hands_out_a_live_reservation() {
        // The OS is free to hand back the same just-released ephemeral port;
        // the reservation set is what stops two sidecars sharing it.
        let first = reserve_free_port().expect("first allocation");
        let second = reserve_free_port().expect("second allocation");
        assert_ne!(first.port(), second.port());
    }

    #[test]
    fn sidecar_config_for_model_uses_gpu_defaults() {
        let cfg = SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"));
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.context_size, 8192);
    }

    #[test]
    fn without_gpu_offload_disables_gpu_and_caps_context() {
        let gpu = SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"));
        let cpu = gpu.without_gpu_offload();
        assert_eq!(cpu.n_gpu_layers, 0);
        assert_eq!(cpu.context_size, DEFAULT_CPU_CONTEXT_SIZE);
        assert_eq!(cpu.model_path, gpu.model_path);

        // A low-RAM window stays low rather than growing to the CPU default.
        let low_ram = SidecarConfig {
            context_size: LOW_RAM_CONTEXT_SIZE,
            ..gpu
        };
        assert_eq!(
            low_ram.without_gpu_offload().context_size,
            LOW_RAM_CONTEXT_SIZE
        );
    }

    fn make_caps(
        gpu_vendor: Option<crate::features::llm::engine::system::GPUVendor>,
        ram_gb: f64,
    ) -> SystemCapabilities {
        use crate::features::llm::engine::system::{GPUInfo, Platform};
        SystemCapabilities {
            total_ram_gb: ram_gb,
            available_ram_gb: ram_gb,
            gpu: gpu_vendor.map(|vendor| GPUInfo {
                vendor,
                name: format!("{:?} test GPU", vendor),
                vram_gb: None,
                compute_capability: None,
            }),
            cpu_cores: 8,
            cpu_threads: 16,
            cpu_model: Some("test cpu".to_string()),
            cpu_architecture: Some("x86_64".to_string()),
            platform: Platform::Linux,
            os_version: None,
        }
    }

    #[test]
    fn from_capabilities_apple_silicon_uses_full_offload() {
        use crate::features::llm::engine::system::GPUVendor;
        let caps = make_caps(Some(GPUVendor::Apple), 16.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.context_size, DEFAULT_GPU_CONTEXT_SIZE);
    }

    #[test]
    fn from_capabilities_nvidia_uses_full_offload() {
        use crate::features::llm::engine::system::GPUVendor;
        let caps = make_caps(Some(GPUVendor::Nvidia), 32.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.context_size, DEFAULT_GPU_CONTEXT_SIZE);
    }

    #[test]
    fn from_capabilities_no_gpu_uses_cpu() {
        let caps = make_caps(None, 16.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        assert_eq!(cfg.n_gpu_layers, 0);
        assert_eq!(cfg.context_size, DEFAULT_CPU_CONTEXT_SIZE);
    }

    #[test]
    fn from_capabilities_unknown_gpu_treated_as_no_acceleration() {
        use crate::features::llm::engine::system::GPUVendor;
        let caps = make_caps(Some(GPUVendor::Unknown), 16.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        // Unknown vendor → is_accelerated() returns false → CPU path
        assert_eq!(cfg.n_gpu_layers, 0);
    }

    #[test]
    fn from_capabilities_low_ram_shrinks_context() {
        use crate::features::llm::engine::system::GPUVendor;
        // GPU present + low RAM: still no GPU offload concern, but
        // context shrinks to keep KV cache manageable.
        let caps = make_caps(Some(GPUVendor::Apple), 4.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.context_size, LOW_RAM_CONTEXT_SIZE);
    }

    #[test]
    fn from_capabilities_low_ram_no_gpu_squeezes_both() {
        let caps = make_caps(None, 4.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        assert_eq!(cfg.n_gpu_layers, 0);
        assert_eq!(cfg.context_size, LOW_RAM_CONTEXT_SIZE);
    }

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    fn exited(code: Option<i32>, signal: Option<i32>) -> StartupEnd {
        StartupEnd::Exited { code, signal }
    }

    /// The readiness wait's silence bound, as the classifier sees it.
    const TIMED_OUT: StartupEnd = StartupEnd::TimedOut(TimeoutKind::Silence(90));

    /// Classify using the signals the drain task would have recorded from the
    /// same output, so tests exercise the sticky path production uses.
    fn classify(binary: SidecarBinary, end: &StartupEnd, output: &[String]) -> AttemptError {
        startup_failure(binary, end, output, StartupFacts::scan(output))
    }

    const BROKEN: AttemptFailure = AttemptFailure::BinaryUnusable(UnusableReason::BrokenInstall);

    /// Captured verbatim from the CI-built macOS binary that linked
    /// against dylibs which only existed on the build machine.
    const DYLD_LIBRARY_NOT_LOADED: &str = "\
dyld[1006]: Library not loaded: @rpath/libllama-common.0.dylib
  Referenced from: <E07A64A7-7995-3D31-9684-B8F16EF758A6> /Users/josh/Code/lattice-temp/src-tauri/binaries/llama-server-aarch64-apple-darwin
  Reason: tried: '/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file), '/System/Volumes/Preboot/Cryptexes/OS/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file), '/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file), '/System/Volumes/Preboot/Cryptexes/OS/Users/runner/work/lattice/lattice/llama.cpp/build/bin/libllama-common.0.dylib' (no such file)";

    const DYLD_SYMBOL_NOT_FOUND: &str = "\
dyld[4242]: Symbol not found: _ggml_backend_dev_by_type
  Referenced from: <1B2C3D4E-0000-1111-2222-333344445555> /Applications/Lattice.app/Contents/MacOS/llama-server
  Expected in:     <99887766-5544-3322-1100-AABBCCDDEEFF> /Applications/Lattice.app/Contents/Frameworks/libggml-base.dylib";

    const LINUX_MISSING_VULKAN_LOADER: &str = "/usr/lib/lattice/llama-server: error while loading shared libraries: libvulkan.so.1: cannot open shared object file: No such file or directory";

    const LINUX_OLD_GLIBC: &str = "/usr/lib/lattice/llama-server: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by /usr/lib/lattice/llama-server)";

    const LINUX_OLD_LIBSTDCXX: &str = "/usr/lib/lattice/llama-server: /lib/x86_64-linux-gnu/libstdc++.so.6: version `GLIBCXX_3.4.32' not found (required by /usr/lib/lattice/llama-server)";

    const CORRUPT_MODEL: &str = "\
build: 6500 (d775992) with cc (GCC) 13.2.0 for x86_64-linux-gnu
system info: n_threads = 8, n_threads_batch = 8, total_threads = 16
main: binding port with default address family
main: loading model
srv    load_model: loading model '/models/broken.gguf'
gguf_init_from_file_impl: invalid magic characters: 'lmth', expected 'GGUF'
llama_model_load: error loading model: llama_model_loader: failed to load model from /models/broken.gguf
llama_model_load_from_file_impl: failed to load model
srv    load_model: failed to load model, '/models/broken.gguf'
main: exiting due to model loading error";

    const VULKAN_DEVICE_LOST: &str = "\
build: 6500 (d775992) with cc (GCC) 13.2.0 for x86_64-linux-gnu
ggml_vulkan: Found 1 Vulkan devices:
ggml_vulkan: 0 = Intel(R) UHD Graphics 620 (Intel Corporation) | uma: 1 | fp16: 1 | bf16: 0 | warp: 32 | shared memory: 1 | int dot: 0 | matrix cores: none
terminate called after throwing an instance of 'vk::DeviceLostError'
  what():  vk::Device::waitForFences: ErrorDeviceLost";

    const BUILD_BANNER: &str =
        "build: 6500 (d775992) with MSVC 19.44.35211.0 for x64\nsystem info: n_threads = 8";

    #[test]
    fn dyld_library_not_loaded_is_binary_unusable() {
        let output = lines(DYLD_LIBRARY_NOT_LOADED);
        let err = classify(SidecarBinary::Primary, &exited(None, Some(6)), &output);
        assert_eq!(err.kind, BROKEN);
        assert!(
            err.message.starts_with(
                "Lattice's bundled llama-server can't run on this machine: \
                 dyld[1006]: Library not loaded: @rpath/libllama-common.0.dylib. \
                 Reinstall Lattice."
            ),
            "{}",
            err.message
        );
        // The path to the fetch script is developer tooling, never user advice.
        assert_eq!(
            err.message.contains("fetch-llama-binaries.sh"),
            cfg!(debug_assertions)
        );
        // The tail rides along for diagnostics, clipped per line.
        assert!(err.message.contains("llama-server output (last 3 lines):"));
        assert!(err.message.contains("Reason: tried: '/Users/runner/work/"));
        assert!(!err.message.contains("GGUF"), "must not blame the model");
    }

    #[test]
    fn loader_and_exec_failures_are_binary_unusable() {
        let cases: &[(&str, Option<i32>, Option<i32>, &str)] = &[
            (
                DYLD_SYMBOL_NOT_FOUND,
                None,
                Some(6),
                "dyld[4242]: Symbol not found: _ggml_backend_dev_by_type",
            ),
            (
                LINUX_MISSING_VULKAN_LOADER,
                Some(127),
                None,
                "error while loading shared libraries: libvulkan.so.1",
            ),
            (LINUX_OLD_GLIBC, Some(1), None, "version `GLIBC_2.38' not found"),
            (
                LINUX_OLD_LIBSTDCXX,
                Some(1),
                None,
                "version `GLIBCXX_3.4.32' not found",
            ),
            (
                "bash: ./llama-server: cannot execute binary file: Exec format error",
                Some(126),
                None,
                "cannot execute binary file",
            ),
            (
                "/usr/lib/lattice/llama-server: symbol lookup error: /usr/lib/lattice/libggml-vulkan.so: undefined symbol: ggml_backend_buffer_init",
                Some(127),
                None,
                "undefined symbol: ggml_backend_buffer_init",
            ),
        ];
        for (output, code, signal, expected_reason) in cases {
            let evidence =
                binary_unusable_reason(SidecarBinary::Primary, *code, *signal, &lines(output))
                    .unwrap_or_else(|| panic!("not classified as unusable: {output}"));
            assert!(
                evidence.detail.contains(expected_reason),
                "detail {:?} lacks {expected_reason:?}",
                evidence.detail
            );
            assert!(evidence.conclusive, "loader evidence is deterministic");
            let err = classify(
                SidecarBinary::Primary,
                &exited(*code, *signal),
                &lines(output),
            );
            assert!(
                matches!(err.kind, AttemptFailure::BinaryUnusable(_)),
                "{output}"
            );
        }
    }

    #[test]
    fn windows_loader_exit_codes_are_binary_unusable() {
        let cases = [
            (0xC000_0135_u32, "STATUS_DLL_NOT_FOUND"),
            (0xC000_0139_u32, "STATUS_ENTRYPOINT_NOT_FOUND"),
            (0xC000_007B_u32, "STATUS_INVALID_IMAGE_FORMAT"),
            (0xC000_001D_u32, "AVX2"),
        ];
        for (status, expected) in cases {
            // The loader fails before the program prints anything.
            let err = classify(SidecarBinary::Cpu, &exited(Some(status as i32), None), &[]);
            assert!(matches!(err.kind, AttemptFailure::BinaryUnusable(_)));
            assert!(err.message.contains(expected), "{}", err.message);
            assert!(err
                .message
                .starts_with("Lattice's bundled llama-server-cpu can't run on this machine: "));
        }
        // A missing DLL is the graphics driver's business on the GPU build and
        // our own on the CPU build, and each gets its own advice.
        let gpu = classify(
            SidecarBinary::Primary,
            &exited(Some(0xC000_0135_u32 as i32), None),
            &[],
        );
        assert_eq!(
            gpu.kind,
            AttemptFailure::BinaryUnusable(UnusableReason::MissingGpuRuntime)
        );
        assert!(gpu.message.contains("Update your graphics driver"));
        assert!(!gpu.message.contains("Reinstall Lattice"));
        assert!(windows_loader_status(SidecarBinary::Primary, 1).is_none());
        // STATUS_ACCESS_VIOLATION is a crash inside the program, not a loader failure.
        assert!(windows_loader_status(SidecarBinary::Primary, 0xC000_0005_u32 as i32).is_none());
    }

    #[test]
    fn unix_signals_that_mean_the_binary_cannot_run() {
        // SIGILL after the banner: the CPU lacks an instruction set.
        let primary = SidecarBinary::Primary;
        let evidence = binary_unusable_reason(primary, None, Some(SIGILL), &lines(BUILD_BANNER))
            .expect("SIGILL is unusable");
        assert!(evidence.detail.contains("AVX2"));
        assert_eq!(evidence.reason, UnusableReason::UnsupportedCpu);
        // SIGKILL before any output: the OS refused to run it — but memory
        // pressure looks the same, so the verdict must not be cached.
        let signal_only =
            binary_unusable_reason(primary, None, Some(SIGKILL), &[]).expect("SIGKILL is unusable");
        assert!(
            !signal_only.conclusive,
            "signal-only evidence is inconclusive"
        );
        // SIGKILL after output is an OOM kill or similar, not the binary.
        assert!(
            binary_unusable_reason(primary, None, Some(SIGKILL), &lines(BUILD_BANNER)).is_none()
        );
        // A plain abort without a loader message says nothing about the binary.
        assert!(binary_unusable_reason(primary, None, Some(6), &lines(BUILD_BANNER)).is_none());
    }

    #[test]
    fn corrupt_model_is_a_startup_failure_not_a_binary_failure() {
        let output = lines(CORRUPT_MODEL);
        let err = classify(SidecarBinary::Primary, &exited(Some(1), None), &output);
        assert_eq!(err.kind, AttemptFailure::Startup);
        assert!(err.message.starts_with(
            "llama-server exited during startup (exit code 1). The model file may be corrupt"
        ));
        assert!(err.message.contains("invalid magic characters"));
        assert!(matches!(
            err.into_llm_error(),
            LLMError::GenerationFailed(_)
        ));
    }

    #[test]
    fn gpu_backend_failures_are_classified_as_gpu_init() {
        let err = classify(
            SidecarBinary::Primary,
            &exited(None, Some(6)),
            &lines(VULKAN_DEVICE_LOST),
        );
        assert_eq!(err.kind, AttemptFailure::GpuInit);
        assert!(err.message.contains("GPU backend"));

        // Windows: abort() surfaces as STATUS_STACK_BUFFER_OVERRUN.
        let err = classify(
            SidecarBinary::Primary,
            &exited(Some(0xC000_0409_u32 as i32), None),
            &lines("ggml_vulkan: Device memory allocation of size 1073741824 failed."),
        );
        assert_eq!(err.kind, AttemptFailure::GpuInit);
        assert!(
            err.message.contains("exit code 0xC0000409"),
            "{}",
            err.message
        );

        // A crash or hang before the model loader spoke happened in backend init.
        for end in [exited(None, Some(11)), TIMED_OUT] {
            let err = classify(SidecarBinary::Primary, &end, &lines(BUILD_BANNER));
            assert_eq!(err.kind, AttemptFailure::GpuInit, "{end:?}");
        }
    }

    #[test]
    fn failures_after_model_load_began_are_startup_failures() {
        let mut output = lines(BUILD_BANNER);
        output.push("llama_model_loader: loaded meta data with 30 key-value pairs".to_string());
        for end in [
            exited(None, Some(11)),
            exited(None, Some(SIGKILL)),
            TIMED_OUT,
            StartupEnd::StreamClosed,
            StartupEnd::EventError("wait failed".to_string()),
        ] {
            let err = classify(SidecarBinary::Primary, &end, &output);
            assert_eq!(err.kind, AttemptFailure::Startup, "{end:?}");
        }
        // An orderly non-zero exit is not a crash, banner or not.
        let err = classify(SidecarBinary::Primary, &exited(Some(1), None), &[]);
        assert_eq!(err.kind, AttemptFailure::Startup);
        assert!(err.message.contains("exit code 1"));
    }

    #[test]
    fn normal_vulkan_banner_is_not_a_gpu_failure() {
        let banner = lines(
            "ggml_vulkan: Found 1 Vulkan devices:\n\
             ggml_vulkan: 0 = NVIDIA GeForce RTX 3060 (NVIDIA) | uma: 0 | fp16: 1 | bf16: 0 | warp: 32 | shared memory: 0 | int dot: 1 | matrix cores: KHR_coopmat\n\
             ggml_metal_device_init: GPU name:   MTL0 (Apple M3 Max)",
        );
        assert!(!looks_like_gpu_failure(&banner));
        assert!(!looks_like_gpu_failure(&lines(CORRUPT_MODEL)));
    }

    #[test]
    fn spawn_failures_are_binary_unusable_and_name_the_path() {
        let not_found = tauri_plugin_shell::Error::Io(std::io::ErrorKind::NotFound.into());
        let err = spawn_failure(SidecarBinary::Cpu, &not_found);
        assert_eq!(err.kind, BROKEN);
        let expected = resolved_sidecar_path(SidecarBinary::Cpu);
        assert!(
            err.message.starts_with(&format!(
                "Lattice's bundled llama-server-cpu can't run on this machine: {} does not exist. \
                 Reinstall Lattice",
                expected.display()
            )),
            "{}",
            err.message
        );
        assert!(matches!(
            err.into_llm_error(),
            LLMError::SidecarBinaryUnusable(_)
        ));

        let denied = tauri_plugin_shell::Error::Io(std::io::ErrorKind::PermissionDenied.into());
        let err = spawn_failure(SidecarBinary::Primary, &denied);
        assert_eq!(err.kind, BROKEN);
        assert!(err.message.contains("can't run on this machine: starting "));
        assert!(err
            .message
            .contains("llama-server failed: permission denied"));
    }

    #[test]
    fn resolved_sidecar_path_is_the_installed_name_next_to_the_executable() {
        let exe_dir = std::env::current_exe()
            .expect("current exe")
            .parent()
            .expect("exe dir")
            .to_path_buf();
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        assert_eq!(
            resolved_sidecar_path(SidecarBinary::Primary),
            exe_dir.join(format!("llama-server{suffix}"))
        );
        assert_eq!(
            resolved_sidecar_path(SidecarBinary::Cpu),
            exe_dir.join(format!("llama-server-cpu{suffix}"))
        );
    }

    const GPU: Attempt = Attempt {
        binary: SidecarBinary::Primary,
        gpu_offload: true,
    };
    const PRIMARY_CPU: Attempt = Attempt {
        binary: SidecarBinary::Primary,
        gpu_offload: false,
    };
    const CPU_BUILD: Attempt = Attempt {
        binary: SidecarBinary::Cpu,
        gpu_offload: false,
    };

    #[test]
    fn fallback_policy_with_cpu_build_bundled() {
        use AttemptFailure::*;
        let table = [
            (GPU, BROKEN, Some(CPU_BUILD)),
            (GPU, GpuInit, Some(PRIMARY_CPU)),
            (GPU, Startup, Some(PRIMARY_CPU)),
            // A port collision is retried by the caller, not degraded here.
            (GPU, Retry, None),
            (GPU, Fatal, None),
            (PRIMARY_CPU, BROKEN, Some(CPU_BUILD)),
            (PRIMARY_CPU, GpuInit, Some(CPU_BUILD)),
            (PRIMARY_CPU, Startup, None),
            (PRIMARY_CPU, Retry, None),
            (PRIMARY_CPU, Fatal, None),
            (CPU_BUILD, BROKEN, None),
            (CPU_BUILD, GpuInit, None),
            (CPU_BUILD, Startup, None),
            (CPU_BUILD, Retry, None),
            (CPU_BUILD, Fatal, None),
        ];
        for (failed, failure, expected) in table {
            assert_eq!(
                next_attempt(failed, failure, true),
                expected,
                "{failed:?} / {failure:?}"
            );
        }
    }

    #[test]
    fn fallback_policy_without_cpu_build() {
        use AttemptFailure::*;
        let table = [
            // The incident: no same-binary retry for a binary that cannot run.
            (GPU, BROKEN, None),
            (GPU, GpuInit, Some(PRIMARY_CPU)),
            (GPU, Startup, Some(PRIMARY_CPU)),
            (GPU, Retry, None),
            (GPU, Fatal, None),
            (PRIMARY_CPU, BROKEN, None),
            (PRIMARY_CPU, GpuInit, None),
            (PRIMARY_CPU, Startup, None),
            (PRIMARY_CPU, Retry, None),
            (PRIMARY_CPU, Fatal, None),
        ];
        for (failed, failure, expected) in table {
            assert_eq!(
                next_attempt(failed, failure, false),
                expected,
                "{failed:?} / {failure:?}"
            );
        }
    }

    #[test]
    fn fallback_chain_always_terminates_without_repeating() {
        use AttemptFailure::*;
        let failures = [BROKEN, GpuInit, Startup, Retry, Fatal];
        for bundled in [true, false] {
            for start in [GPU, PRIMARY_CPU] {
                // Every sequence of failures, three deep (the longest chain).
                for a in failures {
                    for b in failures {
                        for c in failures {
                            let mut seen = vec![start];
                            let mut current = start;
                            for failure in [a, b, c] {
                                match next_attempt(current, failure, bundled) {
                                    Some(next) => {
                                        assert!(!seen.contains(&next), "repeat of {next:?}");
                                        seen.push(next);
                                        current = next;
                                    }
                                    None => break,
                                }
                            }
                            assert!(seen.len() <= 3);
                            if !bundled {
                                assert!(!seen.contains(&CPU_BUILD));
                            }
                        }
                    }
                }
            }
        }
    }

    fn attempt_error(kind: AttemptFailure, message: &str) -> AttemptError {
        AttemptError {
            kind,
            message: message.to_string(),
        }
    }

    #[test]
    fn reported_error_prefers_binary_unusable_and_summarizes_the_rest() {
        let err = reported_error(vec![
            (
                GPU,
                attempt_error(AttemptFailure::GpuInit, "llama-server crashed\n\noutput"),
            ),
            (
                PRIMARY_CPU,
                attempt_error(AttemptFailure::GpuInit, "llama-server crashed again"),
            ),
            (
                CPU_BUILD,
                attempt_error(
                    AttemptFailure::BinaryUnusable(UnusableReason::UnsupportedCpu),
                    "Lattice's bundled llama-server-cpu can't run on this machine: AVX2",
                ),
            ),
        ]);
        let message = match err {
            LLMError::SidecarBinaryUnusable(message) => message,
            other => panic!("expected SidecarBinaryUnusable, got {other:?}"),
        };
        assert!(message.starts_with("Lattice's bundled llama-server-cpu can't run"));
        assert!(message.contains("\n- llama-server (gpu): llama-server crashed\n"));
        assert!(message.ends_with("\n- llama-server (cpu): llama-server crashed again"));
        assert!(
            !message.contains("output"),
            "only first lines are summarized"
        );
    }

    #[test]
    fn reported_error_falls_back_to_the_requested_configuration() {
        let err = reported_error(vec![
            (GPU, attempt_error(AttemptFailure::Startup, "first")),
            (
                PRIMARY_CPU,
                attempt_error(AttemptFailure::Startup, "second"),
            ),
        ]);
        assert!(
            matches!(&err, LLMError::GenerationFailed(m) if m.starts_with("first\n\nOther startup attempts:"))
        );

        let single = reported_error(vec![(
            GPU,
            attempt_error(AttemptFailure::Fatal, "no model"),
        )]);
        assert!(matches!(&single, LLMError::Other(m) if m == "no model"));

        assert!(matches!(reported_error(Vec::new()), LLMError::Other(_)));
    }

    #[test]
    fn output_tail_is_bounded_by_lines_and_bytes() {
        let mut tail = OutputTail::default();
        for i in 0..100 {
            tail.push(&format!("line {i}"));
        }
        let lines = tail.snapshot();
        assert_eq!(lines.len(), OUTPUT_TAIL_MAX_LINES);
        assert_eq!(lines.last().map(String::as_str), Some("line 99"));

        let mut tail = OutputTail::default();
        let long = "x".repeat(OUTPUT_LINE_MAX_BYTES * 3);
        for _ in 0..10 {
            tail.push(&long);
        }
        let lines = tail.snapshot();
        assert!(lines.iter().all(|l| l.len() == OUTPUT_LINE_MAX_BYTES));
        assert!(lines.iter().map(String::len).sum::<usize>() <= OUTPUT_TAIL_MAX_BYTES);
        assert_eq!(tail.bytes, lines.iter().map(String::len).sum::<usize>());
    }

    #[test]
    fn render_tail_keeps_the_last_lines() {
        assert_eq!(render_tail(&[]), "");
        assert_eq!(render_tail(&lines("\n  \n")), "");
        let output: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        let rendered = render_tail(&output);
        assert!(rendered.starts_with("\n\nllama-server output (last 12 lines):\n  line 8\n"));
        assert!(rendered.ends_with("\n  line 19"));
        assert!(!rendered.contains("line 7\n"));
    }

    #[test]
    fn clip_respects_char_boundaries() {
        assert_eq!(clip("hello", 10), "hello");
        assert_eq!(clip("hello", 3), "hel");
        // 'é' is two bytes; cutting inside it backs off to the boundary.
        assert_eq!(clip("aé", 2), "a");
    }

    #[test]
    fn describe_exit_formats_ntstatus_as_hex() {
        assert_eq!(
            describe_exit(Some(0xC000_0135_u32 as i32), None),
            "exit code 0xC0000135"
        );
        assert_eq!(describe_exit(Some(1), None), "exit code 1");
        assert_eq!(describe_exit(None, Some(6)), "signal 6");
        assert_eq!(describe_exit(None, None), "no exit status");
    }

    #[test]
    fn reaper_matches_only_installed_sidecars_next_to_lattice() {
        let dir = std::path::Path::new("/Applications/Lattice.app/Contents/MacOS");
        for name in [
            "llama-server",
            "llama-server.exe",
            "LLAMA-SERVER.EXE",
            "llama-server-cpu",
            "llama-server-cpu.exe",
        ] {
            assert!(is_installed_sidecar(&dir.join(name), dir), "{name}");
        }
        for name in [
            "llama-cli",
            "llama-server-gpu",
            "llama-server-cp",
            "lattice-desktop",
        ] {
            assert!(!is_installed_sidecar(&dir.join(name), dir), "{name}");
        }
        // A developer's own build, or another app's bundled copy, elsewhere.
        for path in [
            "/Users/dev/code/llama.cpp/build/bin/llama-server",
            "/Applications/Other.app/Contents/MacOS/llama-server",
            "/Applications/Lattice.app/Contents/MacOS/binaries/llama-server",
            "llama-server",
        ] {
            assert!(
                !is_installed_sidecar(std::path::Path::new(path), dir),
                "{path}"
            );
        }
    }

    #[test]
    fn bundled_binaries_follow_the_platform() {
        let bundled = SidecarBinary::bundled();
        assert_eq!(bundled.first(), Some(&SidecarBinary::Primary));
        assert_eq!(
            bundled.contains(&SidecarBinary::Cpu),
            cfg!(any(target_os = "windows", target_os = "linux"))
        );
        assert_eq!(SidecarBinary::Primary.label(), SIDECAR_BIN);
        assert_eq!(SidecarBinary::Cpu.label(), SIDECAR_CPU_BIN);
    }

    /// `externalBin` file stems in a Tauri config (target triple is added
    /// by Tauri at build time, so the configured entry has none).
    fn external_bin_stems(file: &str) -> Vec<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let json: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
        json["bundle"]["externalBin"]
            .as_array()
            .unwrap_or_else(|| panic!("{file} has no bundle.externalBin array"))
            .iter()
            .map(|entry| {
                let entry = entry.as_str().expect("externalBin entries are strings");
                std::path::Path::new(entry)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("externalBin entry has a file name")
                    .to_string()
            })
            .collect()
    }

    /// The spawn names must be the installed stems of what the bundle
    /// ships, per platform: the primary build everywhere (the base config
    /// is what macOS uses), the CPU build on Windows and Linux, matching
    /// `SidecarBinary::CPU_BUILD_BUNDLED`.
    #[test]
    fn sidecar_names_match_the_bundle_configuration() {
        assert_eq!(external_bin_stems("tauri.conf.json"), [SIDECAR_BIN]);
        for platform_conf in ["tauri.windows.conf.json", "tauri.linux.conf.json"] {
            assert_eq!(
                external_bin_stems(platform_conf),
                [SIDECAR_BIN, SIDECAR_CPU_BIN],
                "{platform_conf}"
            );
        }
    }

    #[test]
    fn registry_starts_empty() {
        let registry = SidecarRegistry::new();
        assert_eq!(registry.live_count(), 0);
        assert_eq!(registry.kill_all(), 0);
    }

    #[test]
    fn registry_kill_all_is_idempotent_when_empty() {
        let registry = SidecarRegistry::new();
        registry.kill_all();
        registry.kill_all();
        registry.kill_all();
        assert_eq!(registry.live_count(), 0);
    }

    /// Nothing listens on this port, so `/health` never answers and the
    /// oneshot is the only thing that can resolve the wait.
    const DEAD_ENDPOINT: &str = "http://127.0.0.1:18987";

    async fn wait(
        rx: tokio::sync::oneshot::Receiver<Result<(), StartupEnd>>,
    ) -> Result<(), StartupEnd> {
        let signals = StartupSignals::default();
        await_ready(
            rx,
            DEAD_ENDPOINT,
            "token",
            &signals,
            Instant::now() + Duration::from_secs(30),
        )
        .await
    }

    /// `await_ready` should propagate the oneshot result faithfully:
    /// Ok(()) → Ok(()), Err(end) → Err(end), and a dropped sender →
    /// `MonitorGone`.
    #[tokio::test]
    async fn await_ready_propagates_ok() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(Ok(())).expect("send");
        assert!(wait(rx).await.is_ok());
    }

    #[tokio::test]
    async fn await_ready_propagates_err() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let end = StartupEnd::Exited {
            code: None,
            signal: Some(6),
        };
        tx.send(Err(end.clone())).expect("send");
        assert_eq!(wait(rx).await, Err(end));
    }

    #[tokio::test]
    async fn await_ready_handles_dropped_sender() {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), StartupEnd>>();
        drop(tx);
        assert_eq!(wait(rx).await, Err(StartupEnd::MonitorGone));
    }

    /// The whole start is bounded once, not once per fallback attempt: a
    /// deadline already in the past ends the wait immediately.
    #[tokio::test]
    async fn await_ready_stops_at_the_shared_budget() {
        let (_tx, rx) = tokio::sync::oneshot::channel::<Result<(), StartupEnd>>();
        let signals = StartupSignals::default();
        let end = await_ready(rx, DEAD_ENDPOINT, "token", &signals, Instant::now()).await;
        assert!(
            matches!(end, Err(StartupEnd::TimedOut(TimeoutKind::Budget(_)))),
            "{end:?}"
        );
    }

    /// The pinned b8981 startup, captured from the bundled macOS binary:
    /// every line on stderr, nothing on stdout.
    const B8981_STARTUP: &[&str] = &[
        "main: model loaded",
        "main: server is listening on http://127.0.0.1:18099",
        "main: starting the main loop...",
    ];

    struct DrainHarness {
        tx: tokio::sync::mpsc::Sender<CommandEvent>,
        ready: tokio::sync::oneshot::Receiver<Result<(), StartupEnd>>,
        tail: Arc<SyncMutex<OutputTail>>,
        signals: Arc<StartupSignals>,
    }

    /// The drain takes a `Receiver<CommandEvent>`, so a test can play a
    /// sidecar's whole life through a channel — no binary, no process.
    fn drain_harness() -> DrainHarness {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let tail = Arc::new(SyncMutex::new(OutputTail::default()));
        let signals = Arc::new(StartupSignals::default());
        let child = Arc::new(SyncMutex::new(None));
        let ready = spawn_event_drain(
            rx,
            DEAD_ENDPOINT.to_string(),
            child,
            Arc::clone(&tail),
            Arc::clone(&signals),
        );
        DrainHarness {
            tx,
            ready,
            tail,
            signals,
        }
    }

    async fn send(tx: &tokio::sync::mpsc::Sender<CommandEvent>, event: CommandEvent) {
        tx.send(event).await.expect("drain is listening");
    }

    fn stderr(line: &str) -> CommandEvent {
        CommandEvent::Stderr(line.as_bytes().to_vec())
    }

    async fn startup_end(harness: DrainHarness) -> Result<(), StartupEnd> {
        let DrainHarness { tx, ready, .. } = harness;
        drop(tx);
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .expect("drain reported within 2 s")
            .expect("drain did not drop the sender")
    }

    #[tokio::test]
    async fn drain_signals_ready_on_the_real_b8981_startup_log() {
        let harness = drain_harness();
        for line in B8981_STARTUP {
            send(&harness.tx, stderr(line)).await;
        }
        let ready = tokio::time::timeout(Duration::from_secs(2), harness.ready)
            .await
            .expect("readiness within 2 s")
            .expect("drain did not drop the sender");
        assert_eq!(ready, Ok(()));

        let facts = harness.signals.facts();
        assert!(
            facts.model_loader,
            "`main: model loaded` is loader progress"
        );
        assert!(!facts.gpu_failure && !facts.bind_failure);
        assert!(harness
            .tail
            .lock()
            .snapshot()
            .iter()
            .any(|line| line.contains(READY_NEEDLE)));
    }

    #[tokio::test]
    async fn drain_reports_an_exit_before_readiness_and_keeps_the_bind_signal() {
        let harness = drain_harness();
        send(
            &harness.tx,
            stderr("couldn't bind HTTP server socket, hostname: 127.0.0.1, port: 18099"),
        )
        .await;
        send(
            &harness.tx,
            CommandEvent::Terminated(TerminatedPayload {
                code: Some(1),
                signal: None,
            }),
        )
        .await;
        let tail = Arc::clone(&harness.tail);
        let signals = Arc::clone(&harness.signals);
        let end = startup_end(harness).await;
        assert_eq!(
            end,
            Err(StartupEnd::Exited {
                code: Some(1),
                signal: None
            })
        );
        // A port collision retries the same configuration; reading it as a GPU
        // fault is what used to pin the session to CPU-only at half context.
        let err = startup_failure(
            SidecarBinary::Primary,
            &end.expect_err("exited"),
            &tail.lock().snapshot(),
            signals.facts(),
        );
        assert_eq!(err.kind, AttemptFailure::Retry);
    }

    #[tokio::test]
    async fn drain_reports_a_closed_stream_before_readiness() {
        let harness = drain_harness();
        send(&harness.tx, stderr("build: 8981 (deadbee) with clang")).await;
        assert_eq!(startup_end(harness).await, Err(StartupEnd::StreamClosed));
    }

    #[tokio::test]
    async fn drain_reports_an_error_event_before_readiness() {
        let harness = drain_harness();
        send(&harness.tx, CommandEvent::Error("pipe closed".to_string())).await;
        assert_eq!(
            startup_end(harness).await,
            Err(StartupEnd::EventError("pipe closed".to_string()))
        );
    }

    /// A sticky signal survives the tail rolling over, which is the whole
    /// point: a big model prints dozens of `load_tensors:` lines after the
    /// loader banner, and classification used to read the tail.
    #[tokio::test]
    async fn drain_keeps_startup_signals_after_the_tail_rolls_over() {
        let harness = drain_harness();
        send(&harness.tx, stderr("llama_model_loader: loaded meta data")).await;
        for i in 0..(OUTPUT_TAIL_MAX_LINES * 2) {
            send(&harness.tx, stderr(&format!("load_tensors: layer {i}"))).await;
        }
        send(&harness.tx, stderr("done")).await;
        let signals = Arc::clone(&harness.signals);
        let tail = Arc::clone(&harness.tail);
        let _ = startup_end(harness).await;
        assert!(signals.facts().model_loader);
        assert!(
            !tail
                .lock()
                .snapshot()
                .iter()
                .any(|line| line.contains("llama_model_loader")),
            "the banner has rolled out of the tail, as it does in production"
        );
    }

    /// The test that would have caught a readiness needle three llama.cpp
    /// releases out of date: a binary is shipped that never prints it.
    #[test]
    fn ready_needle_is_present_in_every_bundled_binary() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return; // no binaries fetched on this machine
        };
        let mut checked = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_sidecar = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(SIDECAR_BIN));
            if !is_sidecar || !path.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            // A Git LFS pointer or placeholder is not a binary to judge.
            if bytes.len() < 1_000_000 {
                continue;
            }
            assert!(
                bytes
                    .windows(READY_NEEDLE.len())
                    .any(|window| window == READY_NEEDLE.as_bytes()),
                "{} does not contain the readiness needle {READY_NEEDLE:?}; \
                 llama.cpp reworded it and every model load will time out",
                path.display()
            );
            checked += 1;
        }
        tracing::debug!(checked, "checked bundled binaries for the readiness needle");
    }

    #[test]
    fn build_server_args_includes_required_flags() {
        let cfg = SidecarConfig::for_model(PathBuf::from("/models/llama.gguf"));
        let args = build_server_args(&cfg, 12345);

        assert!(args.iter().any(|a| a == "-m"));
        assert!(args.iter().any(|a| a == "/models/llama.gguf"));
        assert!(args.iter().any(|a| a == "--port"));
        assert!(args.iter().any(|a| a == "12345"));
        assert!(args.iter().any(|a| a == "--host"));
        assert!(args.iter().any(|a| a == "127.0.0.1"));
        assert!(args.iter().any(|a| a == "-ngl"));
        assert!(args.iter().any(|a| a == "99"));
    }

    /// An unauthenticated loopback port is reachable from every other process
    /// on the machine, browsers included, and the bundled web UI is an HTML
    /// surface we never meant to serve. The key authenticates the port, so it
    /// must not travel in argv, where `ps` hands it to those same processes.
    #[test]
    fn build_server_args_drop_the_web_ui_and_keep_the_key_out_of_argv() {
        let cfg = SidecarConfig::for_model(PathBuf::from("/models/llama.gguf"));
        let args = build_server_args(&cfg, 12345);

        assert!(args.iter().any(|a| a == "--no-webui"));
        assert!(
            !args.iter().any(|a| a == "--api-key"),
            "the key belongs in LLAMA_API_KEY, not on the command line"
        );
    }

    #[test]
    fn api_tokens_are_unguessable_and_never_repeat() {
        let first = new_api_token();
        let second = new_api_token();

        assert_eq!(first.len(), 32, "128 bits of entropy, hex-encoded");
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, second, "a token is minted per spawn");
    }
}
