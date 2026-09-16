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
//! 1. `SidecarManager::start(app, model_path)`
//!    - Picks a free ephemeral port via `TcpListener::bind("127.0.0.1:0")`.
//!    - Spawns `llama-server` via `tauri-plugin-shell` (the bundled
//!      sidecar binary, not a system-installed one).
//!    - Reads stderr until it sees `"HTTP server listening"` (the
//!      llama-server readiness signal) or hits a 60s timeout.
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
//! - It does not pick which binary variant (Vulkan vs CPU on Windows)
//!   to use; the fallback chain owns that decision.
//! - It does not download model files — that's the existing model
//!   storage layer, untouched by this migration.

use crate::features::llm::engine::system::SystemCapabilities;
use crate::features::llm::engine::types::LLMError;
use parking_lot::Mutex as SyncMutex;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

pub const SIDECAR_BIN: &str = "binaries/llama-server";
const READY_NEEDLE: &str = "HTTP server listening";
const READINESS_TIMEOUT: Duration = Duration::from_secs(180);

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

    /// CPU-only fallback configuration.
    pub fn cpu_only(model_path: PathBuf) -> Self {
        Self {
            model_path,
            n_gpu_layers: 0,
            context_size: DEFAULT_CPU_CONTEXT_SIZE,
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
}

impl SidecarHandle {
    /// Base URL of the local llama-server HTTP API, e.g.
    /// `http://127.0.0.1:53412`. Pass to `SidecarLLMClient`.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Context window, in tokens, that this sidecar was launched with.
    pub fn context_size(&self) -> u32 {
        self.context_size
    }

    /// Explicitly kill the sidecar. After this returns, the handle is
    /// inert; `Drop` becomes a no-op. Idempotent — second call is fine.
    pub fn stop(&self) {
        let mut guard = self.child.lock();
        if let Some(child) = guard.take() {
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
        // of scope mid-session. `try_lock` so we never block in Drop;
        // if the registry already grabbed the lock for shutdown, it
        // will do the kill and our take() returns None.
        if let Some(mut guard) = self.child.try_lock() {
            if let Some(c) = guard.take() {
                let _ = c.kill();
                tracing::debug!("SidecarHandle dropped; killed sidecar at {}", self.endpoint);
            }
        }
    }
}

/// Process-wide registry of live sidecars.
///
/// Held as Tauri-managed state via `app.manage()` so the app's
/// `RunEvent::ExitRequested` handler can synchronously drain it before
/// the tokio runtime tears down. Each entry is a `Weak` reference to a
/// `SidecarHandle`'s child so a normally-dropped handle releases its
/// slot without bookkeeping.
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
    /// Weak ref so a normally-dropped handle frees the slot. We GC
    /// dead entries on every register/kill_all pass; no need for a
    /// separate cleanup task.
    child: Weak<SyncMutex<Option<CommandChild>>>,
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

    /// Register a live sidecar. Called by `SidecarManager::start`
    /// after readiness confirmation. Cleans up dead weak refs from
    /// previous spawns at the same time.
    ///
    /// On Windows, also assigns the sidecar PID to the registry's
    /// Job Object so the kernel kills it automatically when our
    /// process handle closes. The PID is read from the
    /// `CommandChild` while the registry holds it locked, so we know
    /// it hasn't been killed/swapped underneath us.
    fn register(&self, child: &Arc<SyncMutex<Option<CommandChild>>>, endpoint: &str) {
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

        let mut entries = self.entries.lock();
        entries.retain(|e| e.child.strong_count() > 0);
        entries.push(RegistryEntry {
            child: Arc::downgrade(child),
            endpoint: endpoint.to_string(),
        });
        tracing::debug!(
            registry_size = entries.len(),
            endpoint = endpoint,
            "Registered sidecar in process registry"
        );
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
            let Some(child_arc) = entry.child.upgrade() else {
                continue; // already dropped; nothing to kill
            };
            // Lock the per-child mutex briefly: take the CommandChild
            // out, then release before calling kill so SidecarHandle's
            // Drop can race-safely no-op via try_lock.
            let child_opt = child_arc.lock().take();
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
        entries
            .iter()
            .filter(|e| e.child.strong_count() > 0)
            .count()
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
/// A two-stage match avoids terminating an unrelated `llama-server` instance:
///
/// 1. Process name must match `llama-server` or `llama-server.exe`.
/// 2. Argv must contain a fragment matching the bundled-sidecar
///    base name (`SIDECAR_BIN`). Tauri's `externalBin` resolves to
///    `binaries/llama-server-<target-triple>` paths, all of which
///    contain the literal `binaries/llama-server` substring. A
///    user running their own `~/code/llama.cpp/build/bin/llama-server`
///    won't match.
///
/// We don't try to be clever about this; if a user *does* happen to
/// have `binaries/llama-server` in their argv from somewhere unrelated,
/// we'll kill it. That's an acceptable false-positive surface.
///
/// Best-effort: `sysinfo` failures collapse to "no orphans found" and
/// the scan continues. We never panic from here.
pub fn reap_orphan_sidecars() {
    let mut system = sysinfo::System::new();
    system.refresh_processes();

    let target_names: &[&str] = &["llama-server", "llama-server.exe"];

    let mut reaped = 0usize;
    for (pid, process) in system.processes() {
        let name = process.name();
        if !target_names.iter().any(|t| name.eq_ignore_ascii_case(t)) {
            continue;
        }

        // Stage 2: argv match. Without this we'd kill any unrelated
        // `llama-server` process the user might have running (e.g. a
        // dev's local llama.cpp build). `sysinfo::Process::cmd()`
        // returns the full argv vector.
        let cmd = process.cmd();
        let argv_matches = cmd
            .iter()
            .any(|arg| arg.replace('\\', "/").contains(SIDECAR_BIN));
        if !argv_matches {
            tracing::debug!(
                pid = pid.as_u32(),
                process_name = name,
                "Skipping `llama-server` process — argv does not match bundled-sidecar path"
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

/// Spawns and supervises a `llama-server` sidecar.
pub struct SidecarManager;

impl SidecarManager {
    /// Spawn a llama-server sidecar with crash-fallback.
    ///
    /// When GPU offload is requested and startup crashes or times out,
    /// readiness timeout, we retry once with `n_gpu_layers = 0` —
    /// CPU-only mode. The motivating failure modes:
    ///
    /// - Vulkan driver bug crashes ggml init on Windows
    /// - Apple Silicon Metal context-creation race on cold boot
    /// - Out-of-VRAM during model weight allocation
    /// - llama.cpp Vulkan backend incompatible with old Mesa drivers
    ///
    /// All of those manifest as "child exits non-zero shortly after
    /// spawn" or "readiness needle never appears." The fallback covers
    /// them. If the second attempt *also* fails, we return the
    /// original GPU-attempt error — the CPU error is rarely more
    /// informative.
    ///
    /// On successful CPU fallback we emit a `tracing::warn!` with
    /// `target = "sidecar_fallback"` so the UI / observability layer
    /// can render a "running in CPU mode" banner.
    pub async fn start_with_fallback(
        app: &AppHandle,
        config: SidecarConfig,
    ) -> Result<SidecarHandle, LLMError> {
        let initial_ngl = config.n_gpu_layers;
        let model_path = config.model_path.clone();

        match Self::start(app, config).await {
            Ok(handle) => Ok(handle),
            Err(err) if initial_ngl > 0 && is_startup_failure(&err) => {
                tracing::warn!(
                    target: "sidecar_fallback",
                    original_error = %err,
                    "GPU sidecar startup failed; retrying with CPU-only fallback"
                );
                let cpu_config = SidecarConfig::cpu_only(model_path);
                match Self::start(app, cpu_config).await {
                    Ok(handle) => {
                        tracing::warn!(
                            target: "sidecar_fallback",
                            "Sidecar running in CPU fallback mode after GPU startup failed"
                        );
                        Ok(handle)
                    }
                    Err(_cpu_err) => {
                        // Both attempts failed. The GPU error is more
                        // likely to be useful (CPU mode rarely fails
                        // for "interesting" reasons; if it does, it's
                        // usually because the model file itself is
                        // bad, which the GPU error already surfaced).
                        tracing::error!(
                            target: "sidecar_fallback",
                            "CPU fallback also failed; surfacing original GPU error"
                        );
                        Err(err)
                    }
                }
            }
            Err(err) => Err(err),
        }
    }

    /// Spawn a llama-server sidecar bound to a free ephemeral port,
    /// loading the given model. Returns once the server prints its
    /// readiness needle on stderr (or times out).
    ///
    /// Most callers should use [`Self::start_with_fallback`] instead,
    /// which adds CPU-mode retry on GPU startup crashes.
    ///
    /// # Errors
    ///
    /// - `LLMError::Other` if no free port is available, model file is
    ///   missing, or `tauri-plugin-shell` rejects the spawn.
    /// - `LLMError::Timeout` if the sidecar fails to print
    ///   `"HTTP server listening"` within `READINESS_TIMEOUT`.
    /// - `LLMError::GenerationFailed` if the sidecar exits during
    ///   startup (typically a corrupt GGUF or unsupported architecture).
    pub async fn start(app: &AppHandle, config: SidecarConfig) -> Result<SidecarHandle, LLMError> {
        // 1. Validate the model file exists. llama-server's error path
        //    on a missing file is fine, but failing fast here gives a
        //    cleaner error surface to the UI.
        if !config.model_path.exists() {
            return Err(LLMError::Other(format!(
                "Model file not found: {}",
                config.model_path.display()
            )));
        }

        // 2. Pick a free port. The TcpListener drops at end of scope,
        //    releasing the port, and we hand the number to llama-server
        //    via --port. Race window between bind/drop and llama-server
        //    rebinding is theoretical (we're on loopback, no other
        //    process should grab a just-released ephemeral port in that
        //    sub-millisecond window) but if it ever bites we'll see a
        //    "port in use" error from llama-server's stderr and the
        //    readiness wait will time out.
        let port = pick_free_port()?;
        let endpoint = format!("http://127.0.0.1:{port}");

        // 3. Build the args. Order doesn't matter to llama-server.
        let args = build_server_args(&config, port);
        tracing::info!(
            "Spawning llama-server sidecar: model={} port={} ngl={} ctx={}",
            config.model_path.display(),
            port,
            config.n_gpu_layers,
            config.context_size,
        );

        // 4. Spawn via tauri-plugin-shell. `.sidecar()` looks the binary
        //    up in the app bundle's binaries directory.
        let (rx, child) = app
            .shell()
            .sidecar(SIDECAR_BIN)
            .map_err(|err| LLMError::Other(format!("Failed to locate sidecar binary: {err}")))?
            .args(args)
            .spawn()
            .map_err(|err| {
                LLMError::Other(format!("Failed to spawn llama-server sidecar: {err}"))
            })?;

        // 5. Wrap the child in the same Arc<SyncMutex> the success path
        //    will return. We register it BEFORE awaiting readiness so
        //    that if the wait fails (timeout, sidecar crash, port
        //    conflict), the registry's kill_all sweep at shutdown still
        //    finds and reaps it. `CommandChild::Drop` is documented as
        //    NOT killing the underlying OS process, so an unregistered
        //    child whose Arc gets dropped on the error path would
        //    become a zombie.
        let child_arc = Arc::new(SyncMutex::new(Some(child)));

        if let Some(registry) = app.try_state::<SidecarRegistry>() {
            registry.register(&child_arc, &endpoint);
        } else {
            tracing::debug!(
                "SidecarRegistry not managed by AppHandle; falling back to Drop-only cleanup"
            );
        }

        // 6. Spawn the long-lived event drain task.
        //    The drain owns the receiver for the rest of the sidecar's
        //    life — feeds stderr/stdout into tracing, detects
        //    unexpected termination, and signals readiness via a
        //    oneshot. If it fails to read events fast enough, llama-
        //    server's stderr buffer can fill up, but the channel
        //    bounds prevent unbounded memory growth.
        let ready_rx = spawn_event_drain(rx, port, endpoint.clone(), Arc::clone(&child_arc));

        // 7. Await readiness with a timeout. On failure: kill the
        //    child explicitly here, since the error path doesn't
        //    return a SidecarHandle (which is the type whose Drop
        //    kills) and CommandChild::Drop doesn't kill on its own.
        if let Err(err) = await_ready(ready_rx).await {
            tracing::warn!(
                endpoint = %endpoint,
                error = %err,
                "Sidecar startup failed; killing child to avoid zombie"
            );
            let child_opt = child_arc.lock().take();
            if let Some(c) = child_opt {
                let _ = c.kill();
            }
            return Err(err);
        }

        // Recorded explicitly so a mismatch between what the sidecar runs and
        // what prompt budgeting assumes is visible in the log rather than
        // showing up as mysteriously truncated context on smaller machines.
        tracing::info!(
            context_size = config.context_size,
            "llama-server ready; prompt budgeting will use this context window"
        );

        Ok(SidecarHandle {
            endpoint,
            child: child_arc,
            context_size: config.context_size,
        })
    }
}

fn is_startup_failure(err: &LLMError) -> bool {
    matches!(err, LLMError::Timeout | LLMError::GenerationFailed(_))
}

/// Picks an ephemeral TCP port by binding to `127.0.0.1:0`, reading
/// back the OS-assigned port number, and immediately dropping the
/// listener. The port is racy — a different process could theoretically
/// claim it before llama-server rebinds — but in practice this is the
/// industry-standard idiom (Cargo, rustc test runners, dev servers all
/// do this) and the loopback-only scope makes the race window tiny.
fn pick_free_port() -> Result<u16, LLMError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| LLMError::Other(format!("Failed to allocate ephemeral port: {err}")))?;
    let port = listener
        .local_addr()
        .map_err(|err| LLMError::Other(format!("Failed to read assigned port: {err}")))?
        .port();
    drop(listener);
    Ok(port)
}

fn build_server_args(config: &SidecarConfig, port: u16) -> Vec<String> {
    vec![
        "-m".to_string(),
        config.model_path.to_string_lossy().to_string(),
        "--port".to_string(),
        port.to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(), // explicit — never bind public iface
        "-ngl".to_string(),
        config.n_gpu_layers.to_string(),
        "--ctx-size".to_string(),
        config.context_size.to_string(),
    ]
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
/// - Detects `Terminated` mid-flight and clears the child slot in
///   `child_arc` so `SidecarRegistry::kill_all` doesn't try to kill
///   a dead PID at shutdown.
///
/// # Readiness handshake
///
/// Returns a `oneshot::Receiver<Result<(), LLMError>>` so the caller
/// can `await` readiness while the drain task owns the
/// `CommandEvent` stream. The oneshot fires exactly once:
///
/// - `Ok(())` when stderr emits the readiness needle, or
/// - `Err(...)` if the sidecar terminates / errors before readiness.
///
/// After that signal, the drain keeps running for stderr capture
/// even though the caller has moved on.
fn spawn_event_drain(
    rx: Receiver<CommandEvent>,
    port: u16,
    endpoint: String,
    child_arc: Arc<SyncMutex<Option<CommandChild>>>,
) -> tokio::sync::oneshot::Receiver<Result<(), LLMError>> {
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let mut rx = rx;
        let mut ready_tx = Some(ready_tx);
        let mut readiness_signaled = false;

        // Helper to fire the readiness oneshot exactly once. After
        // readiness is signaled we keep draining; the caller has
        // moved on and ignores further outcomes from this task.
        let signal_ready =
            |ready_tx: &mut Option<tokio::sync::oneshot::Sender<Result<(), LLMError>>>,
             result: Result<(), LLMError>| {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(result);
                }
            };

        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        tracing::debug!(target: "llama_server", "{}", trimmed);
                    }
                    if !readiness_signaled && trimmed.contains(READY_NEEDLE) {
                        tracing::info!("llama-server sidecar ready on http://127.0.0.1:{port}");
                        signal_ready(&mut ready_tx, Ok(()));
                        readiness_signaled = true;
                    }
                }
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        tracing::debug!(target: "llama_server", "stdout: {}", trimmed);
                    }
                }
                CommandEvent::Error(err) => {
                    if !readiness_signaled {
                        signal_ready(
                            &mut ready_tx,
                            Err(LLMError::GenerationFailed(format!(
                                "llama-server sidecar errored during startup: {err}"
                            ))),
                        );
                        readiness_signaled = true;
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
                    if !readiness_signaled {
                        signal_ready(
                            &mut ready_tx,
                            Err(LLMError::GenerationFailed(format!(
                                "llama-server sidecar exited during startup \
                                 (code: {:?}, signal: {:?}). \
                                 Check the GGUF file is valid and the model \
                                 architecture is supported.",
                                payload.code, payload.signal
                            ))),
                        );
                        readiness_signaled = true;
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
        if !readiness_signaled {
            signal_ready(
                &mut ready_tx,
                Err(LLMError::GenerationFailed(
                    "llama-server sidecar event stream closed before readiness".to_string(),
                )),
            );
        }

        tracing::debug!(
            target: "llama_server",
            endpoint = %endpoint,
            "Drain task exiting (channel closed)"
        );
    });

    ready_rx
}

/// Wait for readiness with a timeout, consuming the oneshot from
/// `spawn_event_drain`. The drain task itself is unaffected by this
/// timeout — it keeps running regardless. Only the readiness signal
/// is bounded.
async fn await_ready(
    ready_rx: tokio::sync::oneshot::Receiver<Result<(), LLMError>>,
) -> Result<(), LLMError> {
    match timeout(READINESS_TIMEOUT, ready_rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_recv_err)) => {
            // Sender dropped without sending — drain task panicked
            // or exited unexpectedly. Surface as a generic startup
            // failure.
            Err(LLMError::GenerationFailed(
                "Sidecar drain task ended without signaling readiness".to_string(),
            ))
        }
        Err(_) => Err(LLMError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_free_port_returns_nonzero_loopback_port() {
        let port = pick_free_port().expect("port allocation failed");
        assert!(port > 0, "expected a non-zero port, got {port}");
    }

    #[test]
    fn pick_free_port_avoids_collision() {
        // Allocate two in succession. Both must succeed and be different
        // (the OS shouldn't reuse the same ephemeral port back-to-back
        // while a TIME_WAIT socket is live).
        let p1 = pick_free_port().expect("first allocation");
        let p2 = pick_free_port().expect("second allocation");
        // We don't assert p1 != p2 — the OS *could* technically hand
        // out the same port if the first listener was fully released.
        // Just assert both are valid.
        assert!(p1 > 0 && p2 > 0);
    }

    #[test]
    fn sidecar_config_for_model_uses_gpu_defaults() {
        let cfg = SidecarConfig::for_model(PathBuf::from("/tmp/model.gguf"));
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.context_size, 8192);
    }

    #[test]
    fn sidecar_config_cpu_only_disables_gpu() {
        let cfg = SidecarConfig::cpu_only(PathBuf::from("/tmp/model.gguf"));
        assert_eq!(cfg.n_gpu_layers, 0);
        assert_eq!(cfg.context_size, 4096);
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

    #[test]
    fn is_startup_failure_classifies_errors() {
        assert!(is_startup_failure(&LLMError::Timeout));
        assert!(is_startup_failure(&LLMError::GenerationFailed(
            "child exited during startup".to_string()
        )));
        // Other errors should NOT trigger a CPU-mode retry — they're
        // not curable by dropping `-ngl`.
        assert!(!is_startup_failure(&LLMError::Other(
            "model file not found".to_string()
        )));
        assert!(!is_startup_failure(&LLMError::PlatformNotSupported(
            "no AppHandle".to_string()
        )));
        assert!(!is_startup_failure(&LLMError::InvalidConfig(
            "bad params".to_string()
        )));
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

    /// `await_ready` should propagate the oneshot result faithfully:
    /// Ok(()) → Ok(()), Err(LLMError) → Err(LLMError), and a dropped
    /// sender → a clear "drain task ended" error.
    #[tokio::test]
    async fn await_ready_propagates_ok() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(Ok(())).expect("send");
        let result = await_ready(rx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn await_ready_propagates_err() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(Err(LLMError::GenerationFailed("boom".to_string())))
            .expect("send");
        let result = await_ready(rx).await;
        assert!(matches!(result, Err(LLMError::GenerationFailed(msg)) if msg == "boom"));
    }

    #[tokio::test]
    async fn await_ready_handles_dropped_sender() {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), LLMError>>();
        drop(tx);
        let result = await_ready(rx).await;
        assert!(matches!(result, Err(LLMError::GenerationFailed(_))));
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
}
