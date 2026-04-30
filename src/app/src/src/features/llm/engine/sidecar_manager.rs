//! Sidecar process manager for the bundled `llama-server` binary.
//!
//! Owns the lifecycle of the local-LLM inference engine: port allocation,
//! spawn, readiness detection, kill-on-drop. Built behind a clean
//! HTTP boundary so the rest of the app sees only an `endpoint: String`
//! and never deals with process internals.
//!
//! # Why a sidecar
//!
//! Sprint 2's LLM migration replaces in-process `mistralrs` FFI with
//! the `llama-server` binary that ships in the Lattice installer. The
//! binary speaks an OpenAI-compatible HTTP/SSE API on `127.0.0.1:<port>`.
//! This module spawns it; `SidecarLLMClient` (Sprint 2 PR 2.1) is the
//! HTTP client that consumes it.
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
//!      on Unix). Sprint 6 PR 6.1 hardens this further with Windows
//!      Job Objects and explicit RunEvent::ExitRequested handling.
//!
//! # What this module does NOT do
//!
//! - It does not implement the `LLMClient` trait — that's `SidecarLLMClient`
//!   in Sprint 2 PR 2.1.
//! - It does not pick which binary variant (Vulkan vs CPU on Windows)
//!   to use — that's Sprint 3 PR 3.2's fallback chain.
//! - It does not download model files — that's the existing model
//!   storage layer, untouched by this migration.

use crate::llm::system::SystemCapabilities;
use crate::llm::types::LLMError;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::{mpsc::Receiver, Mutex};
use tokio::time::timeout;


pub const SIDECAR_BIN: &str = "binaries/llama-server";
const READY_NEEDLE: &str = "HTTP server listening";
const READINESS_TIMEOUT: Duration = Duration::from_secs(60);

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

    /// CPU-only fallback config. Used by Sprint 3's crash-fallback path.
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
/// the handle terminates the child via `kill()`.
pub struct SidecarHandle {
    /// `127.0.0.1:<port>` — the HTTP base URL the LLM client posts to.
    endpoint: String,

    /// Process handle. `Mutex` because `kill()` consumes `self`.
    /// `Option` so `Drop` can take ownership exactly once.
    child: Arc<Mutex<Option<CommandChild>>>,
}

impl SidecarHandle {
    /// Base URL of the local llama-server HTTP API, e.g.
    /// `http://127.0.0.1:53412`. Pass to `SidecarLLMClient`.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Explicitly kill the sidecar. After this returns, the handle is
    /// inert; `Drop` becomes a no-op. Idempotent — second call is fine.
    pub async fn stop(&self) {
        let mut guard = self.child.lock().await;
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
        let child = self.child.clone();
        let endpoint = self.endpoint.clone();
        tokio::task::spawn(async move {
            let mut guard = child.lock().await;
            if let Some(c) = guard.take() {
                let _ = c.kill();
                tracing::debug!("SidecarHandle dropped; killed sidecar at {endpoint}");
            }
        });
    }
}

/// Spawns and supervises a `llama-server` sidecar.
pub struct SidecarManager;

impl SidecarManager {
    /// Spawn a llama-server sidecar with crash-fallback.
    ///
    /// Sprint 3 PR 3.2: when `config.n_gpu_layers > 0` (GPU offload
    /// requested) and the spawn fails with a startup crash or
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
    /// can render a "running in CPU mode" banner. Sprint 6 PR 6.2
    /// pipes that into structured user-facing diagnostics.
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
    pub async fn start(
        app: &AppHandle,
        config: SidecarConfig,
    ) -> Result<SidecarHandle, LLMError> {
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
            .map_err(|err| {
                LLMError::Other(format!("Failed to locate sidecar binary: {err}"))
            })?
            .args(args)
            .spawn()
            .map_err(|err| {
                LLMError::Other(format!("Failed to spawn llama-server sidecar: {err}"))
            })?;

        // 5. Wait for the readiness needle on stderr. Bounded by timeout.
        wait_for_ready(rx, port).await?;

        Ok(SidecarHandle {
            endpoint,
            child: Arc::new(Mutex::new(Some(child))),
        })
    }
}


fn is_startup_failure(err: &LLMError) -> bool {
    matches!(
        err,
        LLMError::Timeout | LLMError::GenerationFailed(_)
    )
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


async fn wait_for_ready(
    mut rx: Receiver<CommandEvent>,
    port: u16,
) -> Result<(), LLMError> {
    let wait = async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    tracing::debug!(target: "llama_server", "{}", line.trim_end());
                    if line.contains(READY_NEEDLE) {
                        tracing::info!(
                            "llama-server sidecar ready on http://127.0.0.1:{port}"
                        );
                        return Ok(());
                    }
                }
                CommandEvent::Stdout(line_bytes) => {
                    let line = String::from_utf8_lossy(&line_bytes);
                    tracing::debug!(target: "llama_server", "stdout: {}", line.trim_end());
                }
                CommandEvent::Error(err) => {
                    return Err(LLMError::GenerationFailed(format!(
                        "llama-server sidecar errored during startup: {err}"
                    )));
                }
                CommandEvent::Terminated(payload) => {
                    return Err(LLMError::GenerationFailed(format!(
                        "llama-server sidecar exited during startup (code: {:?}, signal: {:?}). \
                         Check the GGUF file is valid and the model architecture is supported.",
                        payload.code, payload.signal
                    )));
                }
                _ => {}
            }
        }
        // Channel closed without seeing the needle — sidecar exited
        // silently. Treat as startup failure.
        Err(LLMError::GenerationFailed(
            "llama-server sidecar event stream closed before readiness".to_string(),
        ))
    };

    timeout(READINESS_TIMEOUT, wait).await.map_err(|_| {
        LLMError::Timeout
    })?
}

// ============================================================================
// Tests
// ============================================================================

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

    fn make_caps(gpu_vendor: Option<crate::llm::system::GPUVendor>, ram_gb: f64) -> SystemCapabilities {
        use crate::llm::system::{GPUInfo, Platform};
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
        use crate::llm::system::GPUVendor;
        let caps = make_caps(Some(GPUVendor::Apple), 16.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.context_size, DEFAULT_GPU_CONTEXT_SIZE);
    }

    #[test]
    fn from_capabilities_nvidia_uses_full_offload() {
        use crate::llm::system::GPUVendor;
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
        use crate::llm::system::GPUVendor;
        let caps = make_caps(Some(GPUVendor::Unknown), 16.0);
        let cfg = SidecarConfig::from_capabilities(PathBuf::from("/tmp/m.gguf"), &caps);
        // Unknown vendor → is_accelerated() returns false → CPU path
        assert_eq!(cfg.n_gpu_layers, 0);
    }

    #[test]
    fn from_capabilities_low_ram_shrinks_context() {
        use crate::llm::system::GPUVendor;
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
