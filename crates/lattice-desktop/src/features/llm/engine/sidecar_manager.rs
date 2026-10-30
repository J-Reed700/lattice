//! Sidecar process manager for the bundled `llama-server` binary: port
//! allocation, spawn, readiness and kill-on-drop, so the rest of the app sees
//! only an endpoint. `LlamaCppLlm` is its HTTP client.
//!
//! Starting preflights the binary once per process (`--version`), picks a
//! free port, spawns it through `tauri-plugin-shell`, and waits for `/health`
//! (the "listening on" log line is a fast path); the wait bounds silence, not
//! duration. On failure it walks the fallback policy (`next_attempt`): GPU
//! off, then the CPU-only build on Windows/Linux. Stopping kills the child,
//! with Windows Job Objects cleaning up the process tree.

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
/// Shared part of the b-series `server is listening on` and semver
/// `llama_server: listening on` logs. A fast path only: `/health` decides
/// readiness. `ready_needle_is_present` pins it to the bundled binaries.
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

pub(crate) use config::{memory_hint, resident_bytes_of};
pub use config::{
    Footprint, MemoryHint, MemoryRoom, MemorySource, PoolingServing, PoolingTask, SidecarBinary,
    SidecarConfig, SpeculativeDecoding,
};
pub use preflight::spawn_binary_preflight;
pub use process::SidecarHandle;
pub(super) use process::SpawnedChild;
pub(crate) use readiness::{build_server_args, new_api_token};
pub use registry::{kill_registered_sidecars, reap_orphan_sidecars, Admission, SidecarRegistry};
pub use startup::SidecarManager;

use failure::*;
use preflight::*;
use readiness::*;
#[cfg(test)]
use registry::*;

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod weights_fixture;
