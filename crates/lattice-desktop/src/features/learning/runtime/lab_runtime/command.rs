//! Building the contained `run` argv without interpolating learner text.
use super::*;

pub(super) fn container_name(run_id: &str) -> String {
    format!("lattice-learning-{}", run_id.replace('-', ""))
}

pub(super) const WORKSPACE_BOOTSTRAP: &str = "cp -R -n -- /input/. /workspace/ && exec \"$@\"";
pub(super) const WORKSPACE_BOOTSTRAP_ARG0: &str = "lattice-learning-stage-input";

pub(super) fn workspace_tmpfs_size(limits: &LearningLabLimits) -> u32 {
    (limits.memory_megabytes / 4).clamp(8, 256)
}

/// Build a container command without interpolating learner text into a shell.
/// Inputs bind read-only at `/input`; a memory-bounded executable tmpfs at
/// `/workspace` receives a copy before the trusted program runs. Protected
/// evaluator files are over-mounted read-only after the tmpfs and survive the
/// copy because the bootstrap uses `cp -n`.
pub fn container_run_args(
    spec: &LearningLabExecutionSpec,
    workspace: &Path,
) -> Result<Vec<OsString>> {
    validate_execution_spec(spec)?;
    if !workspace.is_absolute() {
        return Err(AppError::InvalidInput(
            "The lab run workspace must be an absolute path.".into(),
        ));
    }
    let workspace = workspace.to_str().ok_or_else(|| {
        AppError::InvalidInput("The lab workspace path is not valid Unicode.".into())
    })?;
    // Docker and Podman parse commas inside `--mount` as option separators.
    // Run workspaces are generated internally, but reject an unsafe host path
    // here as a final boundary instead of allowing its text to change the mount.
    if workspace.contains(',') {
        return Err(AppError::InvalidInput(
            "The lab workspace path cannot contain a comma.".into(),
        ));
    }
    let cpu = format!("{:.3}", f64::from(spec.limits.cpu_millis) / 1_000.0);
    let memory = format!("{}m", spec.limits.memory_megabytes);
    // Docker --mount key/value syntax is writable by default; a bare `rw`
    // token is rejected (it belongs to the older --volume syntax).
    let input_mount = format!("type=bind,src={workspace},dst=/input,readonly");
    let writable_tmpfs = format!(
        "/workspace:rw,exec,nosuid,nodev,size={}m,mode=1777",
        workspace_tmpfs_size(&spec.limits)
    );
    let tmpfs = format!(
        "/tmp:rw,noexec,nosuid,nodev,size={}m,mode=1777",
        (spec.limits.memory_megabytes / 4).clamp(16, 128)
    );
    let mut args: Vec<OsString> = [
        "run",
        "--rm",
        "--init",
        "--name",
        &container_name(&spec.run_id),
        "--pull",
        "never",
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges=true",
        "--pids-limit",
        &spec.limits.process_limit.to_string(),
        "--memory",
        &memory,
        "--memory-swap",
        &memory,
        "--cpus",
        &cpu,
        "--ulimit",
        "nofile=1024:1024",
        "--user",
        "65534:65534",
        "--workdir",
        "/workspace",
        "--mount",
        &input_mount,
        "--tmpfs",
        &writable_tmpfs,
        "--tmpfs",
        &tmpfs,
        "--env",
        "HOME=/tmp",
        "--env",
        "NO_COLOR=1",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    for path in &spec.read_only_paths {
        let source = Path::new(workspace).join(path);
        let source = source.to_str().ok_or_else(|| {
            AppError::InvalidInput("A read-only lab path is not valid Unicode.".into())
        })?;
        args.push(OsString::from("--mount"));
        args.push(OsString::from(format!(
            "type=bind,src={source},dst=/workspace/{path},readonly"
        )));
    }
    // Every engine option, including the nested read-only evaluator mounts,
    // must precede the image. The fixed bootstrap stages the host input mount
    // into bounded tmpfs before forwarding the trusted command argv.
    args.push(OsString::from(&spec.image_id));
    args.push(OsString::from("/bin/sh"));
    args.push(OsString::from("-c"));
    args.push(OsString::from(WORKSPACE_BOOTSTRAP));
    args.push(OsString::from(WORKSPACE_BOOTSTRAP_ARG0));
    args.extend(spec.command.iter().map(OsString::from));
    Ok(args)
}
