//! Contained execution primitives for Learning Studio practical work.
//!
//! This module deliberately does not expose a host-shell escape hatch. A lab
//! stores a trusted argv template and a locally resolved immutable container
//! image ID; learner input is written only into a fresh per-run workspace.
//! The command builder is kept pure so every containment flag is covered by
//! deterministic tests on every target, including hosts without Docker.

use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

mod command;
mod engine_path;
mod execution;
mod validation;

use command::container_name;
pub use command::container_run_args;
#[cfg(test)]
use command::{workspace_tmpfs_size, WORKSPACE_BOOTSTRAP, WORKSPACE_BOOTSTRAP_ARG0};
use engine_path::{
    engine_filename, engine_home_dir, extended_engine_path, known_engine_locations,
    resolve_engine_executable, unresolved_engine_path,
};
#[cfg(test)]
use execution::{bound_combined_output, classify_engine_exit};
pub use execution::{
    cleanup_interrupted_run, detect_container_engine, execute_container_lab, materialize_workspace,
};
pub use validation::{validate_execution_spec, validate_lab_files, validate_lab_path};

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

pub(super) async fn bounded_read<R: AsyncRead + Unpin>(
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

#[cfg(test)]
mod tests;
