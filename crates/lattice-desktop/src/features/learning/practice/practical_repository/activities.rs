//! Runtime setup and profiles, and saving generated activities with their internal snapshots.
use super::{
    decode, engine_name, invalid, json, kind_name, mode_name, parse_engine, parse_kind, parse_mode,
    practical_operation, uuid, InternalActivityFile, InternalActivitySnapshot,
    LearningPracticalRepository, PracticalWorkspaceSnapshot,
};
use crate::features::learning::embedded_runtime::{
    BuiltinLabExecutionSpec, LearningBuiltinRuntime,
};
use crate::features::learning::lab_runtime::{
    self, LearningLabExecutionSpec, LearningLabFile, LearningLabLimits,
};
use crate::features::learning::persistence::{db, hash_text, now};
use crate::features::learning::practical_dto::*;
use crate::features::learning::practical_generation::{
    GeneratedPracticalActivity, PracticalFileRole,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashSet;

impl LearningPracticalRepository {
    /// Validate setup before contacting an engine or downloading an image, and
    /// recover a committed setup when the renderer lost its response.
    pub async fn preflight_runtime_setup(
        &self,
        request: &PrepareLearningRuntimePresetRequestDto,
        payload_hash: &str,
    ) -> Result<Option<PracticalWorkspaceSnapshot>> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.profile_id, "runtime profile")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        Self::active_program(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "runtime_profile",
            &request.operation_id,
            None,
            &request.profile_id,
            "save",
            payload_hash,
            "Operation ID was reused with different runtime setup data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&request.program_id).await.map(Some);
        }
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_runtime_profiles WHERE id=?")
                .bind(&request.profile_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if exists.is_some() {
            return Err(invalid(
                "This runtime profile already exists; refresh the available environments.",
            ));
        }
        tx.commit().await.map_err(db)?;
        Ok(None)
    }

    pub async fn save_runtime_profile(
        &self,
        request: &SaveLearningRuntimeProfileRequestDto,
        payload_hash: &str,
    ) -> Result<PracticalWorkspaceSnapshot> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.profile_id, "runtime profile")?;
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(invalid(
                "Runtime profile names must contain 1–120 characters.",
            ));
        }
        // Reuse the execution boundary to validate immutable image IDs, trusted
        // argv, and bounded limits. The placeholder file is never executed.
        lab_runtime::validate_execution_spec(&LearningLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            image_id: request.image_id.clone(),
            command: request.command.clone(),
            files: vec![LearningLabFile {
                path: "profile-validation.txt".into(),
                content: "validation".into(),
            }],
            read_only_paths: vec![],
            limits: request.limits.clone(),
        })?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        Self::active_program(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "runtime_profile",
            &request.operation_id,
            None,
            &request.profile_id,
            "save",
            payload_hash,
            "Operation ID was reused with different runtime profile data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&request.program_id).await;
        }
        let timestamp = now();
        let result_revision = if let Some(expected) = request.expected_revision {
            let updated = sqlx::query("UPDATE learning_runtime_profiles SET name=?,engine=?,image_id=?,command_json=?,limits_json=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?")
                .bind(name).bind(engine_name(request.engine)).bind(&request.image_id)
                .bind(json(&request.command)?).bind(json(&request.limits)?).bind(timestamp)
                .bind(&request.profile_id).bind(expected).execute(&mut *tx).await.map_err(db)?;
            if updated.rows_affected() != 1 {
                return Err(invalid("Runtime profile changed; reload and retry."));
            }
            expected + 1
        } else {
            sqlx::query("INSERT INTO learning_runtime_profiles(id,name,engine,image_id,command_json,limits_json,enabled,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,1,0,?,?)")
                .bind(&request.profile_id).bind(name).bind(engine_name(request.engine))
                .bind(&request.image_id).bind(json(&request.command)?).bind(json(&request.limits)?)
                .bind(timestamp).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
            0
        };
        operation.record(&mut *tx, &result_revision).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&request.program_id).await
    }

    pub async fn save_generated_activity(
        &self,
        request: &GenerateLearningPracticalActivityRequestDto,
        generated: &GeneratedPracticalActivity,
        generator_model: &str,
        payload_hash: &str,
    ) -> Result<PracticalWorkspaceSnapshot> {
        if request.builtin_runtime.is_some() && request.runtime_profile_id.is_some() {
            return Err(invalid("Choose one execution environment for an activity."));
        }
        if let Some(runtime) = request.builtin_runtime {
            let spec = BuiltinLabExecutionSpec {
                run_id: request.activity_id.clone(),
                runtime,
                files: generated
                    .files
                    .iter()
                    .map(|file| LearningLabFile {
                        path: file.path.clone(),
                        content: file.content.clone(),
                    })
                    .collect(),
                read_only_paths: generated
                    .files
                    .iter()
                    .filter(|file| {
                        file.role == PracticalFileRole::Check
                            || file.role == PracticalFileRole::Reference
                    })
                    .map(|file| file.path.clone())
                    .collect(),
                limits: LearningLabLimits::default(),
            };
            spec.validate()?;
            if !generated.files.iter().any(|file| {
                file.path == runtime.entrypoint() && file.role == PracticalFileRole::Check
            }) {
                return Err(invalid("Built-in activities require an independent evaluator at the runtime entrypoint."));
            }
        }
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.activity_id, "activity"),
            (&request.program_id, "program"),
            (&request.lesson_id, "lesson"),
        ] {
            uuid(value, label)?;
        }
        if request.allowed_aids.len() > 24
            || request
                .allowed_aids
                .iter()
                .any(|aid| aid.trim().is_empty() || aid.chars().count() > 160)
        {
            return Err(invalid("Practical aid descriptions must be bounded."));
        }
        if generator_model.trim().is_empty() || generator_model.chars().count() > 240 {
            return Err(invalid(
                "Practical generation must record a model identity.",
            ));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "practical_activity",
            &request.operation_id,
            Some(&request.program_id),
            &request.activity_id,
            "generate",
            payload_hash,
            "Operation ID was reused with different practical activity data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&request.program_id).await;
        }
        let revision = Self::active_program(&mut tx, &request.program_id).await?;
        if revision != request.expected_program_revision {
            return Err(invalid(
                "The learning program changed; regenerate this activity.",
            ));
        }
        let module_id: Option<String> = sqlx::query_scalar(
            "SELECT module_id FROM learning_lessons WHERE id=? AND program_id=?",
        )
        .bind(&request.lesson_id)
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let module_id =
            module_id.ok_or_else(|| AppError::NotFound("Learning lesson not found".into()))?;
        let outcome_ids = sqlx::query_scalar::<_, String>("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND (lesson_id=? OR (lesson_id IS NULL AND module_id=?)) ORDER BY ordinal,id")
            .bind(&request.program_id).bind(&request.lesson_id).bind(&module_id)
            .fetch_all(&mut *tx).await.map_err(db)?;
        for source_id in &generated.source_version_ids {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&request.program_id)
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid(
                    "Practical activity source version is outside this program.",
                ));
            }
        }
        let (runtime_kind, profile_id, runtime_engine, image_id, command, limits) =
            if let Some(profile_id) = request.runtime_profile_id.as_deref() {
                uuid(profile_id, "runtime profile")?;
                let profile =
                    sqlx::query("SELECT * FROM learning_runtime_profiles WHERE id=? AND enabled=1")
                        .bind(profile_id)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(db)?
                        .ok_or_else(|| invalid("Selected runtime profile is unavailable."))?;
                (
                    "container",
                    Some(profile_id.to_owned()),
                    Some(profile.get::<String, _>("engine")),
                    Some(profile.get::<String, _>("image_id")),
                    Some(profile.get::<String, _>("command_json")),
                    Some(profile.get::<String, _>("limits_json")),
                )
            } else {
                ("none", None, None, None, None, None)
            };
        if (runtime_kind == "container" || request.builtin_runtime.is_some())
            && generated
                .files
                .iter()
                .all(|file| file.role != PracticalFileRole::Check)
        {
            return Err(invalid(
                "Executable practical activities require independent evaluator files.",
            ));
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,NULL,?,?,?,'ready',?,?,?,?,?,?,?,?,?,?,?,?,0,?,?)")
            .bind(&request.activity_id).bind(&request.program_id).bind(&request.lesson_id)
            .bind(kind_name(request.kind)).bind(generated.title.trim()).bind(generated.brief.trim())
            .bind(mode_name(&request.practice_mode)).bind(json(&request.allowed_aids)?)
            .bind(json(&outcome_ids)?).bind(json(&generated.source_version_ids)?)
            .bind(json(&generated.rubric)?).bind(runtime_kind).bind(profile_id)
            .bind(runtime_engine).bind(image_id).bind(command).bind(limits).bind(generator_model)
            .bind(timestamp).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        if let Some(runtime) = request.builtin_runtime {
            sqlx::query("UPDATE learning_practical_activities SET builtin_runtime=?,builtin_runtime_version=? WHERE id=?")
                .bind(runtime.as_str()).bind(runtime.version()).bind(&request.activity_id).execute(&mut *tx).await.map_err(db)?;
        }
        let mut paths = HashSet::new();
        for (ordinal, file) in generated.files.iter().enumerate() {
            if !paths.insert(file.path.as_str()) {
                return Err(invalid("Practical activity contains duplicate file paths."));
            }
            let role = match file.role {
                PracticalFileRole::Starter => "starter",
                PracticalFileRole::Check => "check",
                PracticalFileRole::Reference => "reference",
            };
            sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,?,?,?,?,?,?)")
                .bind(&request.activity_id).bind(ordinal as i64).bind(&file.path).bind(role)
                .bind(&file.content).bind(hash_text(&file.content))
                .bind(file.editable as i64).execute(&mut *tx).await.map_err(db)?;
        }
        operation.record(&mut *tx, &0_i64).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&request.program_id).await
    }

    pub(super) async fn internal_activity(
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        activity_id: &str,
    ) -> Result<InternalActivitySnapshot> {
        let row = sqlx::query("SELECT a.*,p.enabled AS runtime_enabled FROM learning_practical_activities a LEFT JOIN learning_runtime_profiles p ON p.id=a.runtime_profile_id WHERE a.program_id=? AND a.id=?")
            .bind(program_id).bind(activity_id).fetch_optional(&mut **tx).await.map_err(db)?
            .ok_or_else(||AppError::NotFound("Practical activity not found".into()))?;
        if row.get::<String, _>("status") != "ready" {
            return Err(invalid("Only ready practical activities can be used."));
        }
        let file_rows = sqlx::query(
            "SELECT * FROM learning_practical_files WHERE activity_id=? ORDER BY ordinal",
        )
        .bind(activity_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(db)?;
        let files = file_rows
            .into_iter()
            .map(|file| InternalActivityFile {
                path: file.get("path"),
                role: file.get("role"),
                content: file.get("content"),
                content_sha256: file.get("content_sha256"),
                editable: file.get::<i64, _>("editable") != 0,
            })
            .collect();
        Ok(InternalActivitySnapshot {
            id: row.get("id"),
            program_id: row.get("program_id"),
            lesson_id: row.get("lesson_id"),
            kind: parse_kind(row.get("kind"))?,
            title: row.get("title"),
            brief: row.get("brief"),
            practice_mode: parse_mode(row.get("practice_mode"))?,
            allowed_aids: decode(row.get("allowed_aids_json"))?,
            outcome_ids: decode(row.get("outcome_ids_json"))?,
            source_version_ids: decode(row.get("source_version_ids_json"))?,
            rubric: decode(row.get("rubric_json"))?,
            runtime_profile_id: row.get("runtime_profile_id"),
            builtin_runtime: row
                .get::<Option<String>, _>("builtin_runtime")
                .as_deref()
                .map(LearningBuiltinRuntime::parse)
                .transpose()?,
            builtin_runtime_version: row.get("builtin_runtime_version"),
            workspace_owner_token: None,
            runtime_enabled: row.get::<Option<i64>, _>("runtime_enabled").unwrap_or(0) != 0,
            runtime_engine: row
                .get::<Option<String>, _>("runtime_engine")
                .as_deref()
                .map(parse_engine)
                .transpose()?,
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
            files,
            revision: row.get("revision"),
            generator_model: row.get("generator_model"),
        })
    }
}
