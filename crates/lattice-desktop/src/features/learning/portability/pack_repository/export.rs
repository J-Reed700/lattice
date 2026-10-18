//! Read all portable program data from one SQLite snapshot. No file I/O or
//! runtime services. Each section's row decides whether it is exported; the
//! encoders below read the sections whose rows live in `entries`.
use super::{
    db,
    entries::{ExportScope, PracticalPackSnapshot, PrivateAnswerKey, SECTION_ROWS},
    guards, invalid, json, now,
    source_entries::SourceHistorySnapshot,
};
use crate::features::learning::{
    pack::{
        sections::{
            source_body_path, EVIDENCE_ENTRY, FOLLOW_UP_ENTRY, KEYS_ENTRY, LESSON_STATE_ENTRY,
            OUTCOMES_ENTRY, PRACTICAL_ENTRY, SOURCE_HISTORY_ENTRY, SOURCE_LIBRARY_ENTRY,
            SOURCE_SELECTORS_ENTRY,
        },
        LearningPackEntry, LearningPackEntryKind, LearningPackInput, LearningPackPrivacyManifest,
    },
    portability_dto::LearningPackExportOptionsDto,
    practical_repository::LearningPracticalRepository,
    repository::LearningRepository,
    source_library::LearningSourceLibraryRepository,
};
use lattice_core::shared::error::{AppError, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

/// A user-requested export: refused while the course is still being
/// generated or before it has an outline another computer could import.
pub(in crate::features::learning) async fn snapshot(
    pool: &SqlitePool,
    program_id: &str,
    options: &LearningPackExportOptionsDto,
) -> Result<LearningPackInput> {
    let mut tx = pool.begin().await.map_err(db)?;
    guards::require_exportable(&mut tx, program_id).await?;
    let input = snapshot_on(&mut tx, program_id, options).await?;
    tx.commit().await.map_err(db)?;
    Ok(input)
}

/// The caller owns the transaction, including any write lock needed to keep a
/// replacement backup and its subsequent deletion on the same database state.
pub(super) async fn snapshot_on(
    tx: &mut SqliteConnection,
    program_id: &str,
    options: &LearningPackExportOptionsDto,
) -> Result<LearningPackInput> {
    if options.include_source_bodies && !options.source_body_redistribution_confirmed {
        return Err(invalid(
            "Confirm redistribution rights before exporting full source bodies.",
        ));
    }
    let program = LearningRepository::get_on(tx, program_id).await?;
    let scope = ExportScope {
        program: &program,
        options,
    };
    let mut entries = Vec::new();
    for rows in SECTION_ROWS {
        entries.extend(rows.encode(tx, &scope).await?);
    }
    Ok(LearningPackInput {
        schema_version: guards::schema_version(tx).await?,
        pack_id: uuid::Uuid::new_v4().to_string(),
        title: program.summary.title.clone(),
        created_at: now(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        privacy: privacy(options),
        entries,
    })
}

fn privacy(options: &LearningPackExportOptionsDto) -> LearningPackPrivacyManifest {
    LearningPackPrivacyManifest {
            includes_private_chat: false,
            includes_credentials: false,
            includes_full_source_bodies: options.include_source_bodies,
            source_body_redistribution_confirmed: options.source_body_redistribution_confirmed,
            includes_answer_keys: true,
            includes_hidden_evaluators: false,
            includes_learner_evidence: options.include_evidence || options.include_practical_artifacts,
            includes_practical_artifacts: options.include_practical_artifacts,
            omitted_items: vec![
                "Lattice chat outside this program and credentials are never exported.".into(),
                "Saved outline citations and lesson verification reports are included as historical course provenance, independently of learner activity. Import does not reverify them.".into(),
                if !options.include_evidence {
                    "Submitted assessments, learner evidence, recall/review history, workbench history, memory, and canvas data were omitted by request.".into()
                } else {
                    "Assessment and curriculum history, practice workbench records (including tutor turns), memory/deck and recall history, and canvas documents are included because learner evidence was selected.".into()
                },
                if !options.include_practical_artifacts {
                    "Practical activities and learner artifacts were omitted by request.".into()
                } else {
                    "Hidden practical evaluator files are omitted; imported activities are review-only because runtime profiles and checks are not portable.".into()
                },
                "External notebook links and their note contents are omitted; copied memory records do not retain links to local journals.".into(),
                if !options.include_source_bodies {
                    "Full source bodies were omitted by request; only bounded excerpts and immutable version metadata are included.".into()
                } else {
                    "Full source bodies are included after redistribution-rights confirmation.".into()
                },
            ],
    }
}

/// One entry holding `value` as JSON.
pub(super) fn entry<T: Serialize>(
    path: &str,
    kind: LearningPackEntryKind,
    value: &T,
) -> Result<LearningPackEntry> {
    Ok(LearningPackEntry {
        path: path.into(),
        kind,
        bytes: json(value)?,
    })
}

/// Where each lesson stands in the curriculum.
pub(super) async fn lesson_states(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let state_rows=sqlx::query("SELECT id,curriculum_state,replacement_lesson_id FROM learning_lessons WHERE program_id=? ORDER BY id")
            .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let lesson_states:Vec<serde_json::Value>=state_rows.into_iter().map(|r|serde_json::json!({"lessonId":r.get::<String,_>("id"),"state":r.get::<String,_>("curriculum_state"),"replacementLessonId":r.get::<Option<String>,_>("replacement_lesson_id")})).collect();
    Ok(vec![entry(
        LESSON_STATE_ENTRY,
        LearningPackEntryKind::Curriculum,
        &lesson_states,
    )?])
}

/// The private answer keys, which only an import may read.
pub(super) async fn answer_keys(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let keys=sqlx::query("SELECT q.id question_id,k.correct_index,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id JOIN learning_lessons l ON l.id=q.lesson_id WHERE l.program_id=? ORDER BY q.id")
            .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let private_keys = keys
        .into_iter()
        .map(|r| PrivateAnswerKey {
            question_id: r.get("question_id"),
            correct_index: r.get::<i64, _>("correct_index") as usize,
            explanation: r.get("explanation"),
        })
        .collect::<Vec<_>>();
    Ok(vec![entry(
        KEYS_ENTRY,
        LearningPackEntryKind::Curriculum,
        &private_keys,
    )?])
}

/// The course's learning outcomes.
pub(super) async fn outcomes(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let outcome_rows=sqlx::query("SELECT id,module_id,lesson_id,title,description,ordinal,created_at FROM learning_outcome_definitions WHERE program_id=? ORDER BY ordinal,id").bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let outcomes:Vec<serde_json::Value>=outcome_rows.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"moduleId":r.get::<Option<String>,_>("module_id"),"lessonId":r.get::<Option<String>,_>("lesson_id"),"title":r.get::<String,_>("title"),"description":r.get::<String,_>("description"),"ordinal":r.get::<i64,_>("ordinal"),"createdAt":r.get::<i64,_>("created_at")})).collect();
    Ok(vec![entry(
        OUTCOMES_ENTRY,
        LearningPackEntryKind::Curriculum,
        &outcomes,
    )?])
}

/// Learner evidence events.
pub(super) async fn events(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let evidence=sqlx::query("SELECT id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at FROM learning_evidence_events WHERE program_id=? ORDER BY observed_at,id").bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let rows:Vec<serde_json::Value>=evidence.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"outcomeId":r.get::<Option<String>,_>("outcome_id"),"sourceKind":r.get::<String,_>("source_kind"),"sourceId":r.get::<String,_>("source_id"),"dimension":r.get::<String,_>("dimension"),"result":r.get::<String,_>("result"),"score":r.get::<Option<f64>,_>("score"),"observation":r.get::<String,_>("observation"),"evidenceQuote":r.get::<Option<String>,_>("evidence_quote"),"assistance":r.get::<String,_>("assistance_json"),"observedAt":r.get::<i64,_>("observed_at")})).collect();
    Ok(vec![entry(
        EVIDENCE_ENTRY,
        LearningPackEntryKind::Evidence,
        &rows,
    )?])
}

/// Follow-up recommendations drawn from the evidence.
pub(super) async fn follow_ups(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let followups=sqlx::query("SELECT id,outcome_id,reason_code,explanation,action_kind,action_ref,status,evidence_event_ids_json,created_at,decided_at FROM learning_follow_up_recommendations WHERE program_id=? ORDER BY created_at,id").bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let rows:Vec<serde_json::Value>=followups.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"outcomeId":r.get::<Option<String>,_>("outcome_id"),"reasonCode":r.get::<String,_>("reason_code"),"explanation":r.get::<String,_>("explanation"),"actionKind":r.get::<String,_>("action_kind"),"actionRef":r.get::<Option<String>,_>("action_ref"),"status":r.get::<String,_>("status"),"evidenceEventIds":r.get::<String,_>("evidence_event_ids_json"),"createdAt":r.get::<i64,_>("created_at"),"decidedAt":r.get::<Option<i64>,_>("decided_at")})).collect();
    Ok(vec![entry(
        FOLLOW_UP_ENTRY,
        LearningPackEntryKind::Evidence,
        &rows,
    )?])
}

/// Practical activities, their runs and simulations.
pub(super) async fn practical(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let practical = LearningPracticalRepository::workspace_on(tx, program_id).await?;
    let sessions=sqlx::query("SELECT id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at FROM learning_simulation_sessions WHERE program_id=? ORDER BY created_at,id")
                .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let simulations:Vec<serde_json::Value>=sessions.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"activityId":r.get::<String,_>("activity_id"),"activityRevision":r.get::<i64,_>("activity_revision"),"activitySnapshot":r.get::<String,_>("activity_snapshot_json"),"practiceSessionId":r.get::<Option<String>,_>("practice_session_id"),"operationId":r.get::<String,_>("operation_id"),"payloadHash":r.get::<String,_>("payload_hash"),"learnerRole":r.get::<String,_>("learner_role"),"counterpartRole":r.get::<String,_>("counterpart_role"),"status":r.get::<String,_>("status"),"revision":r.get::<i64,_>("revision"),"createdAt":r.get::<i64,_>("created_at"),"updatedAt":r.get::<i64,_>("updated_at"),"submittedAt":r.get::<Option<i64>,_>("submitted_at")})).collect();
    let turns=sqlx::query("SELECT id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at FROM learning_simulation_turns WHERE program_id=? ORDER BY session_id,ordinal")
                .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let simulation_turns:Vec<serde_json::Value>=turns.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"sessionId":r.get::<String,_>("session_id"),"operationId":r.get::<String,_>("operation_id"),"payloadHash":r.get::<String,_>("payload_hash"),"ordinal":r.get::<i64,_>("ordinal"),"speaker":r.get::<String,_>("speaker"),"content":r.get::<String,_>("content"),"citations":r.get::<String,_>("citations_json"),"modelName":r.get::<Option<String>,_>("model_name"),"createdAt":r.get::<i64,_>("created_at")})).collect();
    let raw_runs=sqlx::query("SELECT id,operation_id,payload_hash,activity_snapshot_json FROM learning_practical_runs WHERE program_id=? ORDER BY created_at,id")
                .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let run_snapshots:Vec<serde_json::Value>=raw_runs.into_iter().map(|r|serde_json::json!({"runId":r.get::<String,_>("id"),"operationId":r.get::<String,_>("operation_id"),"payloadHash":r.get::<String,_>("payload_hash"),"activitySnapshot":r.get::<String,_>("activity_snapshot_json")})).collect();
    let snapshot = PracticalPackSnapshot {
        // Export saved data; availability is recomputed on the receiving device.
        activities: practical.workspace.activities,
        runs: practical.workspace.runs,
        simulations,
        simulation_turns,
        run_snapshots,
    };
    Ok(vec![entry(
        PRACTICAL_ENTRY,
        LearningPackEntryKind::PracticalArtifact,
        &snapshot,
    )?])
}

/// The source library, each version stamped with its body's digest.
pub(super) async fn source_library(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let mut source_workspace =
        LearningSourceLibraryRepository::workspace_on(tx, program_id, true).await?;
    let source_rows =
        sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=?")
            .bind(program_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
    let source_bodies: HashMap<String, String> = source_rows
        .into_iter()
        .map(|row| (row.get("id"), row.get("full_text")))
        .collect();
    for source in &mut source_workspace.sources {
        for version in &mut source.versions {
            let body = source_bodies.get(&version.id).ok_or_else(|| {
                AppError::Database("Exported source snapshot is incomplete.".into())
            })?;
            version.content_sha256 = format!("{:x}", Sha256::digest(body.as_bytes()));
            version.word_count = body.split_whitespace().count();
        }
        let summaries = source.versions.clone();
        if let Some(active) = &source.active_version_id {
            source.active_version = summaries.iter().find(|v| &v.id == active).cloned();
        }
        if let Some(pending) = &source.pending_version_id {
            source.pending_version = summaries.iter().find(|v| &v.id == pending).cloned();
        }
    }
    Ok(vec![entry(
        SOURCE_LIBRARY_ENTRY,
        LearningPackEntryKind::SourceMetadata,
        &source_workspace,
    )?])
}

/// Source refresh checks.
pub(super) async fn source_history(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let check_rows=sqlx::query("SELECT operation_id,source_id,status,checked_at,active_digest,pending_version_id,message FROM learning_source_refresh_checks WHERE program_id=? ORDER BY checked_at,rowid").bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let checks:Vec<serde_json::Value>=check_rows.into_iter().map(|r|serde_json::json!({"operationId":r.get::<String,_>("operation_id"),"sourceId":r.get::<String,_>("source_id"),"status":r.get::<String,_>("status"),"checkedAt":r.get::<i64,_>("checked_at"),"activeDigest":r.get::<Option<String>,_>("active_digest"),"pendingVersionId":r.get::<Option<String>,_>("pending_version_id"),"message":r.get::<Option<String>,_>("message")})).collect();
    Ok(vec![entry(
        SOURCE_HISTORY_ENTRY,
        LearningPackEntryKind::SourceMetadata,
        &SourceHistorySnapshot { checks },
    )?])
}

/// Quote selectors into source versions.
pub(super) async fn selectors(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let selector_rows = sqlx::query("SELECT id,source_id,source_version_id,exact_text,prefix_text,suffix_text,start_byte,end_byte,candidate_count,status,created_at FROM learning_source_selectors WHERE program_id=? ORDER BY created_at,id")
            .bind(program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let selectors: Vec<serde_json::Value> = selector_rows.into_iter().map(|r| serde_json::json!({
            "id":r.get::<String,_>("id"), "sourceId":r.get::<String,_>("source_id"), "sourceVersionId":r.get::<String,_>("source_version_id"),
            "exactText":r.get::<String,_>("exact_text"), "prefixText":r.get::<String,_>("prefix_text"), "suffixText":r.get::<String,_>("suffix_text"),
            "startByte":r.get::<Option<i64>,_>("start_byte"), "endByte":r.get::<Option<i64>,_>("end_byte"),
            "candidateCount":r.get::<i64,_>("candidate_count"), "status":r.get::<String,_>("status"), "createdAt":r.get::<i64,_>("created_at")
        })).collect();
    Ok(vec![entry(
        SOURCE_SELECTORS_ENTRY,
        LearningPackEntryKind::SourceMetadata,
        &selectors,
    )?])
}

/// Every source version's full text, one content-addressed entry each.
pub(super) async fn source_bodies(
    tx: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<LearningPackEntry>> {
    let source_workspace =
        LearningSourceLibraryRepository::workspace_on(tx, program_id, true).await?;
    let source_rows =
        sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=?")
            .bind(program_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
    let source_bodies: HashMap<String, String> = source_rows
        .into_iter()
        .map(|row| (row.get("id"), row.get("full_text")))
        .collect();
    let mut entries = Vec::new();
    for source in &source_workspace.sources {
        for version in &source.versions {
            let body = source_bodies.get(&version.id).ok_or_else(|| {
                AppError::Database("Exported source snapshot is incomplete.".into())
            })?;
            entries.push(LearningPackEntry {
                path: source_body_path(&version.id),
                kind: LearningPackEntryKind::SourceBody,
                bytes: body.as_bytes().to_vec(),
            });
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::{
        dto::LearningProgramDto,
        pack::sections::PROGRAM_ENTRY,
        portability_dto::ExportLearningPackRequestDto,
        tests::{fixture, pool},
    };
    use sqlx::Connection;

    #[tokio::test]
    async fn export_keeps_program_and_related_entries_in_one_snapshot() -> Result<()> {
        let pool = pool().await?;
        let program = fixture();
        LearningRepository::new(pool.clone())
            .create(&program)
            .await?;
        let request = ExportLearningPackRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            options: LearningPackExportOptionsDto {
                include_evidence: true,
                include_practical_artifacts: true,
                include_source_bodies: true,
                source_body_redistribution_confirmed: true,
            },
        };
        let held = pool.acquire().await?;
        let mut read = Box::pin(snapshot(&pool, &request.program_id, &request.options));
        assert!(futures::poll!(&mut read).is_pending());
        let mut writer = Box::pin(pool.acquire());
        assert!(futures::poll!(&mut writer).is_pending());
        drop(held);
        let write = async {
            let mut connection = writer.await?;
            let mut tx = connection.begin().await?;
            sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=?")
                .bind(&program.summary.id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE learning_lessons SET completed=1,curriculum_state='completed' WHERE id=?",
            )
            .bind(&program.modules[0].lessons[0].id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await
        };
        let (read, written) = tokio::join!(read, write);
        written?;
        let read = read?;
        let program_entry = read
            .entries
            .iter()
            .find(|entry| entry.path == PROGRAM_ENTRY)
            .unwrap();
        let exported: LearningProgramDto = serde_json::from_slice(&program_entry.bytes)?;
        let state_entry = read
            .entries
            .iter()
            .find(|entry| entry.path == LESSON_STATE_ENTRY)
            .unwrap();
        let states: Vec<serde_json::Value> = serde_json::from_slice(&state_entry.bytes)?;
        let state = states
            .iter()
            .find(|value| value["lessonId"] == program.modules[0].lessons[0].id)
            .unwrap();
        assert_eq!(exported.summary.revision, program.summary.revision);
        assert!(!exported.modules[0].lessons[0].completed);
        assert_eq!(state["state"], "outline");
        assert_eq!(
            read.entries
                .iter()
                .filter(|entry| entry.path.starts_with("evidence/tables/"))
                .count(),
            super::super::evidence_tables::EVIDENCE_TABLES.len()
        );
        assert!(read.schema_version > 0);
        assert!(read
            .entries
            .iter()
            .any(|entry| entry.path == PRACTICAL_ENTRY));
        let after = snapshot(&pool, &request.program_id, &request.options).await?;
        let entry = after
            .entries
            .iter()
            .find(|entry| entry.path == PROGRAM_ENTRY)
            .unwrap();
        let after: LearningProgramDto = serde_json::from_slice(&entry.bytes)?;
        assert_eq!(after.summary.revision, program.summary.revision + 1);
        assert!(after.modules[0].lessons[0].completed);
        Ok(())
    }

    fn request(program_id: &str) -> ExportLearningPackRequestDto {
        ExportLearningPackRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program_id.into(),
            options: LearningPackExportOptionsDto {
                include_evidence: true,
                include_practical_artifacts: false,
                include_source_bodies: false,
                source_body_redistribution_confirmed: false,
            },
        }
    }

    #[tokio::test]
    async fn export_is_refused_while_a_job_is_preparing_the_course() -> Result<()> {
        let pool = pool().await?;
        let program = fixture();
        LearningRepository::new(pool.clone())
            .create(&program)
            .await?;
        let job = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_total,progress_message,created_at) VALUES(?,'learning_lesson',?,?,'job-hash','pending','{}',1,'Queued',1)")
            .bind(&job)
            .bind(&program.summary.id)
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO learning_jobs(job_id,program_id) VALUES(?,?)")
            .bind(&job)
            .bind(&program.summary.id)
            .execute(&pool)
            .await?;
        let error = snapshot(
            &pool,
            &program.summary.id,
            &request(&program.summary.id).options,
        )
        .await
        .expect_err("a course being generated is not exported");
        assert!(
            error.to_string().contains("still being prepared"),
            "{error}"
        );

        // A finished job no longer blocks.
        sqlx::query("UPDATE jobs SET status='failed',finished_at=2 WHERE id=?")
            .bind(&job)
            .execute(&pool)
            .await?;
        assert!(snapshot(
            &pool,
            &program.summary.id,
            &request(&program.summary.id).options
        )
        .await
        .is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn export_is_refused_before_the_course_has_modules() -> Result<()> {
        let pool = pool().await?;
        let mut program = fixture();
        program.modules.clear();
        program.summary.current_lesson_id = None;
        LearningRepository::new(pool.clone())
            .create(&program)
            .await?;
        let error = snapshot(
            &pool,
            &program.summary.id,
            &request(&program.summary.id).options,
        )
        .await
        .expect_err("an empty outline cannot be imported elsewhere");
        assert!(error.to_string().contains("has no modules yet"), "{error}");
        Ok(())
    }
}
