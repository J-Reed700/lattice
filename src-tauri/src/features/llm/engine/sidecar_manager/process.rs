use super::*;

/// Live sidecar process handle.
///
/// Holds the OS process and the channel of events from it. Dropping
/// the handle terminates the child via `kill()`. The inner mutex uses
/// `parking_lot::Mutex` so the registry's synchronous kill_all path
/// can lock without an async runtime.
pub struct SidecarHandle {
    /// `127.0.0.1:<port>` — the HTTP base URL the LLM client posts to.
    pub(super) endpoint: String,

    /// Bearer token this server was launched with. Loopback is not a trust
    /// boundary: without it any local process — including a page in the user's
    /// browser, which can reach 127.0.0.1 — could generate on the user's GPU
    /// and read back the model path. Never logged.
    pub(super) api_token: String,

    /// Process handle. Sync `Mutex` because `CommandChild::kill()` is
    /// itself synchronous (a syscall, no `.await`) and the shutdown
    /// path runs from a sync Tauri callback. `Option` so the kill
    /// owner takes the child exactly once.
    pub(super) child: Arc<SyncMutex<Option<CommandChild>>>,

    /// The `--ctx-size` this server was actually launched with.
    ///
    /// Not a constant: it is 8192 on GPU machines, 4096 CPU-only, and 2048
    /// under the low-RAM threshold. Callers that budget a prompt must use
    /// this value — reporting a fixed 8192 upstream meant every budget
    /// overshot the real window by 2–4x on smaller machines, so llama-server
    /// silently truncated the prompt server-side and the system prompt and
    /// oldest history simply vanished.
    pub(super) context_size: u32,

    /// The build that actually started, after any fallback.
    pub(super) binary: SidecarBinary,

    /// The `-ngl` this server was actually launched with, after any fallback.
    pub(super) n_gpu_layers: u32,

    /// The registry slot this handle owns, released when it dies.
    pub(super) registration: Option<Registration>,

    /// Held for the life of the process so no other spawn is handed this port.
    pub(super) _port: PortReservation,

    /// Set when the server that finally ran is not the one that was asked
    /// for — a GPU machine running CPU-only, or a halved context. Callers
    /// log it; without it a degradation is invisible outside a warn line.
    pub(super) degraded: Option<String>,
}

/// A handle's slot in the process registry. The registry lives in Tauri
/// managed state, so releasing the slot means looking it up again.
pub(super) struct Registration {
    pub(super) app: AppHandle,
    pub(super) id: u64,
}

impl Registration {
    /// Never called while a child mutex is held: `register` locks the registry
    /// and then a child, so the reverse order would be a lock cycle.
    pub(super) fn release(&self) {
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

    /// Whether the server behind this handle is still up.
    ///
    /// The child slot is emptied by exactly two things: `stop`, and the event
    /// drain when it sees `Terminated`. So this is false for a handle that was
    /// stopped *and* for one whose server exited or crashed on its own — which
    /// is what the pool needs, since a crashed server's handle stays alive as
    /// an `Arc` for as long as any role holds it.
    pub fn is_running(&self) -> bool {
        self.child.lock().is_some()
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

impl Liveness for SidecarHandle {
    fn is_running(&self) -> bool {
        SidecarHandle::is_running(self)
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
pub(in crate::features::llm::engine) struct SpawnedChild {
    pub(super) child: Arc<SyncMutex<Option<CommandChild>>>,
    pub(super) registration: Option<Registration>,
    pub(super) endpoint: String,
    pub(super) armed: bool,
}

impl SpawnedChild {
    /// Spawn `binary` and register it. Registration happens before readiness
    /// so the shutdown sweep can find a child that never became ready.
    pub(super) fn spawn(
        app: &AppHandle,
        binary: SidecarBinary,
        args: Vec<String>,
        endpoint: &str,
        api_token: &str,
        invalidate_preflight: bool,
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
        if invalidate_preflight {
            preflight_cache().forget(binary);
        }

        let child = Arc::new(SyncMutex::new(Some(child)));
        let registration = match app.try_state::<SidecarRegistry>() {
            Some(registry) => match registry.register(&child, endpoint) {
                Some(id) => Some(Registration {
                    app: app.clone(),
                    id,
                }),
                None => {
                    return Err(AttemptError::fatal(
                        "Sidecar registry is shutting down".to_string(),
                    ));
                }
            },
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

    pub(super) fn spawn_probe(
        app: &AppHandle,
        binary: SidecarBinary,
        args: Vec<String>,
        endpoint: &str,
    ) -> Result<(Receiver<CommandEvent>, Self), AttemptError> {
        Self::spawn(app, binary, args, endpoint, "", false)
    }

    pub(in crate::features::llm::engine) async fn run_bounded_probe(
        app: &AppHandle,
        binary: SidecarBinary,
        arg: &str,
        endpoint: &str,
        probe_timeout: Duration,
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let cancellation = crate::shared::runtime::background::cancellation_token();
        if cancellation.is_cancelled() {
            return None;
        }
        let (mut rx, child) = match Self::spawn_probe(app, binary, vec![arg.to_string()], endpoint)
        {
            Ok(spawned) => spawned,
            Err(error) => {
                tracing::debug!(%error.message, "sidecar probe could not start");
                return None;
            }
        };

        let collect = async {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut total = 0usize;
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(bytes)
                        if !append_probe_line(&mut stdout, &bytes, &mut total) =>
                    {
                        tracing::debug!(
                            max_bytes = PROBE_OUTPUT_MAX_BYTES,
                            "sidecar probe output exceeded limit"
                        );
                        return None;
                    }
                    CommandEvent::Stderr(bytes)
                        if !append_probe_line(&mut stderr, &bytes, &mut total) =>
                    {
                        tracing::debug!(
                            max_bytes = PROBE_OUTPUT_MAX_BYTES,
                            "sidecar probe output exceeded limit"
                        );
                        return None;
                    }
                    CommandEvent::Terminated(_) => child.mark_exited(),
                    _ => {}
                }
            }
            Some((stdout, stderr))
        };

        tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            result = timeout(probe_timeout, collect) => match result {
                Ok(output) => output,
                Err(_) => {
                    tracing::debug!(arg, "sidecar probe timed out");
                    None
                }
            }
        }
        // `child` remains owned through timeout/cancellation. Its Drop kills
        // unfinished children and unregisters them; completed events clear it.
    }

    /// The child slot, for the drain task to clear when the process exits.
    pub(super) fn child(&self) -> Arc<SyncMutex<Option<CommandChild>>> {
        Arc::clone(&self.child)
    }

    pub(super) fn mark_exited(&self) {
        self.child.lock().take();
    }

    /// Disarm and hand ownership to the long-lived handle.
    pub(super) fn into_handle(
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
