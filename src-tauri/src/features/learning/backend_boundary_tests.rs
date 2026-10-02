use super::{
    curriculum::{
        advance_generation_job, cancel_generation_job, fail_generation_job, finish_generation_job,
        recover_interrupted_jobs, start_generation_job, validate_generation_job,
        LearningGenerationJob, LearningGenerationJobKind, LearningGenerationJobStatus,
    },
    lab_runtime::{
        materialize_workspace, validate_execution_spec, LearningLabExecutionSpec, LearningLabFile,
        LearningLabLimits,
    },
};
use crate::shared::error::Result;

fn id(seed: u128) -> String {
    uuid::Uuid::from_u128(seed).to_string()
}

fn job(status: LearningGenerationJobStatus) -> LearningGenerationJob {
    LearningGenerationJob {
        id: id(1),
        program_id: id(2),
        operation_id: id(3),
        kind: LearningGenerationJobKind::LessonPreparation,
        payload_sha256: "a".repeat(64),
        base_revision_number: 4,
        status,
        progress_completed: 0,
        progress_total: 3,
        progress_message: "Queued".into(),
        result_id: None,
        error: None,
        retry_of_job_id: None,
        created_at: 10,
        started_at: None,
        finished_at: None,
    }
}

fn lab_spec(files: Vec<LearningLabFile>) -> LearningLabExecutionSpec {
    LearningLabExecutionSpec {
        run_id: id(10),
        image_id: format!("sha256:{}", "a".repeat(64)),
        command: vec!["python".into(), "tests/run.py".into()],
        files,
        read_only_paths: vec![],
        limits: LearningLabLimits::default(),
    }
}

#[test]
fn generation_job_failure_cancellation_and_restart_transitions_are_consistent() -> Result<()> {
    let mut failed = job(LearningGenerationJobStatus::Pending);
    start_generation_job(&mut failed, 20)?;
    advance_generation_job(&mut failed, 2)?;
    assert!(advance_generation_job(&mut failed, 1).is_err());
    assert!(finish_generation_job(&mut failed, id(4), 25).is_ok());

    // A finished job cannot also be failed or cancelled, even if its caller
    // retries an operation after receiving the completion result.
    assert!(fail_generation_job(&mut failed, "late failure".into(), 30).is_err());
    assert!(cancel_generation_job(&mut failed, 30).is_err());
    assert_eq!(failed.status, LearningGenerationJobStatus::Completed);
    assert_eq!(failed.progress_completed, failed.progress_total);
    assert!(failed.result_id.is_some());
    validate_generation_job(&failed)?;

    let mut cancelled = job(LearningGenerationJobStatus::Pending);
    cancel_generation_job(&mut cancelled, 40)?;
    assert_eq!(cancelled.status, LearningGenerationJobStatus::Cancelled);
    assert_eq!(cancelled.finished_at, Some(40));
    assert!(cancelled.result_id.is_none());
    assert!(start_generation_job(&mut cancelled, 50).is_err());

    let mut errored = job(LearningGenerationJobStatus::Running);
    errored.started_at = Some(15);
    fail_generation_job(&mut errored, "model returned invalid JSON".into(), 60)?;
    assert_eq!(
        errored.error.as_deref(),
        Some("model returned invalid JSON")
    );
    assert_eq!(errored.finished_at, Some(60));
    assert!(errored.result_id.is_none());

    let mut recovered = job(LearningGenerationJobStatus::Running);
    recovered.started_at = Some(25);
    recovered.result_id = Some(id(5));
    recover_interrupted_jobs(std::slice::from_mut(&mut recovered));
    assert_eq!(recovered.status, LearningGenerationJobStatus::Interrupted);
    assert!(recovered.finished_at.is_some());
    assert!(recovered.error.is_some());
    assert!(recovered.result_id.is_none());
    start_generation_job(&mut recovered, 70)?;
    assert_eq!(recovered.status, LearningGenerationJobStatus::Running);
    assert!(recovered.error.is_none());
    assert!(recovered.finished_at.is_none());
    Ok(())
}

#[test]
fn generation_job_validation_rejects_inconsistent_terminal_records() {
    let mut value = job(LearningGenerationJobStatus::Completed);
    assert!(validate_generation_job(&value).is_err());
    value.finished_at = Some(30);
    assert!(validate_generation_job(&value).is_err());
    value.result_id = Some(id(4));
    assert!(validate_generation_job(&value).is_err());
    value.progress_completed = value.progress_total;
    assert!(validate_generation_job(&value).is_ok());

    value.status = LearningGenerationJobStatus::Failed;
    value.error = None;
    assert!(validate_generation_job(&value).is_err());
    value.error = Some("provider failure".into());
    assert!(validate_generation_job(&value).is_err());
    value.result_id = None;
    assert!(validate_generation_job(&value).is_ok());

    value.status = LearningGenerationJobStatus::Running;
    value.finished_at = None;
    value.error = None;
    assert!(validate_generation_job(&value).is_ok());
    value.error = Some("stale terminal error".into());
    assert!(validate_generation_job(&value).is_err());

    value.error = None;
    value.operation_id = "not-an-operation-id".into();
    assert!(validate_generation_job(&value).is_err());
}

#[test]
fn lab_spec_rejects_aliases_and_accepts_resource_limit_edges() {
    let duplicate = lab_spec(vec![
        LearningLabFile {
            path: "src/main.py".into(),
            content: "one".into(),
        },
        LearningLabFile {
            path: "src/main.py".into(),
            content: "two".into(),
        },
    ]);
    assert!(validate_execution_spec(&duplicate).is_err());

    let mut repeated_protected_path = lab_spec(vec![LearningLabFile {
        path: "checks/run.py".into(),
        content: "check".into(),
    }]);
    repeated_protected_path.read_only_paths = vec!["checks/run.py".into(); 2];
    assert!(validate_execution_spec(&repeated_protected_path).is_err());

    let mut boundaries = lab_spec(vec![LearningLabFile {
        path: "src/min.py".into(),
        content: String::new(),
    }]);
    boundaries.limits = LearningLabLimits {
        timeout_seconds: 1,
        memory_megabytes: 32,
        cpu_millis: 100,
        process_limit: 8,
        output_bytes: 4_096,
    };
    assert!(validate_execution_spec(&boundaries).is_ok());
    boundaries.limits.output_bytes = 4_095;
    assert!(validate_execution_spec(&boundaries).is_err());
}

#[cfg(unix)]
#[test]
fn lab_materialization_rejects_a_symlinked_parent_that_escapes_workspace() -> anyhow::Result<()> {
    use std::os::unix::fs::symlink;

    let parent = tempfile::tempdir()?;
    let outside = parent.path().join("outside");
    std::fs::create_dir(&outside)?;
    let root = parent.path().join("run");
    std::fs::create_dir(&root)?;
    symlink(&outside, root.join("linked"))?;

    let spec = lab_spec(vec![LearningLabFile {
        path: "linked/escaped.py".into(),
        content: "should never be written".into(),
    }]);
    assert!(materialize_workspace(&root, &spec).is_err());
    assert!(!outside.join("escaped.py").exists());
    Ok(())
}
