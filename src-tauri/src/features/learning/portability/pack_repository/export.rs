//! Read all portable program data from one SQLite snapshot. No file I/O or runtime services.
use super::{
    db, invalid, json, now, provenance, snapshot_evidence_tables, source_body_path,
    CanvasPackSnapshot, PracticalPackSnapshot, PrivateAnswerKey, SourceHistorySnapshot,
    CANVAS_ENTRY, EVIDENCE_ENTRY, FOLLOW_UP_ENTRY, KEYS_ENTRY, LESSON_STATE_ENTRY, OUTCOMES_ENTRY,
    PRACTICAL_ENTRY, PROGRAM_ENTRY,
};
use crate::features::learning::{
    pack::{
        LearningPackEntry, LearningPackEntryKind, LearningPackInput, LearningPackPrivacyManifest,
    },
    portability_dto::ExportLearningPackRequestDto,
    practical_repository::LearningPracticalRepository,
    repository::LearningRepository,
    source_library::LearningSourceLibraryRepository,
};
use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

pub(in crate::features::learning) async fn snapshot(
    pool: &SqlitePool,
    req: &ExportLearningPackRequestDto,
) -> Result<LearningPackInput> {
    let mut tx = pool.begin().await.map_err(db)?;
    let input = snapshot_on(&mut tx, req).await?;
    tx.commit().await.map_err(db)?;
    Ok(input)
}

/// The caller owns the transaction, including any write lock needed to keep a
/// replacement backup and its subsequent deletion on the same database state.
pub(super) async fn snapshot_on(
    tx: &mut sqlx::SqliteConnection,
    req: &ExportLearningPackRequestDto,
) -> Result<LearningPackInput> {
    let program = LearningRepository::get_on(tx, &req.program_id).await?;
    let mut export_program = program.clone();
    let exported_attempts = std::mem::take(&mut export_program.attempts);
    let mut entries = vec![LearningPackEntry {
        path: PROGRAM_ENTRY.into(),
        kind: LearningPackEntryKind::Program,
        bytes: json(&export_program)?,
    }];
    entries.push(LearningPackEntry {
        path: provenance::ENTRY.into(),
        kind: LearningPackEntryKind::Curriculum,
        bytes: json(&provenance::snapshot(tx, &program).await?)?,
    });
    let state_rows=sqlx::query("SELECT id,curriculum_state,replacement_lesson_id FROM learning_lessons WHERE program_id=? ORDER BY id")
            .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let lesson_states:Vec<serde_json::Value>=state_rows.into_iter().map(|r|serde_json::json!({"lessonId":r.get::<String,_>("id"),"state":r.get::<String,_>("curriculum_state"),"replacementLessonId":r.get::<Option<String>,_>("replacement_lesson_id")})).collect();
    entries.push(LearningPackEntry {
        path: LESSON_STATE_ENTRY.into(),
        kind: LearningPackEntryKind::Curriculum,
        bytes: json(&lesson_states)?,
    });
    let keys=sqlx::query("SELECT q.id question_id,k.correct_index,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id JOIN learning_lessons l ON l.id=q.lesson_id WHERE l.program_id=? ORDER BY q.id")
            .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let private_keys = keys
        .into_iter()
        .map(|r| PrivateAnswerKey {
            question_id: r.get("question_id"),
            correct_index: r.get::<i64, _>("correct_index") as usize,
            explanation: r.get("explanation"),
        })
        .collect::<Vec<_>>();
    entries.push(LearningPackEntry {
        path: KEYS_ENTRY.into(),
        kind: LearningPackEntryKind::Curriculum,
        bytes: json(&private_keys)?,
    });
    if req.include_evidence {
        entries.push(LearningPackEntry {
            path: "program/attempts.json".into(),
            kind: LearningPackEntryKind::Attempt,
            bytes: json(&exported_attempts)?,
        });
    }
    let outcome_rows=sqlx::query("SELECT id,module_id,lesson_id,title,description,ordinal,created_at FROM learning_outcome_definitions WHERE program_id=? ORDER BY ordinal,id").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let outcomes:Vec<serde_json::Value>=outcome_rows.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"moduleId":r.get::<Option<String>,_>("module_id"),"lessonId":r.get::<Option<String>,_>("lesson_id"),"title":r.get::<String,_>("title"),"description":r.get::<String,_>("description"),"ordinal":r.get::<i64,_>("ordinal"),"createdAt":r.get::<i64,_>("created_at")})).collect();
    entries.push(LearningPackEntry {
        path: OUTCOMES_ENTRY.into(),
        kind: LearningPackEntryKind::Curriculum,
        bytes: json(&outcomes)?,
    });
    if req.include_evidence {
        entries.push(LearningPackEntry {
            path: "evidence/learning-aggregate.json".into(),
            kind: LearningPackEntryKind::Evidence,
            bytes: json(&snapshot_evidence_tables(tx, &req.program_id).await?)?,
        });
        let evidence=sqlx::query("SELECT id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at FROM learning_evidence_events WHERE program_id=? ORDER BY observed_at,id").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let rows:Vec<serde_json::Value>=evidence.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"outcomeId":r.get::<Option<String>,_>("outcome_id"),"sourceKind":r.get::<String,_>("source_kind"),"sourceId":r.get::<String,_>("source_id"),"dimension":r.get::<String,_>("dimension"),"result":r.get::<String,_>("result"),"score":r.get::<Option<f64>,_>("score"),"observation":r.get::<String,_>("observation"),"evidenceQuote":r.get::<Option<String>,_>("evidence_quote"),"assistance":r.get::<String,_>("assistance_json"),"observedAt":r.get::<i64,_>("observed_at")})).collect();
        entries.push(LearningPackEntry {
            path: EVIDENCE_ENTRY.into(),
            kind: LearningPackEntryKind::Evidence,
            bytes: json(&rows)?,
        });
        let followups=sqlx::query("SELECT id,outcome_id,reason_code,explanation,action_kind,action_ref,status,evidence_event_ids_json,created_at,decided_at FROM learning_follow_up_recommendations WHERE program_id=? ORDER BY created_at,id").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let rows:Vec<serde_json::Value>=followups.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"outcomeId":r.get::<Option<String>,_>("outcome_id"),"reasonCode":r.get::<String,_>("reason_code"),"explanation":r.get::<String,_>("explanation"),"actionKind":r.get::<String,_>("action_kind"),"actionRef":r.get::<Option<String>,_>("action_ref"),"status":r.get::<String,_>("status"),"evidenceEventIds":r.get::<String,_>("evidence_event_ids_json"),"createdAt":r.get::<i64,_>("created_at"),"decidedAt":r.get::<Option<i64>,_>("decided_at")})).collect();
        entries.push(LearningPackEntry {
            path: FOLLOW_UP_ENTRY.into(),
            kind: LearningPackEntryKind::Evidence,
            bytes: json(&rows)?,
        });
    }
    if req.include_practical_artifacts {
        let practical = LearningPracticalRepository::workspace_on(tx, &req.program_id).await?;
        let sessions=sqlx::query("SELECT id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at FROM learning_simulation_sessions WHERE program_id=? ORDER BY created_at,id")
                .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let simulations:Vec<serde_json::Value>=sessions.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"activityId":r.get::<String,_>("activity_id"),"activityRevision":r.get::<i64,_>("activity_revision"),"activitySnapshot":r.get::<String,_>("activity_snapshot_json"),"practiceSessionId":r.get::<Option<String>,_>("practice_session_id"),"operationId":r.get::<String,_>("operation_id"),"payloadHash":r.get::<String,_>("payload_hash"),"learnerRole":r.get::<String,_>("learner_role"),"counterpartRole":r.get::<String,_>("counterpart_role"),"status":r.get::<String,_>("status"),"revision":r.get::<i64,_>("revision"),"createdAt":r.get::<i64,_>("created_at"),"updatedAt":r.get::<i64,_>("updated_at"),"submittedAt":r.get::<Option<i64>,_>("submitted_at")})).collect();
        let turns=sqlx::query("SELECT id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at FROM learning_simulation_turns WHERE program_id=? ORDER BY session_id,ordinal")
                .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let simulation_turns:Vec<serde_json::Value>=turns.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"sessionId":r.get::<String,_>("session_id"),"operationId":r.get::<String,_>("operation_id"),"payloadHash":r.get::<String,_>("payload_hash"),"ordinal":r.get::<i64,_>("ordinal"),"speaker":r.get::<String,_>("speaker"),"content":r.get::<String,_>("content"),"citations":r.get::<String,_>("citations_json"),"modelName":r.get::<Option<String>,_>("model_name"),"createdAt":r.get::<i64,_>("created_at")})).collect();
        let raw_runs=sqlx::query("SELECT id,operation_id,payload_hash,activity_snapshot_json FROM learning_practical_runs WHERE program_id=? ORDER BY created_at,id")
                .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let run_snapshots:Vec<serde_json::Value>=raw_runs.into_iter().map(|r|serde_json::json!({"runId":r.get::<String,_>("id"),"operationId":r.get::<String,_>("operation_id"),"payloadHash":r.get::<String,_>("payload_hash"),"activitySnapshot":r.get::<String,_>("activity_snapshot_json")})).collect();
        let sim_ops=sqlx::query("SELECT operation_id,session_id,kind,payload_hash,result_id,created_at FROM learning_simulation_operations WHERE program_id=? ORDER BY created_at,operation_id")
                .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let simulation_operations:Vec<serde_json::Value>=sim_ops.into_iter().map(|r|serde_json::json!({"operationId":r.get::<String,_>("operation_id"),"sessionId":r.get::<String,_>("session_id"),"kind":r.get::<String,_>("kind"),"payloadHash":r.get::<String,_>("payload_hash"),"resultId":r.get::<Option<String>,_>("result_id"),"createdAt":r.get::<i64,_>("created_at")})).collect();
        let activity_ops=sqlx::query("SELECT operation_id,activity_id,kind,payload_hash,result_revision,created_at FROM learning_practical_activity_operations WHERE program_id=? ORDER BY created_at,operation_id").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let mut practical_operations:Vec<serde_json::Value>=activity_ops.into_iter().map(|r|serde_json::json!({"table":"learning_practical_activity_operations","operationId":r.get::<String,_>("operation_id"),"activityId":r.get::<String,_>("activity_id"),"kind":r.get::<String,_>("kind"),"payloadHash":r.get::<String,_>("payload_hash"),"resultRevision":r.get::<i64,_>("result_revision"),"createdAt":r.get::<i64,_>("created_at")})).collect();
        let run_ops=sqlx::query("SELECT operation_id,run_id,kind,payload_hash,created_at FROM learning_practical_run_operations WHERE program_id=? ORDER BY created_at,operation_id").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        practical_operations.extend(run_ops.into_iter().map(|r|serde_json::json!({"table":"learning_practical_run_operations","operationId":r.get::<String,_>("operation_id"),"runId":r.get::<String,_>("run_id"),"kind":r.get::<String,_>("kind"),"payloadHash":r.get::<String,_>("payload_hash"),"createdAt":r.get::<i64,_>("created_at")})));
        let snapshot = PracticalPackSnapshot {
            // Export saved data; availability is recomputed on the receiving device.
            activities: practical.workspace.activities,
            runs: practical.workspace.runs,
            simulations,
            simulation_turns,
            run_snapshots,
            simulation_operations,
            practical_operations,
        };
        entries.push(LearningPackEntry {
            path: PRACTICAL_ENTRY.into(),
            kind: LearningPackEntryKind::PracticalArtifact,
            bytes: json(&snapshot)?,
        });
    }
    let mut source_workspace =
        LearningSourceLibraryRepository::workspace_on(tx, &req.program_id, true).await?;
    let source_rows =
        sqlx::query("SELECT id,full_text FROM learning_source_versions WHERE program_id=?")
            .bind(&req.program_id)
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
    entries.push(LearningPackEntry {
        path: "sources/library.json".into(),
        kind: LearningPackEntryKind::SourceMetadata,
        bytes: json(&source_workspace)?,
    });
    let operations=sqlx::query("SELECT operation_id,source_id,kind,payload_hash,result_version_id,result_revision,created_at FROM learning_source_operations WHERE program_id=? ORDER BY created_at,operation_id").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let operations:Vec<serde_json::Value>=operations.into_iter().map(|r|serde_json::json!({"operationId":r.get::<String,_>("operation_id"),"sourceId":r.get::<String,_>("source_id"),"kind":r.get::<String,_>("kind"),"payloadHash":r.get::<String,_>("payload_hash"),"resultVersionId":r.get::<Option<String>,_>("result_version_id"),"resultRevision":r.get::<i64,_>("result_revision"),"createdAt":r.get::<i64,_>("created_at")})).collect();
    let check_rows=sqlx::query("SELECT operation_id,source_id,status,checked_at,active_digest,pending_version_id,message FROM learning_source_refresh_checks WHERE program_id=? ORDER BY checked_at,rowid").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let checks:Vec<serde_json::Value>=check_rows.into_iter().map(|r|serde_json::json!({"operationId":r.get::<String,_>("operation_id"),"sourceId":r.get::<String,_>("source_id"),"status":r.get::<String,_>("status"),"checkedAt":r.get::<i64,_>("checked_at"),"activeDigest":r.get::<Option<String>,_>("active_digest"),"pendingVersionId":r.get::<Option<String>,_>("pending_version_id"),"message":r.get::<Option<String>,_>("message")})).collect();
    entries.push(LearningPackEntry {
        path: "sources/history.json".into(),
        kind: LearningPackEntryKind::SourceMetadata,
        bytes: json(&SourceHistorySnapshot { operations, checks })?,
    });
    if req.include_evidence {
        let canvas_rows=sqlx::query("SELECT id,lesson_id,title,description,scene_json,element_count,revision,created_at,updated_at FROM learning_canvases WHERE program_id=? ORDER BY created_at,id")
            .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let canvases:Vec<serde_json::Value>=canvas_rows.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"lessonId":r.get::<Option<String>,_>("lesson_id"),"title":r.get::<String,_>("title"),"description":r.get::<String,_>("description"),"sceneJson":r.get::<String,_>("scene_json"),"elementCount":r.get::<i64,_>("element_count"),"revision":r.get::<i64,_>("revision"),"createdAt":r.get::<i64,_>("created_at"),"updatedAt":r.get::<i64,_>("updated_at")})).collect();
        let snapshot_rows=sqlx::query("SELECT id,canvas_id,name,title,description,scene_json,element_count,canvas_revision,created_at FROM learning_canvas_snapshots WHERE program_id=? ORDER BY created_at,id")
            .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let snapshots:Vec<serde_json::Value>=snapshot_rows.into_iter().map(|r|serde_json::json!({"id":r.get::<String,_>("id"),"canvasId":r.get::<String,_>("canvas_id"),"name":r.get::<String,_>("name"),"title":r.get::<String,_>("title"),"description":r.get::<String,_>("description"),"sceneJson":r.get::<String,_>("scene_json"),"elementCount":r.get::<i64,_>("element_count"),"canvasRevision":r.get::<i64,_>("canvas_revision"),"createdAt":r.get::<i64,_>("created_at")})).collect();
        let operation_rows=sqlx::query("SELECT operation_id,canvas_id,kind,payload_hash,result_revision,result_snapshot_id,created_at FROM learning_canvas_operations WHERE program_id=? ORDER BY created_at,operation_id")
            .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let operations:Vec<serde_json::Value>=operation_rows.into_iter().map(|r|serde_json::json!({"operationId":r.get::<String,_>("operation_id"),"canvasId":r.get::<String,_>("canvas_id"),"kind":r.get::<String,_>("kind"),"payloadHash":r.get::<String,_>("payload_hash"),"resultRevision":r.get::<i64,_>("result_revision"),"resultSnapshotId":r.get::<Option<String>,_>("result_snapshot_id"),"createdAt":r.get::<i64,_>("created_at")})).collect();
        entries.push(LearningPackEntry {
            path: CANVAS_ENTRY.into(),
            kind: LearningPackEntryKind::Canvas,
            bytes: json(&CanvasPackSnapshot {
                canvases,
                snapshots,
                operations,
            })?,
        });
    }
    let selector_rows = sqlx::query("SELECT id,source_id,source_version_id,exact_text,prefix_text,suffix_text,start_byte,end_byte,candidate_count,status,created_at FROM learning_source_selectors WHERE program_id=? ORDER BY created_at,id")
            .bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
    let selectors: Vec<serde_json::Value> = selector_rows.into_iter().map(|r| serde_json::json!({
            "id":r.get::<String,_>("id"), "sourceId":r.get::<String,_>("source_id"), "sourceVersionId":r.get::<String,_>("source_version_id"),
            "exactText":r.get::<String,_>("exact_text"), "prefixText":r.get::<String,_>("prefix_text"), "suffixText":r.get::<String,_>("suffix_text"),
            "startByte":r.get::<Option<i64>,_>("start_byte"), "endByte":r.get::<Option<i64>,_>("end_byte"),
            "candidateCount":r.get::<i64,_>("candidate_count"), "status":r.get::<String,_>("status"), "createdAt":r.get::<i64,_>("created_at")
        })).collect();
    entries.push(LearningPackEntry {
        path: "sources/selectors.json".into(),
        kind: LearningPackEntryKind::SourceMetadata,
        bytes: json(&selectors)?,
    });
    let privacy = LearningPackPrivacyManifest {
            includes_private_chat: false,
            includes_credentials: false,
            includes_full_source_bodies: req.include_source_bodies,
            source_body_redistribution_confirmed: req.source_body_redistribution_confirmed,
            includes_answer_keys: true,
            includes_hidden_evaluators: false,
            includes_learner_evidence: req.include_evidence || req.include_practical_artifacts,
            includes_practical_artifacts: req.include_practical_artifacts,
            omitted_items: vec![
                "Lattice chat outside this program and credentials are never exported.".into(),
                "Saved outline citations and lesson verification reports are included as historical course provenance, independently of learner activity. Import does not reverify them.".into(),
                if !req.include_evidence {
                    "Submitted assessments, learner evidence, recall/review history, workbench history, memory, and canvas data were omitted by request.".into()
                } else {
                    "Assessment and curriculum history, practice workbench records (including tutor turns), memory/deck and recall history, and canvas documents are included because learner evidence was selected.".into()
                },
                if !req.include_practical_artifacts {
                    "Practical activities and learner artifacts were omitted by request.".into()
                } else {
                    "Hidden practical evaluator files are omitted; imported activities are review-only because runtime profiles and checks are not portable.".into()
                },
                "External notebook links and their note contents are omitted; copied memory records do not retain links to local journals.".into(),
                if !req.include_source_bodies {
                    "Full source bodies were omitted by request; only bounded excerpts and immutable version metadata are included.".into()
                } else {
                    "Full source bodies are included after redistribution-rights confirmation.".into()
                },
            ],
        };
    if req.include_source_bodies {
        if !req.source_body_redistribution_confirmed {
            return Err(invalid(
                "Confirm redistribution rights before exporting full source bodies.",
            ));
        }
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
    }
    Ok(LearningPackInput {
        pack_id: uuid::Uuid::new_v4().to_string(),
        title: program.summary.title.clone(),
        created_at: now(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        privacy,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::{
        dto::LearningProgramDto,
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
            file_name: "snapshot.latticepack".into(),
            include_evidence: true,
            include_practical_artifacts: true,
            include_source_bodies: true,
            source_body_redistribution_confirmed: true,
        };
        let held = pool.acquire().await?;
        let mut read = Box::pin(snapshot(&pool, &request));
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
        assert!(read
            .entries
            .iter()
            .any(|entry| entry.path == "evidence/learning-aggregate.json"));
        assert!(read
            .entries
            .iter()
            .any(|entry| entry.path == PRACTICAL_ENTRY));
        let after = snapshot(&pool, &request).await?;
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
}
