//! Contained execution primitives for Learning Studio practical work.
//!
//! This module deliberately does not expose a host-shell escape hatch. A lab
//! stores a trusted argv template and a locally resolved immutable container
//! image ID; learner input is written only into a fresh per-run workspace.
//! The command builder is kept pure so every containment flag is covered by
//! deterministic tests on every target, including hosts without Docker.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_FILES: usize = 64;
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_ARG_CHARS: usize = 2_000;
const MAX_ARGS: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningContainerEngine {
    Docker,
    Podman,
}

impl LearningContainerEngine {
    pub fn executable(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    /// Build an engine command that works both from a login shell and from a
    /// desktop launch whose PATH omits common package-manager/app locations.
    pub(crate) fn command(self) -> Command {
        let path = std::env::var_os("PATH");
        let home = engine_home_dir();
        let candidates = known_engine_locations(self, home.as_deref());
        let filename = engine_filename(self);
        let executable = resolve_engine_executable(&filename, path.as_deref(), &candidates)
            .unwrap_or_else(|| unresolved_engine_path(self));
        let child_path = extended_engine_path(path.as_deref(), &candidates);
        let mut command = Command::new(executable);
        command.env("PATH", child_path);
        command
    }
}

#[cfg(windows)]
fn engine_filename(engine: LearningContainerEngine) -> OsString {
    OsString::from(format!("{}.exe", engine.executable()))
}

#[cfg(not(windows))]
fn engine_filename(engine: LearningContainerEngine) -> OsString {
    OsString::from(engine.executable())
}

#[cfg(windows)]
fn engine_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

#[cfg(not(windows))]
fn engine_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(windows)]
fn unresolved_engine_path(engine: LearningContainerEngine) -> PathBuf {
    PathBuf::from(format!(
        r"C:\__lattice_container_cli_unresolved__\{}.exe",
        engine.executable()
    ))
}

#[cfg(not(windows))]
fn unresolved_engine_path(engine: LearningContainerEngine) -> PathBuf {
    PathBuf::from(format!(
        "/__lattice_container_cli_unresolved__/{}",
        engine.executable()
    ))
}

fn is_usable_engine_executable(candidate: &Path) -> bool {
    if !candidate.is_absolute() || !candidate.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        candidate
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn resolve_engine_executable(
    filename: &OsStr,
    inherited_path: Option<&OsStr>,
    known_locations: &[PathBuf],
) -> Option<PathBuf> {
    inherited_path
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|directory| directory.join(filename))
        .chain(known_locations.iter().cloned())
        .find(|candidate| is_usable_engine_executable(candidate))
}

fn extended_engine_path(inherited_path: Option<&OsStr>, known_locations: &[PathBuf]) -> OsString {
    let mut directories = Vec::new();
    if let Some(path) = inherited_path {
        for directory in std::env::split_paths(path) {
            if directory.is_absolute() && !directories.contains(&directory) {
                directories.push(directory);
            }
        }
    }
    for directory in known_locations
        .iter()
        .filter_map(|location| location.parent())
    {
        if directory.is_absolute() && !directories.iter().any(|item| item == directory) {
            directories.push(directory.to_owned());
        }
    }
    std::env::join_paths(directories).unwrap_or_default()
}

fn classify_engine_exit(success: bool, exit_code: Option<i32>) -> LearningLabRunStatus {
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

fn known_engine_locations(engine: LearningContainerEngine, home: Option<&Path>) -> Vec<PathBuf> {
    let filename = engine_filename(engine);
    let mut locations = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home {
            locations.push(home.join(".docker/bin").join(&filename));
            let desktop_app = match engine {
                LearningContainerEngine::Docker => "Docker.app",
                LearningContainerEngine::Podman => "Podman Desktop.app",
            };
            locations.push(
                home.join("Applications")
                    .join(desktop_app)
                    .join("Contents/Resources/bin")
                    .join(&filename),
            );
        }
        let desktop_app = match engine {
            LearningContainerEngine::Docker => "Docker.app",
            LearningContainerEngine::Podman => "Podman Desktop.app",
        };
        locations.push(
            PathBuf::from("/Applications")
                .join(desktop_app)
                .join("Contents/Resources/bin")
                .join(&filename),
        );
        locations.push(PathBuf::from("/opt/homebrew/bin").join(&filename));
        locations.push(PathBuf::from("/usr/local/bin").join(&filename));
        if engine == LearningContainerEngine::Podman {
            locations.push(PathBuf::from("/opt/podman/bin").join(&filename));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home {
            locations.push(home.join(".docker/bin").join(&filename));
            locations.push(home.join(".local/bin").join(&filename));
        }
        locations.push(PathBuf::from("/usr/local/bin").join(&filename));
        locations.push(PathBuf::from("/usr/bin").join(&filename));
        locations.push(PathBuf::from("/snap/bin").join(&filename));
    }

    #[cfg(windows)]
    {
        if let Some(home) = home {
            locations.push(home.join(".docker/bin").join(&filename));
            if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
                let app = match engine {
                    LearningContainerEngine::Docker => "Docker/Docker/resources/bin",
                    LearningContainerEngine::Podman => "Programs/RedHat/Podman",
                };
                locations.push(PathBuf::from(local_app_data).join(app).join(&filename));
            }
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let app = match engine {
                LearningContainerEngine::Docker => "Docker/Docker/resources/bin",
                LearningContainerEngine::Podman => "RedHat/Podman",
            };
            locations.push(PathBuf::from(program_files).join(app).join(&filename));
        }
    }

    locations
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLabLimits {
    pub timeout_seconds: u64,
    pub memory_megabytes: u32,
    pub cpu_millis: u32,
    pub process_limit: u32,
    pub output_bytes: usize,
}

impl Default for LearningLabLimits {
    fn default() -> Self {
        Self {
            timeout_seconds: 20,
            memory_megabytes: 256,
            cpu_millis: 1_000,
            process_limit: 64,
            output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}

impl LearningLabLimits {
    pub fn validate(&self) -> Result<()> {
        if !(1..=120).contains(&self.timeout_seconds) {
            return Err(AppError::InvalidInput(
                "Lab timeout must be between 1 and 120 seconds.".into(),
            ));
        }
        if !(32..=2_048).contains(&self.memory_megabytes) {
            return Err(AppError::InvalidInput(
                "Lab memory must be between 32 and 2048 MB.".into(),
            ));
        }
        if !(100..=4_000).contains(&self.cpu_millis) {
            return Err(AppError::InvalidInput(
                "Lab CPU allowance must be between 100 and 4000 millicores.".into(),
            ));
        }
        if !(8..=256).contains(&self.process_limit) {
            return Err(AppError::InvalidInput(
                "Lab process limit must be between 8 and 256.".into(),
            ));
        }
        if !(4_096..=MAX_OUTPUT_BYTES).contains(&self.output_bytes) {
            return Err(AppError::InvalidInput(
                "Lab output allowance must be between 4096 and 262144 bytes.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLabFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLabExecutionSpec {
    pub run_id: String,
    /// A locally resolved Docker/Podman image ID, never an unpinned tag.
    pub image_id: String,
    /// Trusted argv authored by the selected lab template. Learner text is
    /// supplied through files and is never interpolated into these arguments.
    pub command: Vec<String>,
    pub files: Vec<LearningLabFile>,
    /// Evaluator and reference files are materialized with the run snapshot,
    /// then independently over-mounted read-only inside the container. Keeping
    /// this list in the trusted execution spec prevents learner code from
    /// replacing the checks while preserving a writable project workspace.
    pub read_only_paths: Vec<String>,
    pub limits: LearningLabLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLabRuntimeCapability {
    pub engine: LearningContainerEngine,
    pub available: bool,
    pub version: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningLabRunStatus {
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    RuntimeUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningLabExecutionResult {
    pub status: LearningLabRunStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub duration_ms: i64,
}

fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput(format!("Invalid {label} ID")))
}

fn validate_image_id(value: &str) -> Result<()> {
    let digest = value.strip_prefix("sha256:").ok_or_else(|| {
        AppError::InvalidInput("A lab runtime must use a locally resolved sha256 image ID.".into())
    })?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::InvalidInput(
            "A lab runtime image ID must contain a 64-character SHA-256 digest.".into(),
        ));
    }
    Ok(())
}

/// Return a normalized relative path containing only ordinary components.
/// Windows prefixes, roots, `.` and `..` are rejected on every host so a pack
/// cannot become dangerous merely by being imported on another OS.
pub fn validate_lab_path(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.chars().count() > 240 || value.contains('\0') {
        return Err(AppError::InvalidInput(
            "Lab file paths must contain 1–240 safe characters.".into(),
        ));
    }
    if value.contains('\\')
        || value.contains("//")
        || value.starts_with('/')
        || value.contains(':')
        || value.contains(',')
    {
        return Err(AppError::InvalidInput(
            "Lab file paths must be portable relative paths.".into(),
        ));
    }
    if value.split('/').next().is_some_and(|part| {
        [".lattice-build", "node_modules"]
            .iter()
            .any(|reserved| part.eq_ignore_ascii_case(reserved))
    }) {
        return Err(AppError::InvalidInput(
            "The .lattice-build and node_modules directories are reserved for the runtime.".into(),
        ));
    }
    let path = Path::new(value);
    let mut normal = 0;
    for component in path.components() {
        match component {
            Component::Normal(part) if part != OsStr::new("") => normal += 1,
            _ => {
                return Err(AppError::InvalidInput(
                    "Lab file paths cannot escape the run workspace.".into(),
                ))
            }
        }
    }
    if normal == 0 {
        return Err(AppError::InvalidInput(
            "Lab file paths must name a file.".into(),
        ));
    }
    Ok(path.to_path_buf())
}

pub fn validate_execution_spec(spec: &LearningLabExecutionSpec) -> Result<()> {
    uuid(&spec.run_id, "lab run")?;
    validate_image_id(&spec.image_id)?;
    spec.limits.validate()?;
    if spec.command.is_empty() || spec.command.len() > MAX_ARGS {
        return Err(AppError::InvalidInput(format!(
            "A lab command must contain 1–{MAX_ARGS} arguments."
        )));
    }
    for argument in &spec.command {
        if argument.is_empty()
            || argument.contains('\0')
            || argument.chars().count() > MAX_ARG_CHARS
        {
            return Err(AppError::InvalidInput(
                "A lab command contains an invalid or oversized argument.".into(),
            ));
        }
    }
    validate_lab_files(
        &spec.run_id,
        &spec.files,
        &spec.read_only_paths,
        &spec.limits,
    )
}

/// Shared input boundary for embedded and container providers.
pub fn validate_lab_files(
    run_id: &str,
    files: &[LearningLabFile],
    read_only_paths: &[String],
    limits: &LearningLabLimits,
) -> Result<()> {
    uuid(run_id, "lab run")?;
    limits.validate()?;
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(AppError::InvalidInput(format!(
            "A lab run must contain 1–{MAX_FILES} files."
        )));
    }
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for file in files {
        let path = validate_lab_path(&file.path)?;
        let size = file.content.len();
        if size > MAX_FILE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "Lab file '{}' exceeds 256 KiB.",
                file.path
            )));
        }
        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err(AppError::InvalidInput(
                "Lab files exceed the 2 MiB run limit.".into(),
            ));
        }
        if !seen.insert(path) {
            return Err(AppError::InvalidInput(
                "A lab run cannot contain duplicate file paths.".into(),
            ));
        }
    }
    if read_only_paths.len() > files.len() {
        return Err(AppError::InvalidInput(
            "A lab run has more read-only paths than files.".into(),
        ));
    }
    let mut read_only = HashSet::new();
    for value in read_only_paths {
        let path = validate_lab_path(value)?;
        if !read_only.insert(path.clone()) {
            return Err(AppError::InvalidInput(
                "A lab run cannot repeat a read-only path.".into(),
            ));
        }
        if !seen.contains(&path) {
            return Err(AppError::InvalidInput(
                "Every read-only lab path must reference a materialized file.".into(),
            ));
        }
    }
    Ok(())
}

fn container_name(run_id: &str) -> String {
    format!("lattice-learning-{}", run_id.replace('-', ""))
}

const WORKSPACE_BOOTSTRAP: &str = "cp -R -n -- /input/. /workspace/ && exec \"$@\"";
const WORKSPACE_BOOTSTRAP_ARG0: &str = "lattice-learning-stage-input";

fn workspace_tmpfs_size(limits: &LearningLabLimits) -> u32 {
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

async fn bounded_read<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(limit.min(16 * 1024));
    let mut chunk = [0u8; 8 * 1024];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        if let Some(retained) = chunk.get(..read.min(remaining)) {
            output.extend_from_slice(retained);
        }
        if read > remaining || output.len() == limit {
            truncated = true;
            // Keep draining so a child cannot deadlock on a full pipe.
        }
    }
    Ok((output, truncated))
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
fn bound_combined_output(stdout: &mut String, stderr: &mut String, limit: usize) -> bool {
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
    let stdout_task = tokio::spawn(bounded_read(stdout, output_limit));
    let stderr_task = tokio::spawn(bounded_read(stderr, output_limit));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_executable_resolution_handles_gui_path_and_missing_cli_without_env_mutation() {
        let temporary = tempfile::tempdir().unwrap();
        let gui_path = temporary.path().join("minimal-gui-path");
        let home = temporary.path().join("home");
        std::fs::create_dir_all(&gui_path).unwrap();
        #[cfg(unix)]
        std::fs::write(
            gui_path.join(engine_filename(LearningContainerEngine::Docker)),
            "not executable",
        )
        .unwrap();
        let docker_cli = home
            .join(".docker/bin")
            .join(engine_filename(LearningContainerEngine::Docker));
        std::fs::create_dir_all(docker_cli.parent().unwrap()).unwrap();
        std::fs::write(&docker_cli, "stub").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&docker_cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let minimal_path = std::env::join_paths([&gui_path]).unwrap();
        let fallback_locations =
            known_engine_locations(LearningContainerEngine::Docker, Some(home.as_path()));
        assert_eq!(
            resolve_engine_executable(
                &engine_filename(LearningContainerEngine::Docker),
                Some(&minimal_path),
                &fallback_locations,
            ),
            Some(docker_cli.clone())
        );

        let mixed_path = std::env::join_paths([
            gui_path.clone(),
            PathBuf::from("relative-bin"),
            PathBuf::new(),
        ])
        .unwrap();
        let child_path = extended_engine_path(Some(&mixed_path), &fallback_locations);
        let child_directories = std::env::split_paths(&child_path).collect::<Vec<_>>();
        assert!(child_directories.contains(&gui_path));
        assert!(child_directories
            .iter()
            .any(|directory| directory == docker_cli.parent().unwrap()));
        assert!(child_directories
            .iter()
            .all(|directory| directory.is_absolute()));

        let empty_path = std::env::join_paths([temporary.path().join("empty-path")]).unwrap();
        assert_eq!(
            resolve_engine_executable(
                &engine_filename(LearningContainerEngine::Docker),
                Some(&empty_path),
                &[],
            ),
            None,
            "missing engine CLI must remain unresolved without consulting process-global PATH",
        );
    }

    #[test]
    fn engine_exit_codes_distinguish_runtime_startup_failures_from_learner_failures() {
        assert_eq!(
            classify_engine_exit(true, Some(0)),
            LearningLabRunStatus::Passed
        );
        for code in 125..=127 {
            assert_eq!(
                classify_engine_exit(false, Some(code)),
                LearningLabRunStatus::RuntimeUnavailable
            );
        }
        assert_eq!(
            classify_engine_exit(false, Some(1)),
            LearningLabRunStatus::Failed
        );
        assert_eq!(
            classify_engine_exit(false, None),
            LearningLabRunStatus::Failed
        );
    }

    fn spec() -> LearningLabExecutionSpec {
        LearningLabExecutionSpec {
            run_id: "8b915770-f23a-4a86-8d89-77334ce192d6".into(),
            image_id: format!("sha256:{}", "a".repeat(64)),
            command: vec!["python".into(), "-I".into(), "tests/run.py".into()],
            files: vec![LearningLabFile {
                path: "src/answer.py".into(),
                content: "print('hello')\n".into(),
            }],
            read_only_paths: vec!["src/answer.py".into()],
            limits: LearningLabLimits::default(),
        }
    }

    #[test]
    fn rejects_every_portable_path_escape_shape() {
        for path in [
            "../secret",
            "src/../../secret",
            "/etc/passwd",
            "C:/Windows/system.ini",
            "C:\\Windows\\system.ini",
            "./answer.py",
            "src//answer.py",
            "src/answer,copy.py",
            "",
        ] {
            assert!(validate_lab_path(path).is_err(), "accepted {path:?}");
        }
        assert_eq!(
            validate_lab_path("src/lib/answer.py").unwrap(),
            PathBuf::from("src/lib/answer.py")
        );
    }

    #[test]
    fn command_contains_every_containment_and_resource_flag() {
        let args = container_run_args(&spec(), Path::new("/tmp/lattice-lab")).unwrap();
        let text = args
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>();
        for sequence in [
            vec!["--pull", "never"],
            vec!["--network", "none"],
            vec!["--read-only", "--cap-drop", "ALL"],
            vec!["--security-opt", "no-new-privileges=true"],
            vec!["--pids-limit", "64"],
            vec!["--memory", "256m", "--memory-swap", "256m"],
            vec!["--cpus", "1.000"],
            vec!["--ulimit", "nofile=1024:1024"],
            vec!["--user", "65534:65534"],
            vec!["--workdir", "/workspace"],
        ] {
            assert!(
                text.windows(sequence.len())
                    .any(|window| window == sequence.as_slice()),
                "missing {sequence:?} in {text:?}"
            );
        }
        assert!(text
            .iter()
            .any(|arg| { arg == "type=bind,src=/tmp/lattice-lab,dst=/input,readonly" }));
        assert!(text
            .iter()
            .any(|arg| { arg == "/workspace:rw,exec,nosuid,nodev,size=64m,mode=1777" }));
        assert!(text
            .iter()
            .any(|arg| arg == "/tmp:rw,noexec,nosuid,nodev,size=64m,mode=1777"));
        assert!(text.iter().any(|arg| {
            arg == "type=bind,src=/tmp/lattice-lab/src/answer.py,dst=/workspace/src/answer.py,readonly"
        }));
        let image_index = text
            .iter()
            .position(|arg| arg.starts_with("sha256:"))
            .unwrap();
        let protected_mount_index = text
            .iter()
            .position(|arg| {
                arg == "type=bind,src=/tmp/lattice-lab/src/answer.py,dst=/workspace/src/answer.py,readonly"
            })
            .unwrap();
        assert!(protected_mount_index < image_index);
        let container_command = text
            .get(image_index.saturating_add(1)..)
            .unwrap_or_default();
        assert_eq!(
            container_command,
            [
                "/bin/sh",
                "-c",
                WORKSPACE_BOOTSTRAP,
                WORKSPACE_BOOTSTRAP_ARG0,
                "python",
                "-I",
                "tests/run.py"
            ]
        );
        assert_eq!(workspace_tmpfs_size(&spec().limits), 64);
        assert_eq!(
            workspace_tmpfs_size(&LearningLabLimits {
                memory_megabytes: 32,
                ..LearningLabLimits::default()
            }),
            8
        );
        assert!(container_run_args(&spec(), Path::new("/tmp/lattice,lab")).is_err());
    }

    #[test]
    fn combined_output_limit_counts_lossy_utf8_bytes_and_prefers_stdout() {
        let mut stdout = String::from("🙂ab");
        let mut stderr = String::from("ézz");
        assert!(bound_combined_output(&mut stdout, &mut stderr, 4));
        assert_eq!(stdout, "🙂");
        assert!(stderr.is_empty());
        assert!(stdout.len() + stderr.len() <= 4);

        let mut stdout = String::from_utf8_lossy(&[0xff, 0xff]).into_owned();
        let mut stderr = String::from("tail");
        assert!(bound_combined_output(&mut stdout, &mut stderr, 4));
        assert!(stdout.len() + stderr.len() <= 4);
    }

    #[test]
    fn requires_an_immutable_local_image_and_bounded_payload() {
        let mut value = spec();
        value.image_id = "python:latest".into();
        assert!(validate_execution_spec(&value).is_err());
        let mut value = spec();
        value.image_id = format!("sha256:{}", "A".repeat(64));
        assert!(validate_execution_spec(&value).is_err());
        let mut value = spec();
        value.command.push("x".repeat(MAX_ARG_CHARS + 1));
        assert!(validate_execution_spec(&value).is_err());
        let mut value = spec();
        value.files[0].content = "x".repeat(MAX_FILE_BYTES + 1);
        assert!(validate_execution_spec(&value).is_err());
        let mut value = spec();
        value.read_only_paths = vec!["missing.py".into()];
        assert!(validate_execution_spec(&value).is_err());
    }

    #[test]
    fn materialization_never_overwrites_existing_files() -> anyhow::Result<()> {
        let parent = tempfile::tempdir()?;
        let root = parent.path().join("run");
        let value = spec();
        materialize_workspace(&root, &value)?;
        assert_eq!(
            std::fs::read_to_string(root.join("src/answer.py"))?,
            "print('hello')\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&root)?.permissions().mode() & 0o777,
                0o755
            );
            assert_eq!(
                std::fs::metadata(root.join("src"))?.permissions().mode() & 0o777,
                0o755
            );
            assert_eq!(
                std::fs::metadata(root.join("src/answer.py"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o644
            );
        }
        assert!(materialize_workspace(&root, &value).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn bounded_reader_drains_but_retains_only_the_limit() -> anyhow::Result<()> {
        let (mut writer, reader) = tokio::io::duplex(128);
        let write = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            writer.write_all(&vec![b'x'; 10_000]).await.unwrap();
        });
        let (bytes, truncated) = bounded_read(reader, 1_024).await?;
        write.await?;
        assert_eq!(bytes.len(), 1_024);
        assert!(truncated);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires an explicitly selected, already-local POSIX container image"]
    async fn opt_in_container_smoke_proves_runtime_containment_and_read_only_checks(
    ) -> anyhow::Result<()> {
        let image_id = std::env::var("LATTICE_LAB_SMOKE_IMAGE_ID").expect(
            "Set LATTICE_LAB_SMOKE_IMAGE_ID to an already-local sha256 image ID; this test never pulls.",
        );
        let engine = match std::env::var("LATTICE_LAB_SMOKE_ENGINE")
            .unwrap_or_else(|_| "docker".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "docker" => LearningContainerEngine::Docker,
            "podman" => LearningContainerEngine::Podman,
            value => anyhow::bail!("unsupported LATTICE_LAB_SMOKE_ENGINE={value}"),
        };
        let capability = detect_container_engine(engine).await;
        anyhow::ensure!(
            capability.available,
            "{} is unavailable: {}",
            engine.executable(),
            capability.reason.unwrap_or_default()
        );
        let parent = tempfile::tempdir()?;
        let root = parent.path().join("run");
        let spec = LearningLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            image_id,
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "if printf tampered > checks/guard.txt 2>/dev/null; then exit 91; fi; \
                 if printf root-write > /lattice-root-write 2>/dev/null; then exit 92; fi; \
                 if printf host-write > /input/work/input.txt 2>/dev/null; then exit 93; fi; \
                 printf '#!/bin/sh\\nexit 0\\n' > /tmp/noexec-smoke.sh; chmod +x /tmp/noexec-smoke.sh; \
                 if /tmp/noexec-smoke.sh 2>/dev/null; then exit 94; fi; \
                 printf '#!/bin/sh\\nexit 0\\n' > work/exec-smoke.sh; chmod +x work/exec-smoke.sh; work/exec-smoke.sh || exit 95; \
                 printf 'guest-change\\n' > work/input.txt; \
                 printf 'learner-write\\n' > work/new-output.txt; \
                 printf 'GUARD_OK\\n'; cat checks/guard.txt; \
                 printf 'STATUS\\n'; cat /proc/self/status; \
                 printf 'NETWORK\\n'; cat /proc/net/dev; \
                 printf '%300000s' '€'; printf '%300000s' '€' >&2"
                    .into(),
            ],
            files: vec![
                LearningLabFile {
                    path: "work/input.txt".into(),
                    content: "host-intact\n".into(),
                },
                LearningLabFile {
                    path: "checks/guard.txt".into(),
                    content: "guard-intact\n".into(),
                },
            ],
            read_only_paths: vec!["checks/guard.txt".into()],
            limits: LearningLabLimits {
                timeout_seconds: 20,
                ..LearningLabLimits::default()
            },
        };
        materialize_workspace(&root, &spec)?;
        let result = execute_container_lab(engine, &root, &spec, CancellationToken::new()).await?;
        assert_eq!(result.status, LearningLabRunStatus::Passed, "{result:?}");
        assert!(result.stdout.contains("GUARD_OK\nguard-intact"));
        assert!(result.stdout.contains("NoNewPrivs:\t1"));
        assert!(result.stdout.contains("CapEff:\t0000000000000000"));
        let network = result.stdout.split("NETWORK\n").nth(1).unwrap_or_default();
        assert!(network.contains("lo:"), "{network}");
        assert!(!network.contains("eth0:"), "{network}");
        assert_eq!(
            std::fs::read_to_string(root.join("checks/guard.txt"))?,
            "guard-intact\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("work/input.txt"))?,
            "host-intact\n"
        );
        assert!(!root.join("work/new-output.txt").exists());
        assert!(result.output_truncated);
        assert!(result.stdout.len() + result.stderr.len() <= spec.limits.output_bytes);

        let cancelled_root = parent.path().join("cancelled-run");
        let cancelled_spec = LearningLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            image_id: spec.image_id.clone(),
            command: vec!["/bin/sh".into(), "-c".into(), "while :; do :; done".into()],
            files: vec![LearningLabFile {
                path: "work/input.txt".into(),
                content: "cancel me\n".into(),
            }],
            read_only_paths: vec![],
            limits: spec.limits.clone(),
        };
        materialize_workspace(&cancelled_root, &cancelled_spec)?;
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            trigger.cancel();
        });
        let cancelled =
            execute_container_lab(engine, &cancelled_root, &cancelled_spec, cancellation).await?;
        cancel_task.await?;
        assert_eq!(cancelled.status, LearningLabRunStatus::Cancelled);
        let inspect = engine
            .command()
            .args(["inspect", &container_name(&cancelled_spec.run_id)])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
        assert!(
            !inspect.success(),
            "cancelled container remained registered"
        );

        let missing_image_root = parent.path().join("missing-image-run");
        let missing_image_spec = LearningLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            image_id: format!("sha256:{}", "f".repeat(64)),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf SHOULD_NOT_RUN".into(),
            ],
            files: vec![LearningLabFile {
                path: "work/input.txt".into(),
                content: "preserve learner data\n".into(),
            }],
            read_only_paths: vec![],
            limits: spec.limits.clone(),
        };
        materialize_workspace(&missing_image_root, &missing_image_spec)?;
        let missing_image = execute_container_lab(
            engine,
            &missing_image_root,
            &missing_image_spec,
            CancellationToken::new(),
        )
        .await?;
        assert_eq!(
            missing_image.status,
            LearningLabRunStatus::RuntimeUnavailable
        );
        assert_eq!(missing_image.exit_code, Some(125));
        assert!(!missing_image.stdout.contains("SHOULD_NOT_RUN"));
        assert!(!missing_image.stderr.is_empty());
        assert_eq!(
            std::fs::read_to_string(missing_image_root.join("work/input.txt"))?,
            "preserve learner data\n"
        );
        Ok(())
    }
}
