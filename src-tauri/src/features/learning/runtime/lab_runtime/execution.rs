//! Materializing run workspaces and running, bounding and cleaning up containers.
use super::*;

pub(super) fn classify_engine_exit(success: bool, exit_code: Option<i32>) -> LearningLabRunStatus {
    if success {
        LearningLabRunStatus::Passed
    } else if matches!(exit_code, Some(125..=127)) {
        // Docker and Podman reserve these codes for engine/launch errors.
        // https://docs.docker.com/engine/containers/run/#exit-status
        LearningLabRunStatus::RuntimeUnavailable
    } else {
        LearningLabRunStatus::Failed
    }
}

pub fn materialize_workspace(root: &Path, spec: &LearningLabExecutionSpec) -> Result<()> {
    validate_execution_spec(spec)?;
    std::fs::create_dir(root)?;
    // The workspace is mounted read-only at /input. The guest stages its
    // private writable copy into a bounded tmpfs, so host files need only be
    // readable by the unprivileged container user.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755))?;
    }
    let canonical_root = root.canonicalize()?;
    for file in &spec.files {
        let relative = validate_lab_path(&file.path)?;
        let target = canonical_root.join(relative);
        let parent = target.parent().ok_or_else(|| {
            AppError::InvalidInput("A lab file is missing its parent directory.".into())
        })?;
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755))?;
        }
        let canonical_parent = parent.canonicalize()?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(AppError::Security(
                "A lab file attempted to escape the run workspace.".into(),
            ));
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        use std::io::Write;
        options.open(&target)?.write_all(file.content.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))?;
        }
    }
    Ok(())
}

fn truncate_utf8_to_budget(output: &mut String, budget: usize) -> bool {
    if output.len() <= budget {
        return false;
    }
    let mut boundary = budget.min(output.len());
    while !output.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    output.truncate(boundary);
    true
}

/// Enforce one byte budget across both user-visible output streams, preferring
/// stdout. Trimming occurs after lossy UTF-8 decoding so replacement characters
/// count toward the same limit the DTO actually returns.
pub(super) fn bound_combined_output(
    stdout: &mut String,
    stderr: &mut String,
    limit: usize,
) -> bool {
    let mut truncated = truncate_utf8_to_budget(stdout, limit);
    let stderr_budget = limit.saturating_sub(stdout.len());
    truncated |= truncate_utf8_to_budget(stderr, stderr_budget);
    truncated
}

async fn force_remove(engine: LearningContainerEngine, name: &str) {
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        engine
            .command()
            .args(["rm", "--force", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await;
}

async fn stop_container_process(
    engine: LearningContainerEngine,
    name: &str,
    child: &mut tokio::process::Child,
) {
    force_remove(engine, name).await;
    if !matches!(
        tokio::time::timeout(Duration::from_secs(5), child.wait()).await,
        Ok(Ok(_))
    ) {
        // Do not let a wedged engine client make cancellation or timeout hang
        // forever. The deterministic name still permits startup recovery to
        // remove a container if the daemon becomes reachable again.
        let _ = child.kill().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        force_remove(engine, name).await;
    }
}

/// Best-effort cleanup for a run that was left active when the desktop process
/// stopped. The deterministic container name avoids inspecting or touching any
/// unrelated container.
pub async fn cleanup_interrupted_run(engine: LearningContainerEngine, run_id: &str) -> Result<()> {
    uuid(run_id, "lab run")?;
    force_remove(engine, &container_name(run_id)).await;
    Ok(())
}

pub async fn detect_container_engine(
    engine: LearningContainerEngine,
) -> LearningLabRuntimeCapability {
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        engine
            .command()
            .args(["version", "--format", "{{.Server.Version}}"])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    match response {
        Ok(Ok(output)) if output.status.success() => LearningLabRuntimeCapability {
            engine,
            available: true,
            version: Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            reason: None,
        },
        Ok(Ok(output)) => LearningLabRuntimeCapability {
            engine,
            available: false,
            version: None,
            reason: Some(
                String::from_utf8_lossy(&output.stderr)
                    .trim()
                    .chars()
                    .take(500)
                    .collect(),
            ),
        },
        Ok(Err(error)) => LearningLabRuntimeCapability {
            engine,
            available: false,
            version: None,
            reason: Some(error.to_string()),
        },
        Err(_) => LearningLabRuntimeCapability {
            engine,
            available: false,
            version: None,
            reason: Some("Runtime capability check timed out.".into()),
        },
    }
}

pub async fn execute_container_lab(
    engine: LearningContainerEngine,
    workspace: &Path,
    spec: &LearningLabExecutionSpec,
    cancellation: CancellationToken,
) -> Result<LearningLabExecutionResult> {
    let args = container_run_args(spec, workspace)?;
    let started = std::time::Instant::now();
    let mut child = engine
        .command()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            AppError::ServiceNotAvailable(format!(
                "{} is unavailable: {error}",
                engine.executable()
            ))
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::InternalError("Lab runtime did not provide stdout.".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::InternalError("Lab runtime did not provide stderr.".into()))?;
    let output_limit = spec.limits.output_bytes;
    // raw-spawn: both pipe readers are joined below, before this returns
    let (stdout_task, stderr_task) = (
        tokio::spawn(bounded_read(stdout, output_limit)),
        tokio::spawn(bounded_read(stderr, output_limit)),
    );
    let name = container_name(&spec.run_id);
    let deadline = tokio::time::sleep(Duration::from_secs(spec.limits.timeout_seconds));
    tokio::pin!(deadline);
    let (status, exit_code) = tokio::select! {
        biased;
        _ = cancellation.cancelled() => {
            stop_container_process(engine, &name, &mut child).await;
            (LearningLabRunStatus::Cancelled, None)
        }
        _ = &mut deadline => {
            stop_container_process(engine, &name, &mut child).await;
            (LearningLabRunStatus::TimedOut, None)
        }
        result = child.wait() => {
            let status = result?;
            let code = status.code();
            (classify_engine_exit(status.success(), code), code)
        }
    };
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))??;
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|error| AppError::InternalError(error.to_string()))??;
    let mut stdout = String::from_utf8_lossy(&stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&stderr).into_owned();
    let combined_truncated =
        bound_combined_output(&mut stdout, &mut stderr, spec.limits.output_bytes);
    let output_truncated = stdout_truncated || stderr_truncated || combined_truncated;
    Ok(LearningLabExecutionResult {
        status,
        exit_code,
        stdout,
        stderr,
        output_truncated,
        duration_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
    })
}
