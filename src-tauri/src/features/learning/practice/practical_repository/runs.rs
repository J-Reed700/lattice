//! Contained runs: preparing, finishing, cancelling and cleaning them up.
use super::{
    check_status_name, engine_name, invalid, json, parse_engine, practical_operation, status_name,
    uuid, workspace_owner_token_from_snapshot, LearningPracticalRepository, PreparedExecution,
    PreparedRun,
};
use crate::features::learning::embedded_runtime::{
    BuiltinLabExecutionSpec, LearningBuiltinRuntime,
};
use crate::features::learning::lab_runtime::{
    self, LearningContainerEngine, LearningLabExecutionResult, LearningLabExecutionSpec,
    LearningLabFile, LearningLabLimits, LearningLabRunStatus,
};
use crate::features::learning::persistence::{db, now};
use crate::features::learning::practical_dto::*;
use crate::shared::error::{AppError, Result};
use sqlx::Row;
use std::collections::HashSet;

impl LearningPracticalRepository {
    pub(in crate::features::learning) async fn prepare_run(
        &self,
        request: &StartLearningPracticalRunRequestDto,
        payload_hash: &str,
    ) -> Result<Option<PreparedRun>> {
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.run_id, "run"),
            (&request.program_id, "program"),
            (&request.activity_id, "activity"),
        ] {
            uuid(value, label)?;
        }
        if let Some(session_id) = request.practice_session_id.as_deref() {
            uuid(session_id, "practice session")?;
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "practical_run",
            &request.operation_id,
            Some(&request.program_id),
            &request.run_id,
            "start",
            payload_hash,
            "Operation ID was reused with different run data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(None);
        }
        Self::active_program(&mut tx, &request.program_id).await?;
        let activity =
            Self::internal_activity(&mut tx, &request.program_id, &request.activity_id).await?;
        if activity.revision != request.expected_activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        if activity.runtime_profile_id.is_some() && !activity.runtime_enabled {
            return Err(AppError::ServiceNotAvailable(
                "The practical activity's runtime profile is disabled.".into(),
            ));
        }
        if let Some(session_id) = request.practice_session_id.as_deref() {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_practice_sessions WHERE program_id=? AND id=?",
            )
            .bind(&request.program_id)
            .bind(session_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid("Practice session does not belong to this program."));
            }
        }
        let required = activity
            .files
            .iter()
            .filter(|file| file.role == "starter" && file.editable)
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        let reserved = activity
            .files
            .iter()
            .filter(|file| file.role != "starter")
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        let learner_paths = request
            .learner_files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        if !required.is_subset(&learner_paths) {
            return Err(invalid("The run is missing an editable starter file."));
        }
        if learner_paths.iter().any(|path| reserved.contains(path)) {
            return Err(invalid(
                "Learner files cannot replace reference or evaluator files.",
            ));
        }
        let protected_files = activity
            .files
            .iter()
            .filter(|file| file.role == "check" || file.role == "reference")
            .collect::<Vec<_>>();
        let read_only_paths = protected_files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let mut execution_files = request.learner_files.clone();
        execution_files.extend(protected_files.into_iter().map(|file| LearningLabFile {
            path: file.path.clone(),
            content: file.content.clone(),
        }));
        let execution = if let Some(runtime) = activity.builtin_runtime {
            let spec = BuiltinLabExecutionSpec {
                run_id: request.run_id.clone(),
                runtime,
                files: execution_files,
                read_only_paths,
                limits: LearningLabLimits::default(),
            };
            spec.validate()?;
            PreparedExecution::Builtin {
                spec,
                version: activity
                    .builtin_runtime_version
                    .clone()
                    .ok_or_else(|| invalid("Built-in activity lost its runtime version."))?,
            }
        } else {
            let engine = activity.runtime_engine.ok_or_else(|| {
                AppError::ServiceNotAvailable(
                    "This practical activity does not have a local execution runtime.".into(),
                )
            })?;
            let spec = LearningLabExecutionSpec {
                run_id: request.run_id.clone(),
                image_id: activity.runtime_image_id.clone().ok_or_else(|| {
                    AppError::InvalidState("Practical activity lost its runtime image.".into())
                })?,
                command: activity.runtime_command.clone().ok_or_else(|| {
                    AppError::InvalidState("Practical activity lost its runtime command.".into())
                })?,
                files: execution_files,
                read_only_paths,
                limits: activity.runtime_limits.clone().ok_or_else(|| {
                    AppError::InvalidState("Practical activity lost its runtime limits.".into())
                })?,
            };
            lab_runtime::validate_execution_spec(&spec)?;
            PreparedExecution::Container { engine, spec }
        };
        let workspace_owner_token = match &execution {
            PreparedExecution::Container { .. } => Some(uuid::Uuid::new_v4().to_string()),
            PreparedExecution::Builtin { .. } => None,
        };
        let timestamp = now();
        let mut run_snapshot = activity.clone();
        run_snapshot.workspace_owner_token = workspace_owner_token.clone();
        let snapshot = json(&run_snapshot)?;
        sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,engine,image_id,status,learner_files_json,created_at,builtin_runtime,builtin_runtime_version) VALUES(?,?,?,?,?,?,?,?,?,?,'running',?,?,?,?)")
            .bind(&request.run_id).bind(&request.program_id).bind(&request.activity_id)
            .bind(activity.revision).bind(snapshot).bind(&request.practice_session_id)
            .bind(&request.operation_id).bind(payload_hash).bind(activity.runtime_engine.map(engine_name)).bind(&activity.runtime_image_id)
            .bind(json(&request.learner_files)?).bind(timestamp).bind(activity.builtin_runtime.map(LearningBuiltinRuntime::as_str))
            .bind(&activity.builtin_runtime_version)
            .execute(&mut *tx).await.map_err(db)?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        let dto = self.run(&request.run_id).await?;
        Ok(Some(PreparedRun {
            dto,
            execution,
            outcome_ids: activity.outcome_ids,
            practice_mode: activity.practice_mode,
            allowed_aids: activity.allowed_aids,
            workspace_owner_token,
        }))
    }

    pub(in crate::features::learning) async fn finish_run(
        &self,
        prepared: &PreparedRun,
        execution: LearningLabExecutionResult,
    ) -> Result<()> {
        let status = match execution.status {
            LearningLabRunStatus::Passed => LearningPracticalRunStatus::Passed,
            LearningLabRunStatus::Failed => LearningPracticalRunStatus::Failed,
            LearningLabRunStatus::TimedOut => LearningPracticalRunStatus::TimedOut,
            LearningLabRunStatus::Cancelled => LearningPracticalRunStatus::Cancelled,
            LearningLabRunStatus::RuntimeUnavailable => {
                LearningPracticalRunStatus::RuntimeUnavailable
            }
        };
        let check_status = match status {
            LearningPracticalRunStatus::Passed => LearningPracticalCheckStatus::Passed,
            LearningPracticalRunStatus::Failed => LearningPracticalCheckStatus::Failed,
            LearningPracticalRunStatus::TimedOut
            | LearningPracticalRunStatus::Cancelled
            | LearningPracticalRunStatus::RuntimeUnavailable => {
                LearningPracticalCheckStatus::NotRun
            }
            _ => LearningPracticalCheckStatus::Error,
        };
        let message = match status {
            LearningPracticalRunStatus::Passed => "The contained evaluator completed successfully.",
            LearningPracticalRunStatus::Failed => "The contained evaluator reported a failure.",
            LearningPracticalRunStatus::TimedOut => {
                "The contained evaluator exceeded its time limit."
            }
            LearningPracticalRunStatus::Cancelled => {
                "The run was cancelled and its contained evaluator stopped."
            }
            LearningPracticalRunStatus::RuntimeUnavailable => {
                "No compatible local runtime was available; no evaluation occurred."
            }
            _ => "The evaluator did not complete.",
        };
        let check = LearningPracticalCheckResultDto {
            name: "Contained evaluator".into(),
            status: check_status,
            message: message.into(),
            duration_ms: Some(execution.duration_ms),
        };
        let timestamp = now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let updated = sqlx::query("UPDATE learning_practical_runs SET status=?,stdout=?,stderr=?,output_truncated=?,exit_code=?,duration_ms=?,check_results_json=?,completed_at=? WHERE id=? AND status='running'")
            .bind(status_name(&status)).bind(&execution.stdout).bind(&execution.stderr)
            .bind(execution.output_truncated as i64).bind(execution.exit_code.map(i64::from))
            .bind(execution.duration_ms).bind(json(&vec![check.clone()])?).bind(timestamp)
            .bind(&prepared.dto.id).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() == 1 {
            sqlx::query("INSERT INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,0,?,?,?,?)")
                .bind(&prepared.dto.id).bind(&check.name).bind(check_status_name(check.status))
                .bind(&check.message).bind(check.duration_ms).execute(&mut *tx).await.map_err(db)?;
            if !matches!(
                status,
                LearningPracticalRunStatus::RuntimeUnavailable
                    | LearningPracticalRunStatus::Cancelled
                    | LearningPracticalRunStatus::TimedOut
            ) {
                for outcome_id in &prepared.outcome_ids {
                    sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,'practical',?,'application',?,?,?,?,?,?)")
                        .bind(uuid::Uuid::new_v4().to_string()).bind(&prepared.dto.program_id).bind(outcome_id)
                        .bind(&prepared.dto.id)
                        .bind(if status==LearningPracticalRunStatus::Passed{"observed"}else{"not_observed"})
                        .bind(if status==LearningPracticalRunStatus::Passed{1.0}else{0.0})
                        .bind(message).bind(Option::<String>::None)
                        .bind(json(&serde_json::json!({"mode":prepared.practice_mode,"allowedAids":prepared.allowed_aids}))?)
                        .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
                }
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    pub async fn cancel_run(
        &self,
        request: &CancelLearningPracticalRunRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.run_id, "run")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "practical_run",
            &request.operation_id,
            Some(&request.program_id),
            &request.run_id,
            "cancel",
            payload_hash,
            "Operation ID was reused with different cancellation data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.run(&request.run_id).await;
        }
        let timestamp = now();
        let changed = sqlx::query("UPDATE learning_practical_runs SET status='cancelled',stderr='Run cancelled by the learner.',completed_at=? WHERE id=? AND program_id=? AND status IN ('pending','running')")
            .bind(timestamp).bind(&request.run_id).bind(&request.program_id)
            .execute(&mut *tx).await.map_err(db)?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_practical_runs WHERE id=? AND program_id=?")
                .bind(&request.run_id)
                .bind(&request.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Practical run not found".into()));
        }
        operation.record(&mut *tx, &()).await?;
        let changed = changed.rows_affected() == 1;
        if changed {
            sqlx::query("INSERT OR IGNORE INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,0,'Contained evaluator','not_run','The run was cancelled; its contained evaluator was told to stop.',NULL)")
                .bind(&request.run_id).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        self.run(&request.run_id).await
    }

    pub(in crate::features::learning) async fn run_cleanup(&self, id: &str) -> Result<RunCleanup> {
        let row = sqlx::query(
            "SELECT id,engine,activity_snapshot_json FROM learning_practical_runs WHERE id=?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        cleanup_record(&row)
    }

    pub(in crate::features::learning) async fn unfinished_runs(&self) -> Result<Vec<RunCleanup>> {
        let rows = sqlx::query("SELECT id,engine,activity_snapshot_json FROM learning_practical_runs WHERE status IN ('pending','running')")
            .fetch_all(&self.pool).await.map_err(db)?;
        rows.iter().map(cleanup_record).collect()
    }

    pub(in crate::features::learning) async fn mark_runs_interrupted(&self) -> Result<usize> {
        let changed = sqlx::query("UPDATE learning_practical_runs SET status='interrupted',stderr='The desktop process stopped before the run completed.',completed_at=? WHERE status IN ('pending','running')")
            .bind(now()).execute(&self.pool).await.map_err(db)?;
        Ok(changed.rows_affected() as usize)
    }
}

pub(in crate::features::learning) struct RunCleanup {
    pub id: String,
    pub engine: Option<LearningContainerEngine>,
    pub owner_token: Option<String>,
}

fn cleanup_record(row: &sqlx::sqlite::SqliteRow) -> Result<RunCleanup> {
    Ok(RunCleanup {
        id: row.get("id"),
        engine: row
            .get::<Option<String>, _>("engine")
            .as_deref()
            .map(parse_engine)
            .transpose()?,
        owner_token: workspace_owner_token_from_snapshot(row.get("activity_snapshot_json")),
    })
}
