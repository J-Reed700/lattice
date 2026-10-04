use super::*;

pub(super) fn append_probe_line(target: &mut Vec<u8>, bytes: &[u8], total: &mut usize) -> bool {
    let Some(next_total) = total.checked_add(bytes.len().saturating_add(1)) else {
        return false;
    };
    if next_total > PROBE_OUTPUT_MAX_BYTES {
        return false;
    }
    target.extend_from_slice(bytes);
    target.push(b'\n');
    *total = next_total;
    true
}

/// Result of running `<binary> --version`.
#[derive(Debug, Clone)]
pub(super) enum PreflightOutcome {
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
    pub(super) fn cacheable(&self) -> bool {
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
pub(super) struct PreflightCache {
    pub(super) entries: SyncMutex<Vec<(SidecarBinary, Instant, PreflightOutcome)>>,
    /// Serializes the `--version` runs so two concurrent model loads (chat and
    /// utility prewarm start together) share one.
    pub(super) running: tokio::sync::Mutex<()>,
}

impl PreflightCache {
    pub(super) fn new() -> Self {
        Self {
            entries: SyncMutex::new(Vec::new()),
            running: tokio::sync::Mutex::new(()),
        }
    }

    pub(super) fn get(&self, binary: SidecarBinary) -> Option<PreflightOutcome> {
        self.entries
            .lock()
            .iter()
            .find(|(cached, at, _)| *cached == binary && at.elapsed() < PREFLIGHT_CACHE_TTL)
            .map(|(_, _, outcome)| outcome.clone())
    }

    pub(super) fn put(&self, binary: SidecarBinary, outcome: &PreflightOutcome) {
        let mut entries = self.entries.lock();
        entries.retain(|(cached, at, _)| *cached != binary && at.elapsed() < PREFLIGHT_CACHE_TTL);
        entries.push((binary, Instant::now(), outcome.clone()));
    }

    /// Drop what we know about `binary`, so the next load decides afresh.
    pub(super) fn forget(&self, binary: SidecarBinary) {
        self.entries
            .lock()
            .retain(|(cached, _, _)| *cached != binary);
    }
}

/// The process-wide cache. Injectable so the policy is testable without a
/// process-global `OnceCell` that no test can reset.
pub(super) fn preflight_cache() -> &'static PreflightCache {
    static CACHE: std::sync::OnceLock<PreflightCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(PreflightCache::new)
}

/// The cached preflight for `binary`, running it on first use.
pub(super) async fn preflight(
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
pub(super) async fn run_preflight(app: &AppHandle, binary: SidecarBinary) -> PreflightOutcome {
    #[cfg(windows)]
    suppress_loader_error_dialogs();

    let started = Instant::now();
    let path = resolved_sidecar_path(binary);
    let outcome = match SpawnedChild::spawn_probe(
        app,
        binary,
        vec!["--version".to_string()],
        &format!("probe://{}/version", binary.label()),
    ) {
        Err(err) => preflight_attempt_failure(err),
        Ok((rx, child)) => collect_preflight(binary, rx, child).await,
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

pub(super) fn preflight_attempt_failure(error: AttemptError) -> PreflightOutcome {
    match error.kind {
        AttemptFailure::BinaryUnusable(reason) => PreflightOutcome::Unusable {
            message: error.message,
            reason,
            conclusive: true,
        },
        _ => PreflightOutcome::Inconclusive {
            detail: error.message,
        },
    }
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
pub(super) fn suppress_loader_error_dialogs() {
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

pub(super) async fn collect_preflight(
    binary: SidecarBinary,
    mut rx: Receiver<CommandEvent>,
    child: SpawnedChild,
) -> PreflightOutcome {
    let mut tail = OutputTail::default();
    let mut output_bytes = 0usize;
    let mut output_exceeded = false;
    let exit = timeout(PREFLIGHT_TIMEOUT, async {
        let mut exit = (None, None);
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(bytes) | CommandEvent::Stdout(bytes) => {
                    let remaining = PROBE_OUTPUT_MAX_BYTES.saturating_sub(output_bytes);
                    if bytes.len() > remaining {
                        output_exceeded = true;
                        break;
                    }
                    output_bytes = output_bytes.saturating_add(bytes.len());
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim_end();
                    if !line.is_empty() {
                        tail.push(line);
                    }
                }
                CommandEvent::Error(err) => tail.push(&format!("(event error) {err}")),
                CommandEvent::Terminated(payload) => {
                    exit = (payload.code, payload.signal);
                    child.mark_exited();
                }
                _ => {}
            }
        }
        exit
    })
    .await;

    if output_exceeded {
        return PreflightOutcome::Inconclusive {
            detail: format!("`--version` output exceeded {PROBE_OUTPUT_MAX_BYTES} bytes"),
        };
    }
    let Ok((code, signal)) = exit else {
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

/// Run the `--version` preflight for every bundled build in the background,
/// so the log records at startup whether local models can run at all. Never
/// blocks the caller; the result is cached for the first model load.
pub fn spawn_binary_preflight(app: &AppHandle) {
    let app = app.clone();
    let cancellation = crate::shared::runtime::background::cancellation_token();
    let _ = crate::shared::runtime::background::spawn(async move {
        for &binary in SidecarBinary::bundled() {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return,
                _ = preflight(&app, binary, preflight_cache()) => {}
            }
        }
        // Warm the device probe on the same startup pass. Callers that reach
        // for it without an `AppHandle` can only read the cache, and the log
        // line it emits is the record of what this machine can offload to.
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {},
            _ = crate::features::llm::engine::system::detect_backend_devices(&app) => {}
        }
    });
}
