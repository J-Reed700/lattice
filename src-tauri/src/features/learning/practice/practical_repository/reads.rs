//! Reading the practical workspace, runtime profiles, runs and simulations back into DTOs.
use super::{
    decode, parse_check_status, parse_engine, parse_kind, parse_mode, parse_run_status,
    parse_speaker, simulation_status, uuid, LearningPracticalRepository,
    PracticalWorkspaceSnapshot,
};
use crate::features::learning::embedded_runtime::LearningBuiltinRuntime;
use crate::features::learning::persistence::db;
use crate::features::learning::practical_dto::*;
use crate::shared::error::{AppError, Result};
use sqlx::Row;
use std::collections::HashMap;

impl LearningPracticalRepository {
    pub async fn workspace(&self, program_id: &str) -> Result<PracticalWorkspaceSnapshot> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let workspace = Self::workspace_on(&mut tx, program_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(workspace)
    }

    pub(in crate::features::learning) async fn workspace_on(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
    ) -> Result<PracticalWorkspaceSnapshot> {
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        let profiles = Self::profiles_on(connection).await?;
        let activity_rows = sqlx::query(
            "SELECT * FROM learning_practical_activities WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
        let mut activities = Vec::with_capacity(activity_rows.len());
        let mut builtin_versions = HashMap::new();
        for row in activity_rows {
            builtin_versions.insert(
                row.get::<String, _>("id"),
                row.get("builtin_runtime_version"),
            );
            activities.push(Self::parse_activity(connection, row).await?);
        }
        let run_rows = sqlx::query(
            "SELECT * FROM learning_practical_runs WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
        let mut runs = Vec::with_capacity(run_rows.len());
        for row in run_rows {
            runs.push(Self::parse_run(connection, row).await?);
        }
        let simulation_rows = sqlx::query(
            "SELECT * FROM learning_simulation_sessions WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
        let mut simulations = Vec::with_capacity(simulation_rows.len());
        for row in simulation_rows {
            simulations.push(Self::parse_simulation(connection, row).await?);
        }
        Ok(PracticalWorkspaceSnapshot {
            workspace: LearningPracticalWorkspaceDto {
                program_id: program_id.into(),
                builtin_runtimes: Vec::new(),
                runtime_capabilities: Vec::new(),
                runtime_profiles: profiles,
                activities,
                runs,
                simulations,
            },
            builtin_versions,
        })
    }

    pub(in crate::features::learning) async fn profiles(
        &self,
    ) -> Result<Vec<LearningRuntimeProfileDto>> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::profiles_on(&mut connection).await
    }

    async fn profiles_on(
        connection: &mut sqlx::SqliteConnection,
    ) -> Result<Vec<LearningRuntimeProfileDto>> {
        let rows = sqlx::query("SELECT * FROM learning_runtime_profiles ORDER BY name,id")
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        rows.into_iter()
            .map(|row| {
                Ok(LearningRuntimeProfileDto {
                    id: row.get("id"),
                    name: row.get("name"),
                    engine: parse_engine(row.get("engine"))?,
                    image_id: row.get("image_id"),
                    command: decode(row.get("command_json"))?,
                    limits: decode(row.get("limits_json"))?,
                    enabled: row.get::<i64, _>("enabled") != 0,
                    revision: row.get("revision"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                })
            })
            .collect()
    }

    async fn parse_activity(
        connection: &mut sqlx::SqliteConnection,
        row: sqlx::sqlite::SqliteRow,
    ) -> Result<LearningPracticalActivityDto> {
        let id: String = row.get("id");
        let file_rows = sqlx::query("SELECT * FROM learning_practical_files WHERE activity_id=? AND role IN ('starter','reference') ORDER BY ordinal")
            .bind(&id).fetch_all(&mut *connection).await.map_err(db)?;
        let files = file_rows
            .into_iter()
            .map(|file| {
                Ok(LearningPracticalFileDto {
                    path: file.get("path"),
                    role: match file.get::<String, _>("role").as_str() {
                        "starter" => LearningPracticalPublicFileRole::Starter,
                        "reference" => LearningPracticalPublicFileRole::Reference,
                        _ => {
                            return Err(AppError::Database(
                                "Hidden practical file leaked into public query".into(),
                            ))
                        }
                    },
                    content: file.get("content"),
                    content_sha256: file.get("content_sha256"),
                    editable: file.get::<i64, _>("editable") != 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let builtin_runtime = row
            .get::<Option<String>, _>("builtin_runtime")
            .as_deref()
            .map(LearningBuiltinRuntime::parse)
            .transpose()?;
        let runtime_kind = if builtin_runtime.is_some() {
            LearningPracticalRuntimeKind::Builtin
        } else if row.get::<String, _>("runtime_kind") == "container" {
            LearningPracticalRuntimeKind::Container
        } else {
            LearningPracticalRuntimeKind::None
        };
        let profile_id: Option<String> = row.get("runtime_profile_id");
        let frozen_engine = row
            .get::<Option<String>, _>("runtime_engine")
            .as_deref()
            .map(parse_engine)
            .transpose()?;
        Ok(LearningPracticalActivityDto {
            id,
            program_id: row.get("program_id"),
            lesson_id: row.get("lesson_id"),
            predecessor_id: row.get("predecessor_id"),
            kind: parse_kind(row.get("kind"))?,
            title: row.get("title"),
            brief: row.get("brief"),
            status: match row.get::<String, _>("status").as_str() {
                "draft" => LearningPracticalActivityStatus::Draft,
                "ready" => LearningPracticalActivityStatus::Ready,
                "retired" => LearningPracticalActivityStatus::Retired,
                _ => {
                    return Err(AppError::Database(
                        "Invalid practical activity status".into(),
                    ))
                }
            },
            practice_mode: parse_mode(row.get("practice_mode"))?,
            allowed_aids: decode(row.get("allowed_aids_json"))?,
            outcome_ids: decode(row.get("outcome_ids_json"))?,
            source_version_ids: decode(row.get("source_version_ids_json"))?,
            rubric: decode(row.get("rubric_json"))?,
            runtime_kind,
            builtin_runtime,
            runtime_profile_id: profile_id,
            runtime_engine: frozen_engine,
            runtime_image_id: row.get("runtime_image_id"),
            runtime_command: row
                .get::<Option<String>, _>("runtime_command_json")
                .as_deref()
                .map(decode)
                .transpose()?,
            runtime_limits: row
                .get::<Option<String>, _>("runtime_limits_json")
                .as_deref()
                .map(decode)
                .transpose()?,
            // The service fills these before the snapshot becomes a public DTO.
            runtime_available: false,
            runtime_unavailable_reason: None,
            generator_model: row.get("generator_model"),
            files,
            revision: row.get("revision"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    async fn parse_run(
        connection: &mut sqlx::SqliteConnection,
        row: sqlx::sqlite::SqliteRow,
    ) -> Result<LearningPracticalRunDto> {
        let id: String = row.get("id");
        let checks = sqlx::query(
            "SELECT * FROM learning_practical_run_checks WHERE run_id=? ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .into_iter()
        .map(|check| {
            Ok(LearningPracticalCheckResultDto {
                name: check.get("name"),
                status: parse_check_status(check.get("status"))?,
                message: check.get("message"),
                duration_ms: check.get("duration_ms"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
        Ok(LearningPracticalRunDto {
            id,
            program_id: row.get("program_id"),
            activity_id: row.get("activity_id"),
            activity_revision: row.get("activity_revision"),
            practice_session_id: row.get("practice_session_id"),
            status: parse_run_status(row.get("status"))?,
            builtin_runtime: row
                .get::<Option<String>, _>("builtin_runtime")
                .as_deref()
                .map(LearningBuiltinRuntime::parse)
                .transpose()?,
            engine: row
                .get::<Option<String>, _>("engine")
                .as_deref()
                .map(parse_engine)
                .transpose()?,
            image_id: row.get("image_id"),
            learner_files: decode(row.get("learner_files_json"))?,
            stdout: row.get("stdout"),
            stderr: row.get("stderr"),
            output_truncated: row.get::<i64, _>("output_truncated") != 0,
            exit_code: row
                .get::<Option<i64>, _>("exit_code")
                .map(|value| value as i32),
            duration_ms: row.get("duration_ms"),
            checks,
            created_at: row.get("created_at"),
            completed_at: row.get("completed_at"),
        })
    }

    async fn parse_simulation(
        connection: &mut sqlx::SqliteConnection,
        row: sqlx::sqlite::SqliteRow,
    ) -> Result<LearningSimulationSessionDto> {
        let id: String = row.get("id");
        let turns = sqlx::query(
            "SELECT * FROM learning_simulation_turns WHERE session_id=? ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .into_iter()
        .map(|turn| {
            Ok(LearningSimulationTurnDto {
                id: turn.get("id"),
                ordinal: turn.get("ordinal"),
                speaker: parse_speaker(turn.get("speaker"))?,
                content: turn.get("content"),
                citations: decode(turn.get("citations_json"))?,
                model_name: turn.get("model_name"),
                created_at: turn.get("created_at"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
        Ok(LearningSimulationSessionDto {
            id,
            program_id: row.get("program_id"),
            activity_id: row.get("activity_id"),
            activity_revision: row.get("activity_revision"),
            practice_session_id: row.get("practice_session_id"),
            learner_role: row.get("learner_role"),
            counterpart_role: row.get("counterpart_role"),
            status: simulation_status(row.get("status"))?,
            revision: row.get("revision"),
            turns,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            submitted_at: row.get("submitted_at"),
        })
    }

    pub async fn run(&self, run_id: &str) -> Result<LearningPracticalRunDto> {
        uuid(run_id, "run")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let connection = &mut *tx;
        let row = sqlx::query("SELECT * FROM learning_practical_runs WHERE id=?")
            .bind(run_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Practical run not found".into()))?;
        let result = Self::parse_run(connection, row).await?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }

    pub async fn simulation(&self, session_id: &str) -> Result<LearningSimulationSessionDto> {
        uuid(session_id, "simulation")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let connection = &mut *tx;
        let row = sqlx::query("SELECT * FROM learning_simulation_sessions WHERE id=?")
            .bind(session_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        let result = Self::parse_simulation(connection, row).await?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }
}
