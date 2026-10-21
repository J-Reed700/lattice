use super::*;

/// Ports handed out and not yet released.
///
/// Binding `127.0.0.1:0` reports a port and immediately gives it back, so two
/// spawns racing — chat and utility prewarm start together — were handed the
/// same number, and the loser died with a bind error that used to be read as
/// a GPU fault and "fixed" by dropping the whole session to CPU-only.
///
/// The app's [`SidecarRegistry`] holds the set every spawn reserves from.
#[derive(Default)]
pub(super) struct ReservedPorts {
    pub(super) taken: SyncMutex<BTreeSet<u16>>,
}

/// A port claimed for one sidecar, released when the reservation drops.
/// Held by the handle, so a live server's port is never handed out again.
pub(super) struct PortReservation {
    pub(super) port: u16,
    ports: Arc<ReservedPorts>,
}

impl PortReservation {
    pub(super) fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for PortReservation {
    fn drop(&mut self) {
        self.ports.taken.lock().remove(&self.port);
    }
}

/// Reserve an ephemeral loopback port: bind `127.0.0.1:0`, read the number
/// back, drop the listener, and claim the number in `ports`. llama-server
/// still owns the real bind, so losing that race remains possible — it is
/// classified [`AttemptFailure::Retry`] and retried with a fresh port.
pub(super) fn reserve_free_port(ports: &Arc<ReservedPorts>) -> std::io::Result<PortReservation> {
    const ATTEMPTS: usize = 32;
    for _ in 0..ATTEMPTS {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        if ports.taken.lock().insert(port) {
            return Ok(PortReservation {
                port,
                ports: Arc::clone(ports),
            });
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!("no unreserved loopback port after {ATTEMPTS} attempts"),
    ))
}

/// Prompt-cache checkpoints kept per slot: llama-server's default, pinned so a
/// change upstream cannot quietly change how far a slot can roll back.
const CTX_CHECKPOINTS: u32 = 32;

/// Tokens between two checkpoints. A hybrid model (Qwen 3.5's Gated DeltaNet
/// layers) cannot trim its recurrent state, so a prompt that differs from the
/// cached one rolls back to the last checkpoint before the difference; at the
/// server's 8192 default, a change anywhere in the first 8k tokens threw the
/// whole prefix away. Pure-attention models trim the KV cache directly and
/// never consult these checkpoints, so the flag costs them nothing.
const CHECKPOINT_MIN_STEP: u32 = 1024;

pub(super) fn build_server_args(config: &SidecarConfig, port: u16) -> Vec<String> {
    if let Some(serving) = config.embedding {
        return embedding_server_args(config, serving, port);
    }
    let mut args = vec![
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
        // The GGUF's own Jinja chat template is what knows the model's native
        // tool-call format; without it llama-server cannot render `tools` or
        // parse `tool_calls` back out, and the tool loop gets plain text.
        "--jinja".to_string(),
        "--ctx-checkpoints".to_string(),
        CTX_CHECKPOINTS.to_string(),
        "--checkpoint-min-step".to_string(),
        CHECKPOINT_MIN_STEP.to_string(),
        "--cache-reuse".to_string(),
        "256".to_string(),
    ];
    if let Some(n_max) = config.mtp_draft() {
        // MTP drafting supports one slot only, so the server gives up its
        // parallel slots for it; the scheduler sizes itself from `/props`.
        args.extend([
            "-np".to_string(),
            "1".to_string(),
            "--spec-type".to_string(),
            "draft-mtp".to_string(),
            "--spec-draft-n-max".to_string(),
            n_max.to_string(),
        ]);
    } else if let Some(slots) = config.parallel_slots.filter(|&s| s > 0) {
        args.extend(["-np".to_string(), slots.to_string()]);
    }
    if let Some(seed) = config.seed {
        args.extend(["-s".to_string(), seed.to_string()]);
    }
    if config.kv_cache_quantization {
        args.extend([
            "-ctk".to_string(),
            "q8_0".to_string(),
            "-ctv".to_string(),
            "q8_0".to_string(),
        ]);
    }
    // Beside MTP drafting, not instead of it: the pinned server drafts and
    // sees at once (upstream enabled it in #19493, closing #19712). A build
    // that refuses is reported by `refused_combination`, never worked around.
    if let Some(projector) = &config.projector_path {
        args.extend([
            "--mmproj".to_string(),
            projector.to_string_lossy().to_string(),
        ]);
    }
    args
}

/// An embedding server's flags, and nothing else: no chat template, prompt
/// cache, speculative decoding, projector or quantized cache. The GGUF names
/// its pooling, so `--pooling` is not passed, and `--embd-normalize` stays at
/// its default (Euclidean), which is the L2 normalisation stored vectors have.
/// `-fa auto` is the build's default, named so a change upstream shows here.
fn embedding_server_args(
    config: &SidecarConfig,
    serving: EmbeddingServing,
    port: u16,
) -> Vec<String> {
    let mut args = vec![
        "-m".to_string(),
        config.model_path.to_string_lossy().to_string(),
        "--embedding".to_string(),
        "-c".to_string(),
        config.context_size.to_string(),
    ];
    if let Some(slots) = config.parallel_slots.filter(|&s| s > 0) {
        args.extend(["-np".to_string(), slots.to_string()]);
    }
    args.extend([
        "-b".to_string(),
        serving.ubatch.to_string(),
        "-ub".to_string(),
        serving.ubatch.to_string(),
        "-ngl".to_string(),
        config.n_gpu_layers.to_string(),
        "-fa".to_string(),
        "auto".to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--no-webui".to_string(),
    ]);
    args
}

/// The line an older llama-server prints when asked to draft and see at once.
const DRAFT_AND_SEE_REFUSAL: &str = "speculative decoding is not supported with multimodal";

/// A start the server refused because it cannot run two options together,
/// told as the settings to change rather than retried: every fallback
/// attempt asks for the same two options and is refused the same way.
/// Nothing is switched off for the user.
pub(super) fn refused_combination(output: &[String]) -> Option<AttemptError> {
    output
        .iter()
        .any(|line| line.to_lowercase().contains(DRAFT_AND_SEE_REFUSAL))
        .then(|| AttemptError {
            kind: AttemptFailure::Refused,
            message: "This llama-server cannot draft and see at once. Turn off either Vision \
                      or Speculative decoding for this model in Settings → Models."
                .to_string(),
        })
}

/// Mint a bearer token for one sidecar process.
///
/// 128 bits from the OS entropy source, hex-encoded so it can be spliced into
/// an `Authorization` header and an environment variable without escaping. A new
/// one per spawn means the window in which a leaked token is worth anything
/// closes when the process does.
pub(super) fn new_api_token() -> String {
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
pub(super) fn spawn_event_drain(
    background: &lattice_core::shared::runtime::background::BackgroundTasks,
    rx: Receiver<CommandEvent>,
    endpoint: String,
    child_arc: Arc<SyncMutex<Option<CommandChild>>>,
    tail: Arc<SyncMutex<OutputTail>>,
    signals: Arc<StartupSignals>,
) -> tokio::sync::oneshot::Receiver<Result<(), StartupEnd>> {
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    let _ = background.spawn(async move {
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
                    // CommandChild inside is gone. An empty slot is also
                    // what `is_running` reads, so the role caches treat the
                    // cached port as a miss and the next request starts a
                    // new server instead of failing against this dead one.
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
pub(super) enum HealthProbe {
    Ready,
    /// Answered, but not ready yet — progress all the same.
    Loading,
    /// Nothing listening yet, or the probe timed out.
    Unreachable,
}

pub(super) async fn probe_health(
    http: &reqwest::Client,
    endpoint: &str,
    api_token: &str,
) -> HealthProbe {
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
pub(super) async fn await_ready(
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
