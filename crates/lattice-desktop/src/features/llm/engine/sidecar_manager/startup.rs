use super::*;

/// Spawns and supervises a `llama-server` sidecar.
pub struct SidecarManager;

impl SidecarManager {
    /// Gets the sidecar serving `config`, starting one if nothing is: the entry
    /// point production code should use. `config` is a server's identity, so
    /// roles resolving to the same model on the same settings share one process,
    /// each keeping its own `LlamaCppLlm` and generation settings; the server's
    /// default `--parallel` serves them side by side over one `--ctx-size`
    /// window. Without a managed [`SidecarRegistry`] (tests) the server is
    /// unshared.
    pub async fn start_shared(
        app: &AppHandle,
        config: SidecarConfig,
    ) -> Result<Arc<SidecarHandle>, LLMError> {
        let Some(registry) = app.try_state::<SidecarRegistry>() else {
            tracing::debug!("SidecarRegistry not managed; starting an unshared sidecar");
            return Self::start_with_fallback(app, config).await.map(Arc::new);
        };

        let key = config.clone();
        let (handle, origin) = registry
            .shared
            .get_or_start(key, || Self::start_with_fallback(app, config))
            .await?;

        if origin == Origin::Reused {
            // Logged so sharing is visible: duplicate loads would otherwise look the
            // same in the log.
            tracing::info!(
                endpoint = %handle.endpoint(),
                binary = handle.binary().label(),
                n_gpu_layers = handle.n_gpu_layers(),
                context_size = handle.context_size(),
                "Reusing the running llama-server for this model; no second load"
            );
        }

        Ok(handle)
    }

    /// Spawns a llama-server sidecar, falling back until something runs: each
    /// failure is classified (`AttemptFailure`) and `next_attempt` picks the next
    /// try (see its table). When every attempt fails, a binary-unusable error
    /// wins, since it names the fix; otherwise the first error is reported, with
    /// the others summarized beneath it.
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
                    // A retryable failure (the port taken between allocation and
                    // llama-server's bind) is one the same configuration can still win, so
                    // the configuration is not degraded for it.
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
    pub(super) async fn start_attempt(
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
        } = registered_preflight(app, binary).await
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
        let ports = app
            .try_state::<SidecarRegistry>()
            .map(|registry| Arc::clone(&registry.ports))
            .unwrap_or_default();
        let reservation = reserve_free_port(&ports).map_err(|err| {
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

        // The drain below runs in the app's background scope, so shutdown
        // joins it. A host without one cannot supervise a server.
        let Some(background) =
            app.try_state::<Arc<lattice_core::shared::runtime::background::BackgroundTasks>>()
        else {
            return Err(AttemptError::fatal(
                "No background scope is managed; the sidecar cannot be supervised".into(),
            ));
        };

        // 5. Spawn via tauri-plugin-shell. `.sidecar()` looks the binary
        //    up next to the app executable. A failure here (missing file,
        //    no execute permission, wrong executable format) means the
        //    binary cannot run at all. The guard returned owns the child:
        //    every path out of this function from here on either kills it
        //    or converts it into a handle.
        let (rx, spawned) = SpawnedChild::spawn(app, binary, args, &endpoint, &api_token, true)?;

        // 6. Spawn the long-lived event drain task.
        //    The drain owns the receiver for the rest of the sidecar's
        //    life — feeds stderr/stdout into tracing, keeps the startup
        //    output tail, records the sticky startup signals classification
        //    needs, detects unexpected termination, and signals readiness
        //    via a oneshot.
        let tail = Arc::new(SyncMutex::new(OutputTail::default()));
        let signals = Arc::new(StartupSignals::default());
        let ready_rx = spawn_event_drain(
            &background,
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
            return Err(refused_combination(&output)
                .unwrap_or_else(|| startup_failure(binary, &end, &output, signals.facts())));
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
