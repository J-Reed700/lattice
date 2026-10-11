use super::*;

/// Ports handed out and not yet released.
///
/// Binding `127.0.0.1:0` reports a port and immediately gives it back, so two
/// spawns racing — chat and utility prewarm start together — were handed the
/// same number, and the loser died with a bind error that used to be read as
/// a GPU fault and "fixed" by dropping the whole session to CPU-only.
pub(super) static RESERVED_PORTS: SyncMutex<BTreeSet<u16>> = SyncMutex::new(BTreeSet::new());

/// A port claimed for one sidecar, released when the reservation drops.
/// Held by the handle, so a live server's port is never handed out again.
pub(super) struct PortReservation {
    pub(super) port: u16,
}

impl PortReservation {
    pub(super) fn port(&self) -> u16 {
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
pub(super) fn reserve_free_port() -> std::io::Result<PortReservation> {
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

pub(super) fn build_server_args(config: &SidecarConfig, port: u16) -> Vec<String> {
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
        // The GGUF's own Jinja chat template is what knows the model's native
        // tool-call format; without it llama-server cannot render `tools` or
        // parse `tool_calls` back out, and the tool loop gets plain text.
        "--jinja".to_string(),
    ]
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
    rx: Receiver<CommandEvent>,
    endpoint: String,
    child_arc: Arc<SyncMutex<Option<CommandChild>>>,
    tail: Arc<SyncMutex<OutputTail>>,
    signals: Arc<StartupSignals>,
) -> tokio::sync::oneshot::Receiver<Result<(), StartupEnd>> {
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    let _ = crate::shared::runtime::background::spawn(async move {
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
