//! Writers for every section of an imported pack, in the order the import
//! runs them. Each takes the importing transaction's connection and the
//! identifier map; nothing here decides whether a section is imported.
use super::{
    canvas_entries::CanvasPackSnapshot,
    db,
    entries::{ImportScope, PracticalPackSnapshot, PrivateAnswerKey, SECTION_ROWS},
    invalid, json_string, parse,
    remap::{remap, remap_json_field, remap_json_string_field, remap_refs},
    source_entries::SourceHistorySnapshot,
    validate_uuid,
    validation::validate_source_id,
    value_i64, value_option_str, value_str,
};
use crate::features::learning::references::trust::pack::SourceTrustPackEntry;
use crate::features::learning::{
    dto::{LearningPreparation, LearningProgramDto, LearningProgramStatus},
    pack::{
        sections::{
            source_body_path, EVIDENCE_ENTRY, FOLLOW_UP_ENTRY, LESSON_STATE_ENTRY, OUTCOMES_ENTRY,
            SOURCE_SELECTORS_ENTRY,
        },
        DecodedLearningPack,
    },
};
use lattice_core::shared::error::{AppError, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqliteConnection;
use std::collections::{HashMap, HashSet};

fn enum_label<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)
        .map_err(|e| AppError::Serialization(e.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Pack contains an invalid enum value."))
}

/// Write every section, in the order of `SECTION_ROWS`.
pub(super) async fn run(connection: &mut SqliteConnection, scope: &ImportScope<'_>) -> Result<()> {
    for rows in SECTION_ROWS {
        tracing::debug!(section = ?rows.section(), "importing pack section");
        rows.insert(connection, scope).await?;
    }
    Ok(())
}

pub(super) async fn insert_program(
    tx: &mut SqliteConnection,
    program: &LearningProgramDto,
    map: &HashMap<String, String>,
) -> Result<()> {
    let summary = &program.summary;
    let status = match summary.status {
        LearningProgramStatus::Draft => "draft",
        LearningProgramStatus::Active => "active",
    };
    sqlx::query("INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,current_lesson_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(&summary.id).bind(&summary.title).bind(&summary.goal).bind(status).bind(summary.revision).bind(&program.prior_knowledge).bind(program.minutes_per_session).bind(&program.model_name).bind(&summary.current_lesson_id).bind(summary.created_at).execute(&mut *tx).await.map_err(db)?;
    for (mi, module) in program.modules.iter().enumerate() {
        sqlx::query("INSERT INTO learning_modules(id,program_id,ordinal,title,summary,outcomes_json,prerequisite_ids_json,project_json) VALUES(?,?,?,?,?,?,?,?)").bind(&module.id).bind(&summary.id).bind(mi as i64).bind(&module.title).bind(&module.summary).bind(json_string(&module.outcomes)?).bind(json_string(&module.prerequisite_module_ids)?).bind(json_string(&module.project)?).execute(&mut *tx).await.map_err(db)?;
        for (li, lesson) in module.lessons.iter().enumerate() {
            sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation,completed) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(&lesson.id).bind(&summary.id).bind(&module.id).bind(li as i64).bind(&lesson.title).bind(&lesson.objective).bind(lesson.estimated_minutes).bind(if lesson.preparation==LearningPreparation::Ready{"ready"}else{"outline"}).bind(lesson.completed as i64).execute(&mut *tx).await.map_err(db)?;
            for (bi, block) in lesson.blocks.iter().enumerate() {
                let mut sources = block.source_ids.clone();
                remap_refs(map, &mut sources);
                sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)").bind(&lesson.id).bind(bi as i64).bind(crate::features::learning::repository::block_kind(block.kind.clone())).bind(&block.title).bind(&block.body).bind(json_string(&sources)?).bind(json_string(&block.rubric)?).execute(&mut *tx).await.map_err(db)?;
            }
            for (qi, question) in lesson.questions.iter().enumerate() {
                let mut sources = question.source_ids.clone();
                remap_refs(map, &mut sources);
                sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) VALUES(?,?,?,?,?,?,?,?)").bind(&question.id).bind(&lesson.id).bind(&module.id).bind(crate::features::learning::repository::assessment(question.kind.clone())).bind(&question.prompt).bind(json_string(&question.options)?).bind(json_string(&sources)?).bind(qi as i64).execute(&mut *tx).await.map_err(db)?;
            }
        }
    }
    for source in &program.sources {
        sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,?,?,?)").bind(&source.id).bind(&summary.id).bind(&source.title).bind(&source.url).bind(&source.excerpt).bind(source.acquired_at).execute(&mut *tx).await.map_err(db)?;
    }
    for attempt in &program.attempts {
        // Evidence is carried only when explicitly selected at export. IDs and
        // citations are remapped with the rest of the imported aggregate.
        let mut result_rows = attempt.results.clone();
        for result in &mut result_rows {
            result.question_id = remap(map, &result.question_id);
            remap_refs(map, &mut result.source_ids);
        }
        let answers: Vec<crate::features::learning::dto::LearningAnswerDto> = result_rows
            .iter()
            .map(|r| crate::features::learning::dto::LearningAnswerDto {
                question_id: r.question_id.clone(),
                selected_index: r.selected_index,
            })
            .collect();
        let correct = result_rows
            .iter()
            .filter(|r| r.selected_index == r.correct_index)
            .count() as i64;
        sqlx::query("INSERT INTO learning_attempts(id,program_id,module_id,lesson_id,kind,correct,total,answers_json,results_json,submitted_at,payload_hash,covered_lessons,module_lessons) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&attempt.id).bind(&summary.id).bind(&attempt.module_id).bind(&attempt.lesson_id).bind(crate::features::learning::repository::assessment(attempt.kind.clone())).bind(correct).bind(result_rows.len() as i64).bind(json_string(&answers)?).bind(json_string(&result_rows)?).bind(attempt.submitted_at).bind("imported").bind(attempt.coverage.map(|c| c.covered_lessons)).bind(attempt.coverage.map(|c| c.module_lessons)).execute(&mut *tx).await.map_err(db)?;
    }
    Ok(())
}

pub(super) async fn insert_lesson_states(
    tx: &mut SqliteConnection,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    if let Some(bytes) = pack.entries.get(LESSON_STATE_ENTRY) {
        let states: Vec<serde_json::Value> = parse(bytes)?;
        if states.len() > 2_000 {
            return Err(invalid("Pack has too many curriculum lesson states."));
        }
        for mut state in states {
            remap_json_field(&mut state, "lessonId", map)?;
            remap_json_field(&mut state, "replacementLessonId", map)?;
            let lesson = value_str(&state, "lessonId")?;
            let status = value_str(&state, "state")?;
            if !matches!(
                status,
                "outline" | "ready" | "completed" | "skipped" | "replaced" | "challenged"
            ) {
                return Err(invalid("Pack lesson curriculum state is invalid."));
            }
            sqlx::query("UPDATE learning_lessons SET curriculum_state=?,replacement_lesson_id=? WHERE program_id=? AND id=?")
                .bind(status).bind(state.get("replacementLessonId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(program_id).bind(lesson).execute(&mut *tx).await.map_err(db)?;
        }
    }
    Ok(())
}

pub(super) async fn insert_outcomes(
    tx: &mut SqliteConnection,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    let Some(bytes) = pack.entries.get(OUTCOMES_ENTRY) else {
        return Ok(());
    };
    let rows: Vec<serde_json::Value> = parse(bytes)?;
    if rows.len() > 2000 {
        return Err(invalid("Pack has too many learning outcomes."));
    }
    for row in rows {
        let old_id = value_str(&row, "id")?;
        let id = remap(map, old_id);
        let module = value_option_str(&row, "moduleId")?.map(|v| remap(map, &v));
        let lesson = value_option_str(&row, "lessonId")?.map(|v| remap(map, &v));
        let title = value_str(&row, "title")?;
        let description = value_str(&row, "description")?;
        let ordinal = value_i64(&row, "ordinal")?;
        let created = value_i64(&row, "createdAt")?;
        if title.trim().is_empty() || title.len() > 500 || description.len() > 5000 || ordinal < 0 {
            return Err(invalid("Pack outcome metadata is invalid."));
        }
        sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(id).bind(program_id).bind(module).bind(lesson).bind(title).bind(description).bind(ordinal).bind(created).execute(&mut *tx).await.map_err(db)?;
    }
    Ok(())
}

pub(super) async fn insert_canvas_snapshot(
    tx: &mut SqliteConnection,
    program_id: &str,
    snapshot: &mut CanvasPackSnapshot,
    map: &HashMap<String, String>,
) -> Result<()> {
    for row in &mut snapshot.canvases {
        remap_json_field(row, "id", map)?;
        remap_json_field(row, "lessonId", map)?;
        let scene = value_str(row, "sceneJson")?;
        let _: serde_json::Value = parse(scene.as_bytes())?;
        let count = value_i64(row, "elementCount")?;
        let revision = value_i64(row, "revision")?;
        if count < 0 || revision < 0 || scene.len() > 2_000_000 {
            return Err(invalid("Pack canvas data exceeds its bounds."));
        }
        sqlx::query("INSERT INTO learning_canvases(id,program_id,lesson_id,title,description,scene_json,element_count,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(value_str(row,"id")?).bind(program_id).bind(row.get("lessonId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(value_str(row,"title")?).bind(value_str(row,"description")?).bind(scene).bind(count).bind(revision).bind(value_i64(row,"createdAt")?).bind(value_i64(row,"updatedAt")?).execute(&mut *tx).await.map_err(db)?;
    }
    for row in &mut snapshot.snapshots {
        remap_json_field(row, "id", map)?;
        remap_json_field(row, "canvasId", map)?;
        let scene = value_str(row, "sceneJson")?;
        let _: serde_json::Value = parse(scene.as_bytes())?;
        sqlx::query("INSERT INTO learning_canvas_snapshots(id,program_id,canvas_id,name,title,description,scene_json,element_count,canvas_revision,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(value_str(row,"id")?).bind(program_id).bind(value_str(row,"canvasId")?).bind(value_str(row,"name")?).bind(value_str(row,"title")?).bind(value_str(row,"description")?).bind(scene).bind(value_i64(row,"elementCount")?).bind(value_i64(row,"canvasRevision")?).bind(value_i64(row,"createdAt")?).execute(&mut *tx).await.map_err(db)?;
    }
    Ok(())
}
pub(super) async fn insert_events(
    tx: &mut SqliteConnection,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    if let Some(bytes) = pack.entries.get(EVIDENCE_ENTRY) {
        let rows: Vec<serde_json::Value> = parse(bytes)?;
        if rows.len() > 20_000 {
            return Err(invalid("Pack has too many evidence records."));
        }
        for row in rows {
            let id = remap(map, value_str(&row, "id")?);
            let outcome = value_option_str(&row, "outcomeId")?.map(|v| remap(map, &v));
            let source_kind = value_str(&row, "sourceKind")?;
            let source_id = remap(map, value_str(&row, "sourceId")?);
            let dimension = value_str(&row, "dimension")?;
            let result = value_str(&row, "result")?;
            let score = row
                .get("score")
                .filter(|v| !v.is_null())
                .and_then(serde_json::Value::as_f64);
            let observation = value_str(&row, "observation")?;
            let quote = value_option_str(&row, "evidenceQuote")?;
            let assistance = value_str(&row, "assistance")?;
            let observed = value_i64(&row, "observedAt")?;
            if !matches!(
                source_kind,
                "practice" | "assessment" | "recall" | "practical" | "simulation" | "manual"
            ) || !matches!(
                dimension,
                "recall" | "explanation" | "application" | "transfer"
            ) || !matches!(
                result,
                "observed" | "not_observed" | "uncertain" | "not_assessed"
            ) || score.is_some_and(|v| !(0.0..=1.0).contains(&v))
            {
                return Err(invalid("Pack evidence record is invalid."));
            }
            sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id).bind(program_id).bind(outcome).bind(source_kind).bind(source_id).bind(dimension).bind(result).bind(score).bind(observation).bind(quote).bind(assistance).bind(observed).execute(&mut *tx).await.map_err(db)?;
        }
    }
    Ok(())
}

pub(super) async fn insert_follow_ups(
    tx: &mut SqliteConnection,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    if let Some(bytes) = pack.entries.get(FOLLOW_UP_ENTRY) {
        let rows: Vec<serde_json::Value> = parse(bytes)?;
        if rows.len() > 5000 {
            return Err(invalid("Pack has too many follow-up recommendations."));
        }
        for row in rows {
            let id = remap(map, value_str(&row, "id")?);
            let outcome = value_option_str(&row, "outcomeId")?.map(|v| remap(map, &v));
            let reason = value_str(&row, "reasonCode")?;
            let explanation = value_str(&row, "explanation")?;
            let kind = value_str(&row, "actionKind")?;
            let action = value_option_str(&row, "actionRef")?.map(|v| remap(map, &v));
            let status = value_str(&row, "status")?;
            let evidence_json = value_str(&row, "evidenceEventIds")?;
            let mut evidence: Vec<String> = parse(evidence_json.as_bytes())?;
            remap_refs(map, &mut evidence);
            let created = value_i64(&row, "createdAt")?;
            let decided = row
                .get("decidedAt")
                .filter(|v| !v.is_null())
                .and_then(serde_json::Value::as_i64);
            if !matches!(
                reason,
                "missed_outcome"
                    | "assisted_success"
                    | "low_transfer"
                    | "stale_evidence"
                    | "uncertain_grade"
            ) || !matches!(
                kind,
                "lesson" | "practice" | "assessment" | "recall" | "practical"
            ) || !matches!(status, "pending" | "accepted" | "dismissed" | "completed")
            {
                return Err(invalid("Pack follow-up record is invalid."));
            }
            sqlx::query("INSERT INTO learning_follow_up_recommendations(id,program_id,outcome_id,reason_code,explanation,action_kind,action_ref,status,evidence_event_ids_json,created_at,decided_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(id).bind(program_id).bind(outcome).bind(reason).bind(explanation).bind(kind).bind(action).bind(status).bind(json_string(&evidence)?).bind(created).bind(decided).execute(&mut *tx).await.map_err(db)?;
        }
    }
    Ok(())
}

pub(super) async fn insert_practical_snapshot(
    tx: &mut SqliteConnection,
    program_id: &str,
    snapshot: &PracticalPackSnapshot,
) -> Result<()> {
    use crate::features::learning::practical_dto::{
        LearningPracticalPublicFileRole, LearningPracticalRuntimeKind,
    };
    for activity in &snapshot.activities {
        validate_uuid(&activity.id, "practical activity")?;
        validate_uuid(&activity.lesson_id, "practical lesson")?;
        let profile_available = if let Some(profile) = activity.runtime_profile_id.as_deref() {
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM learning_runtime_profiles WHERE id=?")
                .bind(profile)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .is_some()
        } else {
            false
        };
        let runtime = if profile_available
            && activity.runtime_kind == LearningPracticalRuntimeKind::Container
        {
            activity.runtime_kind
        } else {
            LearningPracticalRuntimeKind::None
        };
        let kind = enum_label(&activity.kind)?;
        let status = enum_label(&activity.status)?;
        let mode = enum_label(&activity.practice_mode)?;
        let runtime_name = enum_label(&runtime)?;
        let runtime_profile = if profile_available {
            activity.runtime_profile_id.as_deref()
        } else {
            None
        };
        let engine = if profile_available {
            activity
                .runtime_engine
                .as_ref()
                .map(enum_label)
                .transpose()?
        } else {
            None
        };
        let image = if profile_available {
            activity.runtime_image_id.as_deref()
        } else {
            None
        };
        let command = if profile_available {
            activity
                .runtime_command
                .as_ref()
                .map(json_string)
                .transpose()?
        } else {
            None
        };
        let limits = if profile_available {
            activity
                .runtime_limits
                .as_ref()
                .map(json_string)
                .transpose()?
        } else {
            None
        };
        sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&activity.id).bind(program_id).bind(&activity.lesson_id).bind(Option::<&str>::None).bind(kind).bind(&activity.title).bind(&activity.brief).bind(status).bind(mode).bind(json_string(&activity.allowed_aids)?).bind(json_string(&activity.outcome_ids)?).bind(json_string(&activity.source_version_ids)?).bind(json_string(&activity.rubric)?).bind(runtime_name).bind(runtime_profile).bind(engine).bind(image).bind(command).bind(limits).bind(&activity.generator_model).bind(activity.revision).bind(activity.created_at).bind(activity.updated_at).execute(&mut *tx).await.map_err(db)?;
        for (ordinal, file) in activity.files.iter().enumerate() {
            if !matches!(
                file.role,
                LearningPracticalPublicFileRole::Starter
                    | LearningPracticalPublicFileRole::Reference
            ) {
                return Err(invalid(
                    "A pack cannot import hidden practical evaluator files.",
                ));
            }
            if format!("{:x}", Sha256::digest(file.content.as_bytes())) != file.content_sha256 {
                return Err(invalid("Practical activity file checksum does not match."));
            }
            let role = enum_label(&file.role)?;
            sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,?,?,?,?,?,?)").bind(&activity.id).bind(ordinal as i64).bind(&file.path).bind(role).bind(&file.content).bind(&file.content_sha256).bind(file.editable as i64).execute(&mut *tx).await.map_err(db)?;
        }
    }
    for activity in &snapshot.activities {
        if let Some(predecessor) = &activity.predecessor_id {
            sqlx::query("UPDATE learning_practical_activities SET predecessor_id=? WHERE program_id=? AND id=?")
                .bind(predecessor).bind(program_id).bind(&activity.id).execute(&mut *tx).await.map_err(db)?;
        }
    }
    for run in &snapshot.runs {
        validate_uuid(&run.id, "practical run")?;
        let _activity = snapshot
            .activities
            .iter()
            .find(|a| a.id == run.activity_id)
            .ok_or_else(|| invalid("Practical run references a missing activity."))?;
        let status = enum_label(&run.status)?;
        let engine = run.engine.as_ref().map(enum_label).transpose()?;
        let checks = json_string(&run.checks)?;
        let raw_snapshot = snapshot
            .run_snapshots
            .iter()
            .find(|raw| {
                raw.get("runId").and_then(serde_json::Value::as_str) == Some(run.id.as_str())
            })
            .ok_or_else(|| invalid("Pack omits a practical run's immutable activity snapshot."))?;
        let snapshot_json = value_str(raw_snapshot, "activitySnapshot")?.to_owned();
        let _: serde_json::Value = parse(snapshot_json.as_bytes())?;
        let files = json_string(&run.learner_files)?;
        let operation_id = value_str(raw_snapshot, "operationId")?;
        let payload_hash = value_str(raw_snapshot, "payloadHash")?;
        sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,engine,image_id,status,learner_files_json,stdout,stderr,output_truncated,exit_code,duration_ms,check_results_json,created_at,completed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&run.id).bind(program_id).bind(&run.activity_id).bind(run.activity_revision).bind(snapshot_json).bind(&run.practice_session_id).bind(operation_id).bind(payload_hash).bind(engine).bind(&run.image_id).bind(status).bind(files).bind(&run.stdout).bind(&run.stderr).bind(run.output_truncated as i64).bind(run.exit_code).bind(run.duration_ms).bind(checks).bind(run.created_at).bind(run.completed_at).execute(&mut *tx).await.map_err(db)?;
        for (ordinal, check) in run.checks.iter().enumerate() {
            let status = enum_label(&check.status)?;
            sqlx::query("INSERT INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,?,?,?,?,?)").bind(&run.id).bind(ordinal as i64).bind(&check.name).bind(status).bind(&check.message).bind(check.duration_ms).execute(&mut *tx).await.map_err(db)?;
        }
    }
    for session in &snapshot.simulations {
        let id = value_str(session, "id")?;
        let activity = value_str(session, "activityId")?;
        let revision = value_i64(session, "activityRevision")?;
        let snapshot_json = value_str(session, "activitySnapshot")?;
        let operation = value_str(session, "operationId")?;
        let payload = value_str(session, "payloadHash")?;
        let status = value_str(session, "status")?;
        if !matches!(status, "active" | "submitted" | "cancelled") {
            return Err(invalid("Pack simulation status is invalid."));
        }
        sqlx::query("INSERT INTO learning_simulation_sessions(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id).bind(program_id).bind(activity).bind(revision).bind(snapshot_json).bind(session.get("practiceSessionId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(operation).bind(payload).bind(value_str(session,"learnerRole")?).bind(value_str(session,"counterpartRole")?).bind(status).bind(value_i64(session,"revision")?).bind(value_i64(session,"createdAt")?).bind(value_i64(session,"updatedAt")?).bind(session.get("submittedAt").filter(|v|!v.is_null()).and_then(serde_json::Value::as_i64)).execute(&mut *tx).await.map_err(db)?;
    }
    for turn in &snapshot.simulation_turns {
        let speaker = value_str(turn, "speaker")?;
        if !matches!(speaker, "learner" | "counterpart" | "coach") {
            return Err(invalid("Pack simulation turn speaker is invalid."));
        }
        let citations = value_str(turn, "citations")?;
        let _: serde_json::Value = parse(citations.as_bytes())?;
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(value_str(turn,"id")?).bind(program_id).bind(value_str(turn,"sessionId")?).bind(value_str(turn,"operationId")?).bind(value_str(turn,"payloadHash")?).bind(value_i64(turn,"ordinal")?).bind(speaker).bind(value_str(turn,"content")?).bind(citations).bind(turn.get("modelName").and_then(serde_json::Value::as_str)).bind(value_i64(turn,"createdAt")?).execute(&mut *tx).await.map_err(db)?;
    }
    Ok(())
}

pub(super) async fn insert_source_workspace(
    tx: &mut SqliteConnection,
    program_id: &str,
    workspace: &crate::features::learning::dto::LearningSourceWorkspaceDto,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    for source in &workspace.sources {
        let source_id = remap(map, &source.id);
        let kind = match source.kind {
            crate::features::learning::dto::LearningSourceKind::Web => "web",
            crate::features::learning::dto::LearningSourceKind::Document => "document",
            crate::features::learning::dto::LearningSourceKind::Pasted => "pasted",
        };
        let policy = match source.freshness_policy {
            crate::features::learning::dto::LearningSourcePolicy::Fixed => "fixed",
            crate::features::learning::dto::LearningSourcePolicy::Manual => "manual",
            crate::features::learning::dto::LearningSourcePolicy::BeforeUse => "before_use",
        };
        // Who added a source is the pack author's record and travels with it
        // (claim review §5.2, §5.4).
        let added_by =
            lattice_core::domain::learning::claim_review::SourceAddedBy::from(source.added_by)
                .as_str();
        sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,deleted_at,deletion_reason,created_at,updated_at,added_by) VALUES(?,?,?,?,?,?,NULL,NULL,?,?,?,?,?,?)")
            .bind(&source_id).bind(program_id).bind(kind).bind(&source.origin).bind(&source.requested_url).bind(policy).bind(source.revision).bind(source.deleted_at).bind(&source.deletion_reason).bind(source.created_at).bind(source.updated_at).bind(added_by).execute(&mut *tx).await.map_err(db)?;
        let mut restored_digests = HashSet::new();
        for version in &source.versions {
            validate_source_id(&version.id)?;
            let version_id = remap(map, &version.id);
            let body = pack.entries.get(&source_body_path(&version.id));
            let full_text = match body {
                Some(bytes) => std::str::from_utf8(bytes)
                    .map_err(|e| AppError::InvalidData(e.to_string()))?
                    .to_owned(),
                None => version.excerpt.clone(),
            };
            if full_text.len() > 1_000_000 {
                return Err(invalid(
                    "A source version in the pack exceeds the restore size limit.",
                ));
            }
            let content_digest = format!("{:x}", Sha256::digest(full_text.as_bytes()));
            if body.is_none() && !restored_digests.insert(content_digest.clone()) {
                return Err(invalid("Two source versions have the same available excerpt. Include licensed full source bodies to preserve both immutable versions."));
            }
            if body.is_some() && version.content_sha256 != content_digest {
                return Err(invalid(
                    "A source body does not match its declared content digest.",
                ));
            }
            let restored_word_count = full_text.split_whitespace().count() as i64;
            let extraction = if body.is_some() {
                version.extraction_version.clone()
            } else {
                "pack_excerpt".into()
            };
            let truncated = body.is_none() || version.truncated;
            // A frozen page is a file on the computer that captured it, not in
            // the pack: keep its hash only where this computer has the page,
            // so elsewhere the source is captured again rather than shown as
            // frozen with nothing behind it.
            sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at,snapshot_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,(SELECT sha256 FROM web_page_snapshots WHERE status='ready' AND sha256=? LIMIT 1))")
                .bind(&version_id).bind(program_id).bind(&source_id).bind(version.version_number).bind(&version.title).bind(&version.publisher).bind(&source.requested_url).bind(&version.resolved_url).bind(&full_text).bind(&version.excerpt).bind(if body.is_some(){version.content_sha256.clone()}else{content_digest}).bind(restored_word_count).bind(truncated as i64).bind(extraction).bind(version.acquired_at).bind(&version.snapshot_hash).execute(&mut *tx).await.map_err(db)?;
        }
        let active = source.active_version_id.as_deref().map(|id| remap(map, id));
        let pending = source
            .pending_version_id
            .as_deref()
            .map(|id| remap(map, id));
        sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=? WHERE id=? AND program_id=?")
            .bind(active).bind(pending).bind(&source_id).bind(program_id).execute(&mut *tx).await.map_err(db)?;
    }
    Ok(())
}

/// A pack's trust rows, after the source versions they hang from.
pub(super) async fn insert_source_trust(
    tx: &mut SqliteConnection,
    program_id: &str,
    rows: SourceTrustPackEntry,
    map: &HashMap<String, String>,
) -> Result<()> {
    rows.check()?;
    rows.remapped(program_id, |id| remap(map, id))
        .write(tx, program_id)
        .await
}

pub(super) async fn insert_source_history(
    tx: &mut SqliteConnection,
    program_id: &str,
    history: &mut SourceHistorySnapshot,
    map: &HashMap<String, String>,
) -> Result<()> {
    for check in &mut history.checks {
        remap_json_field(check, "operationId", map)?;
        remap_json_field(check, "sourceId", map)?;
        remap_json_field(check, "pendingVersionId", map)?;
        let status = value_str(check, "status")?;
        if !matches!(status, "unchanged" | "update_available" | "failed") {
            return Err(invalid("Pack source check status is invalid."));
        }
        sqlx::query("INSERT INTO learning_source_refresh_checks(operation_id,program_id,source_id,status,checked_at,active_digest,pending_version_id,message) VALUES(?,?,?,?,?,?,?,?)")
            .bind(value_str(check,"operationId")?).bind(program_id).bind(value_str(check,"sourceId")?).bind(status).bind(value_i64(check,"checkedAt")?).bind(check.get("activeDigest").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(check.get("pendingVersionId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(check.get("message").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).execute(&mut *tx).await.map_err(db)?;
    }
    Ok(())
}

/// Point a practical snapshot at the imported course. Without learner
/// evidence the practice sessions it cites are not imported, so the links go.
pub(super) fn remap_practical(
    snapshot: &mut PracticalPackSnapshot,
    program_id: &str,
    map: &HashMap<String, String>,
    with_evidence: bool,
    remaps_ids: bool,
) -> Result<()> {
    for activity in &mut snapshot.activities {
        activity.id = remap(map, &activity.id);
        activity.program_id = program_id.to_owned();
        activity.lesson_id = remap(map, &activity.lesson_id);
        activity.predecessor_id = activity.predecessor_id.as_deref().map(|id| remap(map, id));
        remap_refs(map, &mut activity.outcome_ids);
        remap_refs(map, &mut activity.source_version_ids);
    }
    for run in &mut snapshot.runs {
        run.id = remap(map, &run.id);
        run.program_id = program_id.to_owned();
        run.activity_id = remap(map, &run.activity_id);
        run.practice_session_id = if with_evidence {
            run.practice_session_id.as_deref().map(|id| remap(map, id))
        } else {
            None
        };
    }
    for raw in &mut snapshot.run_snapshots {
        remap_json_field(raw, "runId", map)?;
        remap_json_field(raw, "operationId", map)?;
        if remaps_ids {
            remap_json_string_field(raw, "activitySnapshot", map)?;
        }
    }
    for session in &mut snapshot.simulations {
        remap_json_field(session, "id", map)?;
        remap_json_field(session, "activityId", map)?;
        remap_json_field(session, "practiceSessionId", map)?;
        remap_json_field(session, "operationId", map)?;
        if remaps_ids {
            remap_json_string_field(session, "activitySnapshot", map)?;
        }
        if !with_evidence {
            session["practiceSessionId"] = serde_json::Value::Null;
        }
    }
    for turn in &mut snapshot.simulation_turns {
        remap_json_field(turn, "id", map)?;
        remap_json_field(turn, "sessionId", map)?;
        remap_json_field(turn, "operationId", map)?;
        if remaps_ids {
            remap_json_string_field(turn, "citations", map)?;
        }
    }
    Ok(())
}

pub(super) async fn insert_selectors(
    tx: &mut SqliteConnection,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
    remaps_ids: bool,
) -> Result<()> {
    if let Some(bytes) = pack.entries.get(SOURCE_SELECTORS_ENTRY) {
        let selectors: Vec<serde_json::Value> = parse(bytes)?;
        for selector in selectors {
            let old_source = selector
                .get("sourceId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid("Pack selector has no source ID."))?;
            let old_version = selector
                .get("sourceVersionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid("Pack selector has no version ID."))?;
            let source_id = remap(map, old_source);
            let version_id = remap(map, old_version);
            let body_included = pack.entries.contains_key(&source_body_path(old_version));
            let old_status = selector
                .get("status")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("Pack selector has an invalid status."))?;
            if !matches!(
                old_status,
                "exact" | "contextual" | "ambiguous" | "not_found"
            ) {
                return Err(invalid("Pack selector has an invalid status."));
            }
            let status = if body_included {
                old_status
            } else {
                "not_found"
            };
            let exact = selector
                .get("exactText")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("Pack selector has no exact text."))?;
            let prefix = selector
                .get("prefixText")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("Pack selector has no prefix text."))?;
            let suffix = selector
                .get("suffixText")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("Pack selector has no suffix text."))?;
            let start = if body_included {
                selector
                    .get("startByte")
                    .and_then(serde_json::Value::as_i64)
            } else {
                None
            };
            let end = if body_included {
                selector.get("endByte").and_then(serde_json::Value::as_i64)
            } else {
                None
            };
            let candidate_count = if body_included {
                selector
                    .get("candidateCount")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| invalid("Pack selector has an invalid candidate count."))?
            } else {
                0
            };
            let created_at = selector
                .get("createdAt")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| invalid("Pack selector has an invalid timestamp."))?;
            if candidate_count < 0
                || (matches!(status, "exact" | "contextual") && candidate_count < 1)
                || (status == "not_found" && candidate_count != 0)
                || start.is_some_and(|v| v < 0)
                || end.is_some_and(|v| v < 0)
                || exact.chars().count() > 2000
                || prefix.chars().count() > 500
                || suffix.chars().count() > 500
                || matches!(status, "exact" | "contextual")
                    && start.zip(end).is_none_or(|(s, e)| e <= s)
            {
                return Err(invalid("Pack selector offsets are invalid."));
            }
            let selector_id = if remaps_ids {
                uuid::Uuid::new_v4().to_string()
            } else {
                selector
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Pack selector has no ID."))?
                    .to_owned()
            };
            sqlx::query("INSERT INTO learning_source_selectors(id,program_id,source_id,source_version_id,exact_text,prefix_text,suffix_text,start_byte,end_byte,candidate_count,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(selector_id).bind(program_id).bind(source_id).bind(version_id).bind(exact).bind(prefix).bind(suffix).bind(start).bind(end).bind(candidate_count).bind(status).bind(created_at).execute(&mut *tx).await.map_err(db)?;
        }
    }
    Ok(())
}

pub(super) async fn insert_answer_keys(
    tx: &mut SqliteConnection,
    keys: Vec<PrivateAnswerKey>,
    map: &HashMap<String, String>,
) -> Result<()> {
    for key in keys {
        let qid = map
            .get(&key.question_id)
            .cloned()
            .unwrap_or(key.question_id);
        sqlx::query(
            "INSERT INTO learning_answer_keys(question_id,correct_index,explanation) VALUES(?,?,?)",
        )
        .bind(qid)
        .bind(key.correct_index as i64)
        .bind(key.explanation)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    }
    Ok(())
}
