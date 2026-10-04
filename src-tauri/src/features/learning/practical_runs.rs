//! Owns practical runtime execution and filesystem cleanup outside persistence.
use super::{
    embedded_runtime,
    lab_runtime::{self, LearningLabExecutionResult, LearningLabRunStatus},
    practical_dto::*,
    practical_repository::{LearningPracticalRepository, PreparedExecution, RunCleanup},
};
use crate::shared::{
    error::{AppError, Result},
    runtime::background,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};
use tokio_util::sync::CancellationToken;

static ACTIVE_RUNS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(Mutex::default);
struct ActiveRun(String);
impl Drop for ActiveRun {
    fn drop(&mut self) {
        ACTIVE_RUNS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}
fn register_run(id: &str, token: CancellationToken) -> Result<ActiveRun> {
    let mut active = ACTIVE_RUNS.lock().unwrap_or_else(|e| e.into_inner());
    // Do not replace the existing token when rejecting a duplicate runner.
    if active.contains_key(id) {
        return Err(AppError::ConcurrentModification {
            resource: "practical run".into(),
            details: "This run is already active.".into(),
        });
    }
    active.insert(id.into(), token);
    Ok(ActiveRun(id.into()))
}

pub async fn start_run(
    repo: &LearningPracticalRepository,
    request: &StartLearningPracticalRunRequestDto,
    payload_hash: &str,
) -> Result<LearningPracticalRunDto> {
    let (repo, request, hash) = (repo.clone(), request.clone(), payload_hash.to_owned());
    let task = background::spawn(async move { execute_run(&repo, &request, &hash).await })
        .ok_or_else(|| AppError::ServiceNotAvailable("Application is shutting down".into()))?;
    task.await
        .map_err(|error| AppError::InternalError(format!("Practical runner stopped: {error}")))?
}

pub async fn cancel_run(
    repo: &LearningPracticalRepository,
    request: &CancelLearningPracticalRunRequestDto,
    payload_hash: &str,
) -> Result<LearningPracticalRunDto> {
    let result = repo.cancel_run(request, payload_hash).await?;
    if result.status == LearningPracticalRunStatus::Cancelled {
        let active = ACTIVE_RUNS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&request.run_id)
            .cloned();
        if let Some(token) = &active {
            token.cancel();
        }
        cleanup_orphan(repo.run_cleanup(&request.run_id).await?, active.is_some()).await?;
    }
    Ok(result)
}

pub async fn recover_running_runs(repo: &LearningPracticalRepository) -> Result<usize> {
    for run in repo.unfinished_runs().await? {
        // Recovery remains best effort if a container engine is unavailable.
        let _ = cleanup_orphan(run, false).await;
    }
    repo.mark_runs_interrupted().await
}

async fn cleanup_orphan(run: RunCleanup, active: bool) -> Result<()> {
    if let Some(engine) = run.engine {
        lab_runtime::cleanup_interrupted_run(engine, &run.id).await?;
        if !active {
            if let Some(token) = run.owner_token {
                remove_owned_workspace(&run.id, &token);
            }
        }
    }
    Ok(())
}

async fn execute_run(
    repo: &LearningPracticalRepository,
    request: &StartLearningPracticalRunRequestDto,
    payload_hash: &str,
) -> Result<LearningPracticalRunDto> {
    let Some(prepared) = repo.prepare_run(request, payload_hash).await? else {
        return repo.run(&request.run_id).await;
    };
    let unavailable_reason = match &prepared.execution {
            PreparedExecution::Container { engine, .. } => {
                let capability = lab_runtime::detect_container_engine(*engine).await;
                (!capability.available).then(|| capability.reason.unwrap_or_else(|| "The selected container runtime is unavailable.".into()))
            },
            PreparedExecution::Builtin {spec, version} if version != spec.runtime.version() =>
                Some("The activity's built-in runtime version is no longer available. Generate a new activity to use the current runtime.".into()),
            PreparedExecution::Builtin {spec, ..} => embedded_runtime::capabilities().into_iter()
                .find(|capability| capability.id == spec.runtime && !capability.available)
                .map(|capability| capability.reason.unwrap_or_else(|| "The built-in runtime is unavailable.".into())),
        };
    if let Some(reason) = unavailable_reason {
        let result = LearningLabExecutionResult {
            status: LearningLabRunStatus::RuntimeUnavailable,
            exit_code: None,
            stdout: String::new(),
            stderr: reason,
            output_truncated: false,
            duration_ms: 0,
        };
        repo.finish_run(&prepared, result).await?;
        return repo.run(&request.run_id).await;
    }
    let token = background::cancellation_token().child_token();
    let _registration = register_run(&request.run_id, token.clone())?;
    // A cancellation may have committed between run creation and registry
    // registration. Never launch a container for a terminal run.
    if repo.run(&request.run_id).await?.status != LearningPracticalRunStatus::Running {
        return repo.run(&request.run_id).await;
    }
    let root = run_workspace(&request.run_id);
    let mut workspace_owned = false;
    let mut ownership_marker_created = false;
    let outcome = async {
        match &prepared.execution {
            PreparedExecution::Builtin { spec, .. } => {
                embedded_runtime::execute_builtin_lab(spec.clone(), token).await
            }
            PreparedExecution::Container { engine, spec } => {
                if let Some(parent) = root.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if root.exists() {
                    return Err(AppError::Security(
                        "A practical run workspace already exists unexpectedly.".into(),
                    ));
                }
                lab_runtime::materialize_workspace(&root, spec)?;
                // The workspace is ours only after materialization has
                // atomically created the previously absent directory.
                workspace_owned = true;
                let owner_token = prepared.workspace_owner_token.as_deref().ok_or_else(|| {
                    AppError::InvalidState(
                        "Container run is missing its workspace ownership token.".into(),
                    )
                })?;
                create_workspace_owner_marker(&request.run_id, owner_token)?;
                ownership_marker_created = true;
                lab_runtime::execute_container_lab(*engine, &root, spec, token).await
            }
        }
    }
    .await;
    cleanup_invocation_workspace(&request.run_id, workspace_owned, ownership_marker_created);
    let execution = match outcome {
        Ok(result) => result,
        Err(error) => LearningLabExecutionResult {
            status: if matches!(&error, AppError::ServiceNotAvailable(_)) {
                LearningLabRunStatus::RuntimeUnavailable
            } else {
                LearningLabRunStatus::Failed
            },
            exit_code: None,
            stdout: String::new(),
            stderr: error.to_string().chars().take(2_000).collect(),
            output_truncated: false,
            duration_ms: 0,
        },
    };
    repo.finish_run(&prepared, execution).await?;
    repo.run(&request.run_id).await
}

pub(super) fn run_workspace(run_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("lattice-learning-labs")
        .join(run_id)
}

pub(super) fn workspace_owner_marker(run_id: &str) -> PathBuf {
    run_workspace(run_id).with_extension("owner")
}

pub(super) fn cleanup_invocation_workspace(
    run_id: &str,
    workspace_owned: bool,
    ownership_marker_created: bool,
) {
    if !workspace_owned {
        return;
    }
    // This invocation created the directory. If marker creation failed,
    // direct cleanup is still safe because materialize_workspace succeeded.
    let _ = std::fs::remove_dir_all(run_workspace(run_id));
    if ownership_marker_created {
        let _ = std::fs::remove_file(workspace_owner_marker(run_id));
    }
}

fn create_workspace_owner_marker(run_id: &str, token: &str) -> Result<()> {
    use std::io::Write;
    let path = workspace_owner_marker(run_id);
    let mut marker = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    marker.write_all(token.as_bytes())?;
    marker.sync_all()?;
    Ok(())
}

pub(super) fn remove_owned_workspace(run_id: &str, token: &str) {
    let marker_path = workspace_owner_marker(run_id);
    if std::fs::read_to_string(&marker_path).is_ok_and(|marker| marker == token) {
        let _ = std::fs::remove_dir_all(run_workspace(run_id));
        let _ = std::fs::remove_file(marker_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_registration_preserves_the_running_cancellation_token() {
        let id = uuid::Uuid::new_v4().to_string();
        let original = CancellationToken::new();
        let guard = register_run(&id, original.clone()).unwrap();
        assert!(register_run(&id, CancellationToken::new()).is_err());
        ACTIVE_RUNS.lock().unwrap().get(&id).unwrap().cancel();
        assert!(original.is_cancelled());
        drop(guard);
        assert!(!ACTIVE_RUNS.lock().unwrap().contains_key(&id));
    }
}
