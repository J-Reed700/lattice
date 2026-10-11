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
//! This module spawns it; `LlamaCppLlm` is its HTTP client.
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
//! - It does not talk to the server; `LlamaCppLlm` does.
//! - It does not download model files — that's the existing model
//!   storage layer, untouched by this migration.

use crate::features::llm::engine::sidecar_pool::{Liveness, Origin, SharedProcesses};
use crate::features::llm::engine::system::SystemCapabilities;
use crate::features::llm::engine::types::LLMError;
use crate::features::llm::scheduler::InferenceScheduler;
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

/// The primary llama-server build (Metal on Apple Silicon, CPU on Intel Macs,
/// Vulkan on Windows/Linux).
pub const SIDECAR_BIN: &str = "llama-server";
/// CPU-only llama-server build. Bundled on Windows and Linux only, where
/// the primary build is Vulkan and cannot even start without a Vulkan
/// loader (`vulkan-1.dll` / `libvulkan.so.1`). macOS ships one build per
/// architecture. Always reached through [`SidecarBinary::Cpu`].
pub const SIDECAR_CPU_BIN: &str = "llama-server-cpu";
/// Shared part of both the b-series `server is listening on` log and the
/// semver releases' `llama_server: listening on` log. The pre-b4000
/// string was `HTTP server listening`, which appears nowhere in the shipped
/// binaries — the wait never matched and every model load timed out. It is a
/// fast path only: `/health` decides, so the next upstream rewording costs a
/// second of startup rather than the whole feature. `ready_needle_is_present`
/// pins it to the bundled binaries.
const READY_NEEDLE: &str = "listening on";

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
const PROBE_OUTPUT_MAX_BYTES: usize = 1024 * 1024;

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

mod config;
mod failure;
mod preflight;
mod process;
mod readiness;
mod registry;
mod startup;

pub use config::{SidecarBinary, SidecarConfig};
pub use preflight::spawn_binary_preflight;
pub use process::SidecarHandle;
pub(super) use process::SpawnedChild;
pub use registry::{reap_orphan_sidecars, SidecarRegistry};
pub use startup::SidecarManager;

use config::*;
use failure::*;
use preflight::*;
use readiness::*;
#[cfg(test)]
use registry::*;

#[cfg(test)]
mod tests;
