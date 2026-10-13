//! Persistence for accepted plan snapshots, diagnostics, and Learning's view of
//! its lesson jobs. Job rows and their lifecycle belong to the job runtime; this
//! repository admits lesson jobs against the course and keeps Learning's
//! `learning_jobs` facts beside them.
use crate::features::learning::{
    curriculum::{
        self, LearningCurriculumChange, LearningCurriculumLesson, LearningCurriculumLessonState,
        LearningCurriculumModule, LearningCurriculumRevision,
    },
    operations::Operation,
    persistence::{db, decode, encode, hash, hash_text, now},
    plan_dto::*,
};
use crate::shared::error::{AppError, Result};
#[cfg(test)]
use crate::{
    features::learning::{
        curriculum::{
            LearningGenerationJob, LearningGenerationJobKind, LearningGenerationJobStatus,
        },
        repository::LearningRepository,
    },
    shared::runtime::jobs::JobStore,
};
use sqlx::{Row, SqlitePool};

#[path = "generation_recovery.rs"]
mod generation_recovery;
pub(in crate::features::learning) use generation_recovery::{
    lesson_failure, lesson_job_config, LESSON_PREPARATION,
};

mod diagnostics;
mod jobs;
mod preparation;

use diagnostics::decode_diagnostic_row;

#[cfg(test)]
#[path = "generation_context_tests.rs"]
mod generation_context_tests;

pub(crate) fn stable_job_operation_id(program_id: &str, lesson_id: &str, revision: i64) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(
        format!("learning-prepare:{program_id}:{lesson_id}:{revision}").as_bytes(),
    );
    let mut bytes = [0u8; 16];
    if let Some(prefix) = digest.get(..bytes.len()) {
        bytes.copy_from_slice(prefix);
    }
    if let Some(version_byte) = bytes.get_mut(6) {
        *version_byte = (*version_byte & 0x0f) | 0x50;
    }
    if let Some(variant_byte) = bytes.get_mut(8) {
        *variant_byte = (*variant_byte & 0x3f) | 0x80;
    }
    uuid::Uuid::from_bytes(bytes).to_string()
}

fn curriculum_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
    conflict: &'static str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "curriculum",
        kind,
        program_id: Some(program_id),
        subject_id: None,
        payload_hash,
        conflict,
    }
}
fn diagnostic_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "diagnostic",
        kind,
        program_id: Some(program_id),
        subject_id: None,
        payload_hash,
        conflict: if kind == "submit" {
            "Operation ID was reused with different diagnostic answers."
        } else {
            "Operation ID was reused with a different diagnostic request."
        },
    }
}
fn validate_operation_id(value: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput("Operation ID must be a UUID.".into()))
}

#[derive(Clone)]
pub struct LearningCurriculumRepository {
    pool: SqlitePool,
}

impl LearningCurriculumRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(in crate::features::learning) async fn seed_accepted(
        &self,
        program_id: &str,
    ) -> Result<()> {
        // The UI and a background worker may initialize the same legacy plan.
        // Claim the writer before reading so they cannot both seed a snapshot.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(program_id).fetch_one(&mut *tx).await.map_err(db)?;
        if existing > 0 {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        let p = sqlx::query("SELECT id,current_lesson_id FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        let current: Option<String> = p.get("current_lesson_id");
        let mut modules = Vec::new();
        let module_rows =
            sqlx::query("SELECT * FROM learning_modules WHERE program_id=? ORDER BY ordinal")
                .bind(program_id)
                .fetch_all(&mut *tx)
                .await
                .map_err(db)?;
        for m in module_rows {
            let module_id: String = m.get("id");
            let outcome_labels: Vec<String> = decode(m.get("outcomes_json"))?;
            let mut outcome_ids = Vec::new();
            for (ordinal, label) in outcome_labels.iter().enumerate() {
                let existing:Option<String>=sqlx::query_scalar("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND module_id=? AND title=? ORDER BY ordinal LIMIT 1")
                    .bind(program_id).bind(&module_id).bind(label).fetch_optional(&mut *tx).await.map_err(db)?;
                let outcome_id = if let Some(id) = existing {
                    id
                } else {
                    let id = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,NULL,?,?,?,?)")
                        .bind(&id).bind(program_id).bind(&module_id).bind(label).bind(label).bind(ordinal as i64).bind(now()).execute(&mut *tx).await.map_err(db)?;
                    id
                };
                outcome_ids.push(outcome_id);
            }
            let mut lessons = Vec::new();
            for l in sqlx::query("SELECT * FROM learning_lessons WHERE program_id=? AND module_id=? ORDER BY ordinal")
                .bind(program_id).bind(&module_id).fetch_all(&mut *tx).await.map_err(db)? {
                let lesson_id: String = l.get("id");
                let practice_started: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_practice_sessions WHERE program_id=? AND lesson_id=?")
                    .bind(program_id).bind(&lesson_id).fetch_one(&mut *tx).await.map_err(db)?;
                let assessment_started: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_assessment_form_item_outcomes fio JOIN learning_assessment_forms f ON f.id=fio.form_id JOIN learning_outcome_definitions o ON o.id=fio.outcome_id WHERE f.program_id=? AND (o.lesson_id=? OR (o.lesson_id IS NULL AND o.module_id=?))")
                    .bind(program_id).bind(&lesson_id).bind(&module_id).fetch_one(&mut *tx).await.map_err(db)?;
                let completed = l.get::<i64,_>("completed") != 0;
                let preparation: String = l.get("preparation");
                lessons.push(LearningCurriculumLesson {
                    id: lesson_id,
                    title: l.get("title"),
                    objective: l.get("objective"),
                    estimated_minutes: l.get::<i64,_>("estimated_minutes").max(1) as u32,
                    state: if completed { LearningCurriculumLessonState::Completed }
                        else if preparation == "ready" { LearningCurriculumLessonState::Ready }
                        else { LearningCurriculumLessonState::Outline },
                    assessment_started: practice_started > 0 || assessment_started > 0,
                    replacement_lesson_id: None,
                });
            }
            modules.push(LearningCurriculumModule {
                id: module_id,
                title: m.get("title"),
                purpose: m.get("summary"),
                prerequisite_module_ids: decode(m.get("prerequisite_ids_json"))?,
                outcome_ids,
                lessons,
            });
        }
        if modules.is_empty() {
            return Err(AppError::InvalidState(
                "A program needs at least one module before its plan can be edited.".into(),
            ));
        }
        let revision = LearningCurriculumRevision {
            id: uuid::Uuid::new_v4().to_string(),
            program_id: program_id.into(),
            revision_number: 1,
            parent_revision_id: None,
            reason: "Initial accepted curriculum snapshot".into(),
            modules,
            created_at: now(),
        };
        curriculum::validate_curriculum(&revision)?;
        let json = encode(&revision)?;
        let digest = hash_text(&json);
        let required = revision
            .modules
            .iter()
            .flat_map(|m| &m.lessons)
            .filter(|l| {
                !matches!(
                    l.state,
                    LearningCurriculumLessonState::Skipped
                        | LearningCurriculumLessonState::Replaced
                        | LearningCurriculumLessonState::Challenged
                )
            })
            .count() as i64;
        sqlx::query("INSERT INTO learning_curriculum_revisions(id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,required_lesson_count,resume_lesson_id,created_at,accepted_at) VALUES(?,?,1,NULL,'accepted',?,?,?,?,?,?,?)")
            .bind(&revision.id).bind(program_id).bind(&revision.reason).bind(json).bind(digest).bind(required).bind(current).bind(revision.created_at).bind(revision.created_at).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn accepted_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        program_id: &str,
    ) -> Result<(String, LearningCurriculumRevision)> {
        let row = sqlx::query("SELECT id,snapshot_json FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(program_id).fetch_optional(&mut **tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;
        let mut snapshot: LearningCurriculumRevision = decode(row.get("snapshot_json"))?;
        for lesson in snapshot
            .modules
            .iter_mut()
            .flat_map(|m| m.lessons.iter_mut())
        {
            let row =
                sqlx::query("SELECT completed FROM learning_lessons WHERE id=? AND program_id=?")
                    .bind(&lesson.id)
                    .bind(program_id)
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(db)?;
            if let Some(row) = row {
                if row.get::<i64, _>("completed") != 0 {
                    lesson.state = LearningCurriculumLessonState::Completed;
                }
            }
            let practice:i64=sqlx::query_scalar("SELECT count(*) FROM learning_practice_sessions WHERE program_id=? AND lesson_id=?").bind(program_id).bind(&lesson.id).fetch_one(&mut **tx).await.map_err(db)?;
            let module_id: String = sqlx::query_scalar(
                "SELECT module_id FROM learning_lessons WHERE id=? AND program_id=?",
            )
            .bind(&lesson.id)
            .bind(program_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
            let forms:i64=sqlx::query_scalar("SELECT count(*) FROM learning_assessment_form_item_outcomes fio JOIN learning_assessment_forms f ON f.id=fio.form_id JOIN learning_outcome_definitions o ON o.id=fio.outcome_id WHERE f.program_id=? AND (o.lesson_id=? OR (o.lesson_id IS NULL AND o.module_id=?))").bind(program_id).bind(&lesson.id).bind(&module_id).fetch_one(&mut **tx).await.map_err(db)?;
            lesson.assessment_started |= practice > 0 || forms > 0;
        }
        Ok((row.get("id"), snapshot))
    }

    pub async fn plan(&self, program_id: &str) -> Result<LearningPlanDto> {
        self.seed_accepted(program_id).await?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let program_revision =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        let accepted_row = sqlx::query("SELECT snapshot_json,required_lesson_count,resume_lesson_id FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(program_id).fetch_one(&mut *tx).await.map_err(db)?;
        let accepted: LearningCurriculumRevision = decode(accepted_row.get("snapshot_json"))?;
        let draft_row = sqlx::query("SELECT snapshot_json,required_lesson_count,resume_lesson_id,id FROM learning_curriculum_revisions WHERE program_id=? AND status='draft' ORDER BY created_at DESC LIMIT 1")
            .bind(program_id).fetch_optional(&mut *tx).await.map_err(db)?;
        let (draft, changes, before, after, resume) = if let Some(row) = draft_row {
            let revision: LearningCurriculumRevision = decode(row.get("snapshot_json"))?;
            let change_rows = sqlx::query("SELECT operation,lesson_id,before_json,after_json,explanation FROM learning_curriculum_changes WHERE revision_id=? ORDER BY ordinal")
                .bind(row.get::<String,_>("id")).fetch_all(&mut *tx).await.map_err(db)?;
            let changes = change_rows
                .into_iter()
                .map(|r| {
                    let stored: Option<String> = r.get("after_json");
                    if let Some(value) = stored
                        .as_deref()
                        .and_then(|s| decode::<LearningCurriculumChange>(s).ok())
                    {
                        return value;
                    }
                    use crate::features::learning::curriculum::LearningCurriculumChangeKind as K;
                    let kind = match r.get::<String, _>("operation").as_str() {
                        "add" => K::Added,
                        "edit" => K::Edited,
                        "move" => K::Moved,
                        "skip" => K::Skipped,
                        "replace" => K::Replaced,
                        "challenge" => K::Challenged,
                        _ => K::Edited,
                    };
                    let from_module_id: Option<String> = r.get("before_json");
                    let to_module_id: Option<String> = r.get("after_json");
                    LearningCurriculumChange {
                        kind,
                        lesson_id: r.get::<Option<String>, _>("lesson_id").unwrap_or_default(),
                        from_module_id,
                        to_module_id,
                        before: None,
                        after: None,
                        description: r.get("explanation"),
                    }
                })
                .collect();
            (
                Some(revision),
                changes,
                accepted_row.get::<i64, _>("required_lesson_count") as usize,
                row.get::<i64, _>("required_lesson_count") as usize,
                row.get("resume_lesson_id"),
            )
        } else {
            (
                None,
                vec![],
                accepted_row.get::<i64, _>("required_lesson_count") as usize,
                accepted_row.get::<i64, _>("required_lesson_count") as usize,
                accepted_row.get("resume_lesson_id"),
            )
        };
        let jobs = Self::jobs_on(&mut tx, program_id).await?;
        let diagnostic = sqlx::query("SELECT * FROM learning_diagnostic_attempts WHERE program_id=? ORDER BY created_at DESC,id DESC LIMIT 1")
            .bind(program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .map(|r| decode_diagnostic_row(&r)).transpose()?;
        tx.commit().await.map_err(db)?;
        Ok(LearningPlanDto {
            program_id: program_id.into(),
            program_revision,
            accepted_revision: Some(accepted),
            draft_revision: draft,
            preview_changes: changes,
            required_lesson_count_before: before,
            required_lesson_count_after: after,
            resume_lesson_id: resume,
            jobs,
            latest_diagnostic: diagnostic,
        })
    }

    pub async fn preview(
        &self,
        request: &PreviewLearningCurriculumRevisionRequestDto,
    ) -> Result<LearningPlanDto> {
        validate_operation_id(&request.operation_id)?;
        self.seed_accepted(&request.program_id).await?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            "create_revision",
            &payload,
            "Operation ID was reused with a different curriculum request.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.plan(&request.program_id).await;
        }
        let program_revision: i64 =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(&request.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if program_revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let (accepted_id, base) = self.accepted_tx(&mut tx, &request.program_id).await?;
        let result = curriculum::apply_curriculum_revision(
            &base,
            None,
            crate::features::learning::curriculum::LearningCurriculumRevisionRequest {
                revision_id: uuid::Uuid::new_v4().to_string(),
                expected_revision_number: base.revision_number,
                reason: request.reason.clone(),
                created_at: now(),
                operations: request.operations.clone(),
            },
        )?;
        let json = encode(&result.revision)?;
        let digest = hash_text(&json);
        let existing_draft: Option<String> = sqlx::query_scalar(
            "SELECT id FROM learning_curriculum_revisions WHERE program_id=? AND status='draft'",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if let Some(id) = existing_draft {
            sqlx::query("DELETE FROM learning_curriculum_revisions WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let required = result.required_lesson_count_after as i64;
        let cursor: Option<String> =
            sqlx::query_scalar("SELECT current_lesson_id FROM learning_programs WHERE id=?")
                .bind(&request.program_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        let resume = curriculum::apply_curriculum_revision(
            &base,
            cursor.as_deref(),
            crate::features::learning::curriculum::LearningCurriculumRevisionRequest {
                revision_id: result.revision.id.clone(),
                expected_revision_number: base.revision_number,
                reason: request.reason.clone(),
                created_at: result.revision.created_at,
                operations: request.operations.clone(),
            },
        )?
        .resume_lesson_id;
        sqlx::query("INSERT INTO learning_curriculum_revisions(id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,required_lesson_count,resume_lesson_id,created_at,accepted_at) VALUES(?,?,?,?, 'draft',?,?,?,?,?,?,NULL)")
            .bind(&result.revision.id).bind(&request.program_id).bind(result.revision.revision_number as i64).bind(accepted_id).bind(&request.reason).bind(json).bind(digest).bind(required).bind(resume).bind(result.revision.created_at).execute(&mut *tx).await.map_err(db)?;
        for (ordinal, change) in result.changes.iter().enumerate() {
            let op = match change.kind {
                crate::features::learning::curriculum::LearningCurriculumChangeKind::Added => "add",
                crate::features::learning::curriculum::LearningCurriculumChangeKind::Edited => {
                    "edit"
                }
                crate::features::learning::curriculum::LearningCurriculumChangeKind::Moved => {
                    "move"
                }
                crate::features::learning::curriculum::LearningCurriculumChangeKind::Skipped => {
                    "skip"
                }
                crate::features::learning::curriculum::LearningCurriculumChangeKind::Replaced => {
                    "replace"
                }
                crate::features::learning::curriculum::LearningCurriculumChangeKind::Challenged => {
                    "challenge"
                }
            };
            sqlx::query("INSERT INTO learning_curriculum_changes(revision_id,ordinal,operation,lesson_id,before_json,after_json,explanation) VALUES(?,?,?,?,?,?,?)")
                .bind(&result.revision.id).bind(ordinal as i64).bind(op).bind(&change.lesson_id).bind(change.from_module_id.as_deref()).bind(encode(change)?).bind(&change.description).execute(&mut *tx).await.map_err(db)?;
        }
        operation.record(&mut *tx, &result.revision.id).await?;
        tx.commit().await.map_err(db)?;
        self.plan(&request.program_id).await
    }

    pub async fn accept_revision(
        &self,
        request: &LearningCurriculumRevisionActionRequestDto,
    ) -> Result<LearningPlanDto> {
        self.seed_accepted(&request.program_id).await?;
        self.accept_revision_tx(request).await?;
        self.plan(&request.program_id).await
    }

    async fn accept_revision_tx(
        &self,
        request: &LearningCurriculumRevisionActionRequestDto,
    ) -> Result<()> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            "accept_revision",
            &payload,
            "Operation ID was reused with different curriculum data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&request.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let draft=sqlx::query("SELECT snapshot_json,status FROM learning_curriculum_revisions WHERE id=? AND program_id=?").bind(&request.revision_id).bind(&request.program_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Draft curriculum revision not found".into()))?;
        if draft.get::<String, _>("status") != "draft" {
            return Err(AppError::InvalidState(
                "Only a draft curriculum can be accepted.".into(),
            ));
        }
        let accepted: LearningCurriculumRevision = decode(draft.get("snapshot_json"))?;
        curriculum::validate_curriculum(&accepted)?;
        sqlx::query("UPDATE learning_curriculum_revisions SET status='superseded' WHERE program_id=? AND status='accepted'").bind(&request.program_id).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE learning_curriculum_revisions SET status='accepted',accepted_at=? WHERE id=? AND status='draft'").bind(now()).bind(&request.revision_id).execute(&mut *tx).await.map_err(db)?;
        materialize(&mut tx, &accepted).await?;
        sqlx::query("UPDATE learning_programs SET revision=revision+1,current_lesson_id=? WHERE id=? AND revision=?").bind(sqlx::query_scalar::<_,Option<String>>("SELECT resume_lesson_id FROM learning_curriculum_revisions WHERE id=?").bind(&request.revision_id).fetch_one(&mut *tx).await.map_err(db)?).bind(&request.program_id).bind(request.expected_revision).execute(&mut *tx).await.map_err(db)?;
        operation.record(&mut *tx, &request.revision_id).await?;
        tx.commit().await.map_err(db)
    }

    pub async fn discard_revision(
        &self,
        request: &DiscardLearningCurriculumRevisionRequestDto,
    ) -> Result<LearningPlanDto> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            "discard_revision",
            &payload,
            "Operation ID was reused with different discard data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.plan(&request.program_id).await;
        }
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&request.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let changed=sqlx::query("DELETE FROM learning_curriculum_revisions WHERE id=? AND program_id=? AND status='draft'").bind(&request.revision_id).bind(&request.program_id).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::NotFound(
                "Draft curriculum revision not found".into(),
            ));
        }
        operation.record(&mut *tx, &request.revision_id).await?;
        tx.commit().await.map_err(db)?;
        self.plan(&request.program_id).await
    }
}

async fn materialize(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    revision: &LearningCurriculumRevision,
) -> Result<()> {
    // Stage ordinals out of the target range before applying the reordered
    // snapshot; SQLite checks both unique keys per row, not at commit time.
    sqlx::query("UPDATE learning_modules SET ordinal=ordinal+1000000 WHERE program_id=?")
        .bind(&revision.program_id)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_lessons SET ordinal=ordinal+1000000 WHERE program_id=?")
        .bind(&revision.program_id)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    for (mi, module) in revision.modules.iter().enumerate() {
        let mut outcome_labels = Vec::new();
        for outcome_id in &module.outcome_ids {
            let title:Option<String>=sqlx::query_scalar("SELECT title FROM learning_outcome_definitions WHERE program_id=? AND module_id=? AND id=?").bind(&revision.program_id).bind(&module.id).bind(outcome_id).fetch_optional(&mut **tx).await.map_err(db)?;
            let title = title.ok_or_else(|| {
                AppError::InvalidInput("Curriculum outcome must be a saved program outcome.".into())
            })?;
            outcome_labels.push(title);
        }
        sqlx::query("UPDATE learning_modules SET ordinal=?,title=?,summary=?,outcomes_json=? WHERE id=? AND program_id=?")
            .bind(mi as i64).bind(&module.title).bind(&module.purpose).bind(encode(&outcome_labels)?).bind(&module.id).bind(&revision.program_id).execute(&mut **tx).await.map_err(db)?;
        for (li, lesson) in module.lessons.iter().enumerate() {
            let old = sqlx::query("SELECT id FROM learning_lessons WHERE id=? AND program_id=?")
                .bind(&lesson.id)
                .bind(&revision.program_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
            if old.is_none() {
                sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation,completed,curriculum_state,replacement_lesson_id) VALUES(?,?,?,?,?,?,?,'outline',0,?,?)")
                    .bind(&lesson.id).bind(&revision.program_id).bind(&module.id).bind(li as i64).bind(&lesson.title).bind(&lesson.objective).bind(lesson.estimated_minutes as i64).bind(state_name(lesson.state)).bind(&lesson.replacement_lesson_id).execute(&mut **tx).await.map_err(db)?;
            } else {
                let protected: i64 =
                    sqlx::query_scalar("SELECT completed FROM learning_lessons WHERE id=?")
                        .bind(&lesson.id)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(db)?;
                if protected == 0 && !lesson.assessment_started {
                    sqlx::query("UPDATE learning_lessons SET module_id=?,ordinal=?,title=?,objective=?,estimated_minutes=?,curriculum_state=?,replacement_lesson_id=? WHERE id=? AND program_id=?")
                    .bind(&module.id).bind(li as i64).bind(&lesson.title).bind(&lesson.objective).bind(lesson.estimated_minutes as i64).bind(state_name(lesson.state)).bind(&lesson.replacement_lesson_id).bind(&lesson.id).bind(&revision.program_id).execute(&mut **tx).await.map_err(db)?;
                    sqlx::query("UPDATE learning_questions SET module_id=? WHERE lesson_id=?")
                        .bind(&module.id)
                        .bind(&lesson.id)
                        .execute(&mut **tx)
                        .await
                        .map_err(db)?;
                } else {
                    sqlx::query(
                        "UPDATE learning_lessons SET ordinal=? WHERE id=? AND program_id=?",
                    )
                    .bind(li as i64)
                    .bind(&lesson.id)
                    .bind(&revision.program_id)
                    .execute(&mut **tx)
                    .await
                    .map_err(db)?;
                }
            }
        }
    }
    Ok(())
}

fn state_name(state: LearningCurriculumLessonState) -> &'static str {
    match state {
        LearningCurriculumLessonState::Outline => "outline",
        LearningCurriculumLessonState::Ready => "ready",
        LearningCurriculumLessonState::Completed => "completed",
        LearningCurriculumLessonState::Skipped => "skipped",
        LearningCurriculumLessonState::Replaced => "replaced",
        LearningCurriculumLessonState::Challenged => "challenged",
    }
}

/// Learning's facts about a lesson job, kept beside the runtime's row.
#[derive(Clone)]
struct JobScope {
    program_id: String,
    base_revision_number: i64,
    content_revision: Option<i64>,
}

#[cfg(test)]
mod tests;
