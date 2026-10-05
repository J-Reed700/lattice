//! Persistence for accepted plan snapshots, diagnostics, and generation jobs.
use crate::features::learning::{
    curriculum::{
        self, LearningCurriculumChange, LearningCurriculumLesson, LearningCurriculumLessonState,
        LearningCurriculumModule, LearningCurriculumRevision, LearningGenerationJob,
        LearningGenerationJobKind, LearningGenerationJobStatus,
    },
    plan_dto::*,
    repository::LearningRepository,
};
use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

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

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn encode<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn decode<T: serde::de::DeserializeOwned>(v: &str) -> Result<T> {
    serde_json::from_str(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn hash(v: &str) -> String {
    format!("{:x}", Sha256::digest(v.as_bytes()))
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
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

    async fn seed_accepted(&self, program_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
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
        let digest = hash(&json);
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
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row) = sqlx::query("SELECT payload_hash,result_id FROM learning_curriculum_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("payload_hash") != payload { return Err(AppError::InvalidInput("Operation ID was reused with a different curriculum request.".into())); }
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
        let digest = hash(&json);
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
        sqlx::query("INSERT INTO learning_curriculum_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'create_revision',?,?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(payload).bind(&result.revision.id).bind(now()).execute(&mut *tx).await.map_err(db)?;
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
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row) = sqlx::query(
            "SELECT payload_hash FROM learning_curriculum_operations WHERE operation_id=?",
        )
        .bind(&request.operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            if row.get::<String, _>("payload_hash") != payload {
                return Err(AppError::InvalidInput(
                    "Operation ID was reused with different curriculum data.".into(),
                ));
            }
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
        sqlx::query("INSERT INTO learning_curriculum_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'accept_revision',?,?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(payload).bind(&request.revision_id).bind(now()).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn discard_revision(
        &self,
        request: &DiscardLearningCurriculumRevisionRequestDto,
    ) -> Result<LearningPlanDto> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row) = sqlx::query(
            "SELECT payload_hash FROM learning_curriculum_operations WHERE operation_id=?",
        )
        .bind(&request.operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            if row.get::<String, _>("payload_hash") != payload {
                return Err(AppError::InvalidInput(
                    "Operation ID was reused with different discard data.".into(),
                ));
            }
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
        sqlx::query("INSERT INTO learning_curriculum_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'discard_revision',?,?,?)").bind(&request.operation_id).bind(&request.program_id).bind(payload).bind(&request.revision_id).bind(now()).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.plan(&request.program_id).await
    }

    pub async fn diagnostic_replay<T: serde::Serialize>(
        &self,
        operation_id: &str,
        request: &T,
    ) -> Result<Option<LearningDiagnosticAttemptDto>> {
        validate_operation_id(operation_id)?;
        if let Some(row) = sqlx::query("SELECT diagnostic_id,payload_hash FROM learning_diagnostic_operations WHERE operation_id=?").bind(operation_id).fetch_optional(&self.pool).await.map_err(db)? {
            if row.get::<String,_>("payload_hash") != hash(&encode(request)?) { return Err(AppError::InvalidInput("Operation ID was reused with different diagnostic data.".into())); }
            return Ok(Some(self.diagnostic(&row.get::<String,_>("diagnostic_id")).await?));
        }
        Ok(None)
    }

    pub async fn diagnostic_for_program(
        &self,
        program_id: &str,
        diagnostic_id: &str,
    ) -> Result<LearningDiagnosticAttemptDto> {
        let attempt = self.diagnostic(diagnostic_id).await?;
        if attempt.program_id != program_id {
            return Err(AppError::NotFound(
                "Diagnostic not found in this program".into(),
            ));
        }
        Ok(attempt)
    }

    pub async fn diagnostic_tasks(
        &self,
        program_id: &str,
        diagnostic_id: &str,
    ) -> Result<Vec<crate::features::learning::diagnostic_generation::DiagnosticTask>> {
        let encoded: String = sqlx::query_scalar(
            "SELECT evaluation_json FROM learning_diagnostic_attempts WHERE id=? AND program_id=?",
        )
        .bind(diagnostic_id)
        .bind(program_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Diagnostic not found".into()))?;
        decode(&encoded)
    }

    pub async fn start_diagnostic(
        &self,
        request: &StartLearningDiagnosticRequestDto,
    ) -> Result<LearningDiagnosticAttemptDto> {
        self.start_diagnostic_authored(request, None).await
    }

    pub async fn start_diagnostic_authored(
        &self,
        request: &StartLearningDiagnosticRequestDto,
        tasks: Option<&[crate::features::learning::diagnostic_generation::DiagnosticTask]>,
    ) -> Result<LearningDiagnosticAttemptDto> {
        validate_operation_id(&request.operation_id)?;
        self.seed_accepted(&request.program_id).await?;
        let program = LearningRepository::new(self.pool.clone())
            .get(&request.program_id)
            .await?;
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row)=sqlx::query("SELECT diagnostic_id,payload_hash FROM learning_diagnostic_operations WHERE operation_id=?").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("payload_hash")!=payload { return Err(AppError::InvalidInput("Operation ID was reused with a different diagnostic request.".into())); }
            let id:String=row.get("diagnostic_id"); tx.commit().await.map_err(db)?; return self.diagnostic(&id).await;
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM learning_programs WHERE id=? AND status='active'",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Active learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let accepted_json: String = sqlx::query_scalar("SELECT snapshot_json FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(&request.program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;
        let accepted: LearningCurriculumRevision = decode(&accepted_json)?;
        let mut prompts = Vec::new();
        for module in &accepted.modules {
            let source_module = program.modules.iter().find(|source| source.id == module.id);
            let source_ids: Vec<String> = module
                .lessons
                .iter()
                .flat_map(|lesson| {
                    source_module
                        .and_then(|source| source.lessons.iter().find(|item| item.id == lesson.id))
                        .into_iter()
                        .flat_map(|item| {
                            item.blocks
                                .iter()
                                .flat_map(|block| block.source_ids.clone())
                                .chain(
                                    item.questions
                                        .iter()
                                        .flat_map(|question| question.source_ids.clone()),
                                )
                        })
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            for outcome in &module.outcome_ids {
                let title = sqlx::query_scalar::<_, String>(
                    "SELECT title FROM learning_outcome_definitions WHERE program_id=? AND id=?",
                )
                .bind(&request.program_id)
                .bind(outcome)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| {
                    AppError::InvalidState(
                        "Accepted curriculum references a missing learning outcome.".into(),
                    )
                })?;
                let prompt = if let Some(tasks) = tasks {
                    let Some(task) = tasks.iter().find(|t| &t.outcome_id == outcome) else {
                        continue;
                    };
                    task.prompt.clone()
                } else {
                    format!("Self-inventory: What do you currently understand about {title}? Share a question or example if useful.")
                };
                prompts.push(LearningDiagnosticPromptDto {
                    id: uuid::Uuid::new_v4().to_string(),
                    prompt,
                    outcome_id: outcome.clone(),
                    outcome_title: title,
                    source_version_ids: source_ids.clone(),
                });
            }
        }
        if prompts.is_empty() || tasks.is_some_and(|t| t.len() != prompts.len() || t.len() > 6) {
            return Err(AppError::InvalidState(
                "The accepted program has no outcomes for a diagnostic.".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created = now();
        sqlx::query("INSERT INTO learning_diagnostic_attempts(id,program_id,operation_id,status,prompt_json,response_json,findings_json,source_coverage_gaps_json,created_at,submitted_at,evaluation_json) VALUES(?,?,?,'active',?,'[]','[]','[]',?,NULL,?)")
            .bind(&id).bind(&request.program_id).bind(&request.operation_id).bind(encode(&prompts)?).bind(created).bind(encode(&tasks.unwrap_or_default())?).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_diagnostic_operations(operation_id,program_id,diagnostic_id,kind,payload_hash,created_at) VALUES(?,?,?,'start',?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&id).bind(payload).bind(created).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.diagnostic(&id).await
    }

    pub async fn submit_diagnostic(
        &self,
        request: &SubmitLearningDiagnosticRequestDto,
    ) -> Result<LearningDiagnosticAttemptDto> {
        self.submit_diagnostic_evaluated(request, None).await
    }

    pub async fn submit_diagnostic_evaluated(
        &self,
        request: &SubmitLearningDiagnosticRequestDto,
        findings: Option<&[LearningDiagnosticFindingDto]>,
    ) -> Result<LearningDiagnosticAttemptDto> {
        validate_operation_id(&request.operation_id)?;
        if let Some(replay) = self
            .diagnostic_replay(&request.operation_id, request)
            .await?
        {
            return Ok(replay);
        }
        let attempt = self
            .diagnostic_for_program(&request.program_id, &request.diagnostic_id)
            .await?;
        let tasks = self
            .diagnostic_tasks(&request.program_id, &request.diagnostic_id)
            .await?;
        if !tasks.is_empty() && !request.save_only {
            crate::features::learning::diagnostic_generation::validate_findings(
                &attempt,
                &request.responses,
                findings.ok_or_else(|| {
                    AppError::InvalidInput(
                        "These performance tasks require feedback before submission.".into(),
                    )
                })?,
            )?;
        } else if findings.is_some() {
            return Err(AppError::InvalidInput(
                "A self-inventory cannot be presented as a scored diagnostic.".into(),
            ));
        }
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row)=sqlx::query("SELECT diagnostic_id,payload_hash FROM learning_diagnostic_operations WHERE operation_id=?").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("payload_hash")!=payload { return Err(AppError::InvalidInput("Operation ID was reused with different diagnostic answers.".into())); }
            let id:String=row.get("diagnostic_id"); tx.commit().await.map_err(db)?; return self.diagnostic(&id).await;
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
        let row=sqlx::query("SELECT prompt_json,status,revision FROM learning_diagnostic_attempts WHERE id=? AND program_id=?").bind(&request.diagnostic_id).bind(&request.program_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Diagnostic attempt not found".into()))?;
        if row.get::<String, _>("status") != "active" {
            return Err(AppError::InvalidState(
                "This diagnostic attempt is already closed.".into(),
            ));
        }
        let diagnostic_revision: i64 = row.get("revision");
        if request
            .expected_diagnostic_revision
            .is_some_and(|revision| revision != diagnostic_revision)
            || (!tasks.is_empty() && request.expected_diagnostic_revision.is_none())
        {
            return Err(AppError::InvalidState(
                "The starting-point answers changed elsewhere. Reload before saving.".into(),
            ));
        }
        let prompts: Vec<LearningDiagnosticPromptDto> = decode(row.get("prompt_json"))?;
        let expected: std::collections::HashSet<_> =
            prompts.iter().map(|p| p.id.as_str()).collect();
        let mut seen = std::collections::HashSet::new();
        for response in &request.responses {
            if !expected.contains(response.prompt_id.as_str())
                || !seen.insert(response.prompt_id.as_str())
            {
                return Err(AppError::InvalidInput(
                    "Diagnostic responses must use each prompt ID at most once.".into(),
                ));
            }
            if response.response.trim().chars().count() > 4000 {
                return Err(AppError::InvalidInput(
                    "Diagnostic responses must be 4000 characters or fewer.".into(),
                ));
            }
            if response.response.trim().is_empty() && !request.save_only {
                return Err(AppError::InvalidInput(
                    "Answer every self-inventory prompt or skip the diagnostic.".into(),
                ));
            }
        }
        if seen.len() != expected.len() && !request.save_only {
            return Err(AppError::InvalidInput(
                "Answer every self-inventory prompt or skip the diagnostic.".into(),
            ));
        }
        let responses_json = encode(&request.responses)?;
        let mut gaps = Vec::new();
        for prompt in &prompts {
            if prompt.source_version_ids.is_empty() {
                gaps.push(LearningDiagnosticCoverageGapDto {
                    outcome_id: prompt.outcome_id.clone(),
                    outcome_title: prompt.outcome_title.clone(),
                    source_version_ids: vec![],
                });
            }
        }
        sqlx::query("UPDATE learning_diagnostic_attempts SET status=?,response_json=?,findings_json=?,source_coverage_gaps_json=?,submitted_at=?,revision=revision+1 WHERE id=? AND status='active'")
            .bind(if request.save_only { "active" } else { "submitted" }).bind(responses_json).bind(encode(&findings.unwrap_or_default())?).bind(encode(&gaps)?).bind(if request.save_only { None } else { Some(now()) }).bind(&request.diagnostic_id).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_diagnostic_operations(operation_id,program_id,diagnostic_id,kind,payload_hash,created_at) VALUES(?,?,?,'submit',?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.diagnostic_id).bind(payload).bind(now()).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.diagnostic(&request.diagnostic_id).await
    }

    pub async fn skip_diagnostic(
        &self,
        request: &SkipLearningDiagnosticRequestDto,
    ) -> Result<LearningDiagnosticAttemptDto> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row)=sqlx::query("SELECT diagnostic_id,payload_hash FROM learning_diagnostic_operations WHERE operation_id=?").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("payload_hash")!=payload { return Err(AppError::InvalidInput("Operation ID was reused with a different diagnostic request.".into())); }
            let id:String=row.get("diagnostic_id"); tx.commit().await.map_err(db)?; return self.diagnostic(&id).await;
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM learning_programs WHERE id=? AND status='active'",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Active learning program not found".into()))?;
        if revision != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created = now();
        let prompts = serde_json::json!([]);
        sqlx::query("INSERT INTO learning_diagnostic_attempts(id,program_id,operation_id,status,prompt_json,response_json,findings_json,source_coverage_gaps_json,created_at,submitted_at) VALUES(?,?,?,'skipped',?,'[]','[]','[]',?,?)")
            .bind(&id).bind(&request.program_id).bind(&request.operation_id).bind(prompts.to_string()).bind(created).bind(created).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_diagnostic_operations(operation_id,program_id,diagnostic_id,kind,payload_hash,created_at) VALUES(?,?,?,'skip',?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&id).bind(payload).bind(created).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.diagnostic(&id).await
    }

    async fn diagnostic(&self, id: &str) -> Result<LearningDiagnosticAttemptDto> {
        let row = sqlx::query("SELECT * FROM learning_diagnostic_attempts WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Diagnostic attempt not found".into()))?;
        decode_diagnostic_row(&row)
    }

    pub async fn jobs(&self, program_id: &str) -> Result<Vec<LearningGenerationJob>> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::jobs_on(&mut connection, program_id).await
    }

    async fn jobs_on(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<LearningGenerationJob>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_generation_jobs WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(connection)
        .await
        .map_err(db)?;
        rows.into_iter().map(parse_job).collect()
    }

    /// Jobs are persisted before their worker is scheduled. If the process exits
    /// between those steps, startup can safely resume these still-pending jobs.
    pub async fn pending_jobs(&self) -> Result<Vec<LearningGenerationJob>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_generation_jobs WHERE status='pending' ORDER BY created_at,id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter().map(parse_job).collect()
    }

    /// Explicit lesson preparation may resume an existing retry chain. Concurrent
    /// clicks share one stable retry operation and only one worker can claim it.
    pub async fn start_lesson_job(
        &self,
        request: &StartLearningGenerationJobRequestDto,
    ) -> Result<LearningGenerationJob> {
        if request.progress_total != 1 {
            return Err(AppError::InvalidInput(
                "Select one lesson to prepare.".into(),
            ));
        }
        let mut job = self.start_job(request).await?;
        let jobs = self.jobs(&request.program_id).await?;
        let mut visited = std::collections::HashSet::new();
        while let Some(child) = jobs
            .iter()
            .find(|candidate| candidate.retry_of_job_id.as_deref() == Some(job.id.as_str()))
        {
            if !visited.insert(job.id.clone()) {
                return Err(AppError::InvalidState(
                    "The lesson retry history is invalid.".into(),
                ));
            }
            job = child.clone();
        }
        if matches!(
            job.status,
            LearningGenerationJobStatus::Failed
                | LearningGenerationJobStatus::Interrupted
                | LearningGenerationJobStatus::Cancelled
        ) {
            job = self
                .retry_job(&LearningGenerationJobActionRequestDto {
                    operation_id: stable_job_operation_id(
                        &request.program_id,
                        &format!("retry-{}", job.id),
                        request.expected_revision,
                    ),
                    program_id: request.program_id.clone(),
                    job_id: job.id.clone(),
                    expected_revision: request.expected_revision,
                })
                .await?;
        }
        Ok(job)
    }

    pub async fn start_job(
        &self,
        request: &StartLearningGenerationJobRequestDto,
    ) -> Result<LearningGenerationJob> {
        validate_operation_id(&request.operation_id)?;
        if !(1..=3).contains(&request.progress_total) {
            return Err(AppError::InvalidInput(
                "Lesson preparation jobs must contain 1–3 bounded steps.".into(),
            ));
        }
        if request.kind != LearningGenerationJobKind::LessonPreparation {
            return Err(AppError::InvalidInput(
                "Only bounded lesson-preparation jobs are currently supported.".into(),
            ));
        }
        let parsed: serde_json::Value = decode(&request.request_json)?;
        if request.request_json.len() > 64 * 1024 {
            return Err(AppError::InvalidInput(
                "Generation job input is too large.".into(),
            ));
        }
        let lesson_ids = parsed
            .get("lessonIds")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| AppError::InvalidInput("Lesson jobs require lessonIds.".into()))?
            .iter()
            .map(|value| {
                let id = value
                    .as_str()
                    .ok_or_else(|| AppError::InvalidInput("Lesson IDs must be strings.".into()))?;
                uuid::Uuid::parse_str(id)
                    .map_err(|_| AppError::InvalidInput("Invalid lesson ID.".into()))?;
                Ok(id.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        if lesson_ids.len() != request.progress_total as usize
            || lesson_ids.is_empty()
            || lesson_ids.len() > 3
            || lesson_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != lesson_ids.len()
        {
            return Err(AppError::InvalidInput(
                "A job must name one to three unique lessons matching progressTotal.".into(),
            ));
        }
        let digest = hash(&encode(&parsed)?);
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row)=sqlx::query("SELECT payload_hash,result_id FROM learning_curriculum_operations WHERE operation_id=?").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("payload_hash")!=payload { return Err(AppError::InvalidInput("Operation ID was reused with different job data.".into())); }
            let id:String=row.get("result_id"); tx.commit().await.map_err(db)?; return self.job(&id).await;
        }
        let program = sqlx::query("SELECT revision,status FROM learning_programs WHERE id=?")
            .bind(&request.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if program.get::<i64, _>("revision") != request.expected_revision
            || program.get::<String, _>("status") != "active"
        {
            return Err(AppError::InvalidState(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let next_lessons: Vec<String> = sqlx::query_scalar("SELECT l.id FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? AND l.preparation='outline' AND l.completed=0 ORDER BY m.ordinal,l.ordinal")
            .bind(&request.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        if (lesson_ids.len() == 1
            && lesson_ids
                .first()
                .is_some_and(|id| !next_lessons.contains(id)))
            || (lesson_ids.len() > 1
                && next_lessons.get(..lesson_ids.len()) != Some(lesson_ids.as_slice()))
        {
            return Err(AppError::InvalidInput(
                "Select an unfinished outline lesson; batch preparation must follow course order."
                    .into(),
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created = now();
        let kind = job_kind_name(request.kind);
        sqlx::query("INSERT INTO learning_generation_jobs(id,program_id,operation_id,payload_hash,kind,status,requested_json,base_revision_number,progress_current,progress_total,progress_message,staged_result_json,published_result_id,error_code,error_message,retry_of_job_id,created_at,started_at,finished_at,heartbeat_at) VALUES(?,?,?,?,?,'pending',?,?,0,?,'Queued',NULL,NULL,NULL,NULL,NULL,?,NULL,NULL,?)")
            .bind(&id).bind(&request.program_id).bind(&request.operation_id).bind(digest).bind(kind).bind(encode(&parsed)?).bind(request.expected_revision).bind(request.progress_total as i64).bind(created).bind(created).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,0,'pending',0,'Queued',?)").bind(&id).bind(created).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_curriculum_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'start_job',?,?,?)").bind(&request.operation_id).bind(&request.program_id).bind(payload).bind(&id).bind(created).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.job(&id).await
    }

    pub async fn cancel_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        self.job_action(request, false).await
    }
    pub async fn retry_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        self.job_action(request, true).await
    }
    async fn job_action(
        &self,
        request: &LearningGenerationJobActionRequestDto,
        retry: bool,
    ) -> Result<LearningGenerationJob> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(&encode(request)?);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row)=sqlx::query("SELECT payload_hash,result_id FROM learning_curriculum_operations WHERE operation_id=?").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("payload_hash")!=payload { return Err(AppError::InvalidInput("Operation ID was reused with different job action data.".into())); }
            let id:String=row.get("result_id"); tx.commit().await.map_err(db)?; return self.job(&id).await;
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
        let old = sqlx::query("SELECT * FROM learning_generation_jobs WHERE id=? AND program_id=?")
            .bind(&request.job_id)
            .bind(&request.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        let status = old.get::<String, _>("status");
        let created = now();
        let result_id = if retry {
            if !matches!(status.as_str(), "failed" | "interrupted" | "cancelled") {
                return Err(AppError::InvalidState(
                    "Only failed, interrupted, or cancelled jobs can be retried.".into(),
                ));
            }
            let id = uuid::Uuid::new_v4().to_string();
            let digest: String = old.get("payload_hash");
            let kind: String = old.get("kind");
            let body: String = old.get("requested_json");
            let total: i64 = old.get("progress_total");
            let base: i64 = old.get("base_revision_number");
            if base != request.expected_revision {
                return Err(AppError::InvalidState(
                    "The program changed after this job was created; reload the plan before retrying.".into(),
                ));
            }
            let checkpoint: Option<String> = old.get("staged_result_json");
            sqlx::query("INSERT INTO learning_generation_jobs(id,program_id,operation_id,payload_hash,kind,status,requested_json,base_revision_number,progress_current,progress_total,progress_message,retry_of_job_id,created_at,heartbeat_at,staged_result_json) VALUES(?,?,? ,?,?,'pending',?,?,0,?,'Retry queued',?,?,?,?)")
                .bind(&id).bind(&request.program_id).bind(&request.operation_id).bind(&digest).bind(kind).bind(body).bind(base).bind(total).bind(&request.job_id).bind(created).bind(created).bind(checkpoint).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,0,'pending',0,'Retry queued',?)").bind(&id).bind(created).execute(&mut *tx).await.map_err(db)?;
            id
        } else {
            if !matches!(status.as_str(), "pending" | "running" | "interrupted") {
                return Err(AppError::InvalidState(
                    "This generation job has already finished.".into(),
                ));
            }
            sqlx::query("UPDATE learning_generation_jobs SET status='cancelled',finished_at=?,published_result_id=NULL,error_code=NULL,error_message=NULL,progress_message='Cancelled' WHERE id=?")
                .bind(created).bind(&request.job_id).execute(&mut *tx).await.map_err(db)?;
            let ordinal:i64=sqlx::query_scalar("SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=?").bind(&request.job_id).fetch_one(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,?,'cancelled',?,'Cancelled',?)")
                .bind(&request.job_id).bind(ordinal).bind(old.get::<i64,_>("progress_current")).bind(created).execute(&mut *tx).await.map_err(db)?;
            request.job_id.clone()
        };
        let op_kind = if retry { "retry_job" } else { "cancel_job" };
        sqlx::query("INSERT INTO learning_curriculum_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,?,?,?)").bind(&request.operation_id).bind(&request.program_id).bind(op_kind).bind(payload).bind(&result_id).bind(created).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.job(&result_id).await
    }

    pub async fn job(&self, id: &str) -> Result<LearningGenerationJob> {
        let row = sqlx::query("SELECT * FROM learning_generation_jobs WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        parse_job(row)
    }
    pub async fn job_request_json(&self, id: &str) -> Result<String> {
        sqlx::query_scalar("SELECT requested_json FROM learning_generation_jobs WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))
    }

    pub async fn begin_job(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT status,progress_current FROM learning_generation_jobs WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        if row.get::<String, _>("status") != "pending" {
            tx.commit().await.map_err(db)?;
            return Ok(false);
        }
        let stamp = now();
        sqlx::query("UPDATE learning_generation_jobs SET status='running',started_at=?,finished_at=NULL,progress_message='Starting',heartbeat_at=? WHERE id=? AND status='pending'").bind(stamp).bind(stamp).bind(id).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) SELECT ?,coalesce(max(ordinal),-1)+1,'running',?,'Starting',? FROM learning_generation_job_events WHERE job_id=?").bind(id).bind(row.get::<i64,_>("progress_current")).bind(stamp).bind(id).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    pub(in crate::features::learning) async fn lesson_draft(
        &self,
        job_id: &str,
        lesson_id: &str,
    ) -> Result<Option<crate::features::learning::lesson_drafts::Draft>> {
        let checkpoint: Option<String> = sqlx::query_scalar("SELECT staged_result_json FROM learning_generation_jobs WHERE id=? AND kind='lesson_preparation'")
            .bind(job_id).fetch_one(&self.pool).await.map_err(db)?;
        let mut drafts: crate::features::learning::lesson_drafts::Drafts = checkpoint
            .as_deref()
            .map(decode)
            .transpose()?
            .unwrap_or_default();
        Ok(drafts.drafts.remove(lesson_id))
    }

    pub(in crate::features::learning) async fn save_lesson_draft(
        &self,
        job_id: &str,
        lesson_id: &str,
        draft: crate::features::learning::lesson_drafts::Draft,
    ) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let row = sqlx::query("SELECT staged_result_json,status FROM learning_generation_jobs WHERE id=? AND kind='lesson_preparation'")
            .bind(job_id).fetch_one(&mut *tx).await.map_err(db)?;
        if row.get::<String, _>("status") != "running" {
            return Err(AppError::InvalidState(
                "Lesson preparation is no longer running.".into(),
            ));
        }
        let checkpoint: Option<String> = row.get("staged_result_json");
        let mut drafts: crate::features::learning::lesson_drafts::Drafts = checkpoint
            .as_deref()
            .map(decode)
            .transpose()?
            .unwrap_or_default();
        drafts.drafts.insert(lesson_id.into(), draft);
        sqlx::query("UPDATE learning_generation_jobs SET staged_result_json=? WHERE id=? AND status='running'")
            .bind(encode(&drafts)?).bind(job_id).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn advance_job(&self, id: &str, completed: u32, message: &str) -> Result<bool> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let row=sqlx::query("SELECT status,progress_current,progress_total FROM learning_generation_jobs WHERE id=?").bind(id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Generation job not found".into()))?;
        if row.get::<String, _>("status") != "running" {
            tx.commit().await.map_err(db)?;
            return Ok(false);
        }
        let current = row.get::<i64, _>("progress_current") as u32;
        let total = row.get::<i64, _>("progress_total") as u32;
        if completed < current || completed > total {
            return Err(AppError::InvalidInput(
                "Generation progress must advance monotonically within its bound.".into(),
            ));
        }
        let stamp = now();
        sqlx::query("UPDATE learning_generation_jobs SET progress_current=?,progress_message=?,heartbeat_at=? WHERE id=? AND status='running'").bind(completed as i64).bind(message).bind(stamp).bind(id).execute(&mut *tx).await.map_err(db)?;
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,?,'running',?,?,?)").bind(id).bind(ordinal).bind(completed as i64).bind(message).bind(stamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    pub async fn fail_job(&self, id: &str, message: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT status,progress_current FROM learning_generation_jobs WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        if row.get::<String, _>("status") != "running" {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        let stamp = now();
        sqlx::query("UPDATE learning_generation_jobs SET status='failed',finished_at=?,error_code='generation_failed',error_message=?,progress_message='Generation failed',published_result_id=NULL WHERE id=?").bind(stamp).bind(message.chars().take(2000).collect::<String>()).bind(id).execute(&mut *tx).await.map_err(db)?;
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,?,'failed',?,?,?)").bind(id).bind(ordinal).bind(row.get::<i64,_>("progress_current")).bind("Generation failed").bind(stamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn interrupt_job(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT status,progress_current FROM learning_generation_jobs WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        if row.get::<String, _>("status") != "running" {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        let stamp = now();
        sqlx::query("UPDATE learning_generation_jobs SET status='interrupted',finished_at=?,error_code='generation_interrupted',error_message=?,progress_message='Generation interrupted',published_result_id=NULL WHERE id=?").bind(stamp).bind("Application shut down during generation").bind(id).execute(&mut *tx).await.map_err(db)?;
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,?,'interrupted',?,?,?)").bind(id).bind(ordinal).bind(row.get::<i64,_>("progress_current")).bind("Generation interrupted").bind(stamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn publish_prepared_lessons(
        &self,
        id: &str,
        expected_revision: i64,
        lessons: &[(
            String,
            crate::features::learning::dto::PreparedLearningLesson,
        )],
    ) -> Result<()> {
        if lessons.is_empty() || lessons.len() > 3 {
            return Err(AppError::InvalidInput(
                "A generation job may publish one to three prepared lessons.".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        let job = sqlx::query(
            "SELECT program_id,status,progress_total FROM learning_generation_jobs WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        if job.get::<String, _>("status") != "running" {
            return Err(AppError::InvalidState(
                "Cancelled or finished jobs cannot publish staged lessons.".into(),
            ));
        }
        let program_id: String = job.get("program_id");
        if lessons.len() as i64 != job.get::<i64, _>("progress_total") {
            return Err(AppError::InvalidInput(
                "Prepared lesson count must match the bounded job size.".into(),
            ));
        }
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(&program_id).bind(expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidState(
                "Program changed before generation could publish.".into(),
            ));
        }
        let mut unique = std::collections::HashSet::new();
        for (lesson_id, prepared) in lessons {
            if !unique.insert(lesson_id) {
                return Err(AppError::InvalidInput(
                    "A preparation job cannot repeat lesson IDs.".into(),
                ));
            }
            let row =
                sqlx::query("SELECT preparation FROM learning_lessons WHERE id=? AND program_id=?")
                    .bind(lesson_id)
                    .bind(&program_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?
                    .ok_or_else(|| AppError::NotFound("Lesson not found in this program".into()))?;
            if row.get::<String, _>("preparation") != "outline" {
                return Err(AppError::InvalidState(
                    "Only outline lessons can be published by this job.".into(),
                ));
            }
            crate::features::learning::content_verification::persist_report(
                &mut tx,
                &program_id,
                lesson_id,
                prepared,
            )
            .await?;
            for (ordinal, b) in prepared.blocks.iter().enumerate() {
                sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)").bind(lesson_id).bind(ordinal as i64).bind(crate::features::learning::repository::block_kind(b.kind.clone())).bind(&b.title).bind(&b.body).bind(encode(&b.source_ids)?).bind(encode(&b.rubric)?).execute(&mut *tx).await.map_err(db)?;
            }
            for (ordinal, q) in prepared.questions.iter().enumerate() {
                sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) SELECT ?,?,module_id,?,?,?,?,? FROM learning_lessons WHERE id=?").bind(&q.id).bind(lesson_id).bind(crate::features::learning::repository::assessment(q.kind.clone())).bind(&q.prompt).bind(encode(&q.options)?).bind(encode(&q.source_ids)?).bind(ordinal as i64).bind(lesson_id).execute(&mut *tx).await.map_err(db)?;
            }
            for key in &prepared.keys {
                sqlx::query("INSERT INTO learning_answer_keys(question_id,correct_index,explanation) VALUES(?,?,?)").bind(&key.question_id).bind(key.correct_index as i64).bind(&key.explanation).execute(&mut *tx).await.map_err(db)?;
            }
            sqlx::query("UPDATE learning_lessons SET preparation='ready',curriculum_state='ready' WHERE id=? AND program_id=?").bind(lesson_id).bind(&program_id).execute(&mut *tx).await.map_err(db)?;
        }
        // Publish a new immutable accepted snapshot together with the prepared
        // lesson records and the program revision. The old accepted snapshot
        // remains historical; readers never observe a half-prepared plan.
        let accepted_row = sqlx::query("SELECT id,snapshot_json,required_lesson_count,resume_lesson_id FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(&program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;
        let predecessor: String = accepted_row.get("id");
        let mut snapshot: LearningCurriculumRevision = decode(accepted_row.get("snapshot_json"))?;
        let mut found = std::collections::HashSet::new();
        for (lesson_id, _) in lessons {
            for lesson in snapshot
                .modules
                .iter_mut()
                .flat_map(|module| module.lessons.iter_mut())
            {
                if lesson.id == *lesson_id {
                    lesson.state = LearningCurriculumLessonState::Ready;
                    found.insert(lesson_id.as_str());
                }
            }
        }
        if found.len() != lessons.len() {
            return Err(AppError::InvalidState(
                "Prepared lessons must belong to the accepted curriculum snapshot.".into(),
            ));
        }
        snapshot.id = uuid::Uuid::new_v4().to_string();
        snapshot.revision_number = snapshot.revision_number.saturating_add(1);
        snapshot.parent_revision_id = Some(predecessor.clone());
        snapshot.reason = "Published prepared lesson material".into();
        snapshot.created_at = now();
        curriculum::validate_curriculum(&snapshot)?;
        let snapshot_json = encode(&snapshot)?;
        let snapshot_hash = hash(&snapshot_json);
        let required = snapshot
            .modules
            .iter()
            .flat_map(|module| &module.lessons)
            .filter(|lesson| {
                !matches!(
                    lesson.state,
                    LearningCurriculumLessonState::Skipped
                        | LearningCurriculumLessonState::Replaced
                        | LearningCurriculumLessonState::Challenged
                )
            })
            .count() as i64;
        let resume: Option<String> = accepted_row.get("resume_lesson_id");
        sqlx::query("UPDATE learning_curriculum_revisions SET status='superseded' WHERE id=? AND status='accepted'")
            .bind(&predecessor).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_curriculum_revisions(id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,required_lesson_count,resume_lesson_id,created_at,accepted_at) VALUES(?,?,?,?,'accepted',?,?,?,?,?,?,?)")
            .bind(&snapshot.id).bind(&program_id).bind(snapshot.revision_number as i64).bind(&predecessor)
            .bind(&snapshot.reason).bind(snapshot_json).bind(snapshot_hash).bind(required).bind(resume)
            .bind(snapshot.created_at).bind(snapshot.created_at).execute(&mut *tx).await.map_err(db)?;
        let stamp = now();
        let total = lessons.len() as i64;
        sqlx::query("UPDATE learning_generation_jobs SET status='completed',progress_current=?,progress_message='Published prepared lessons',published_result_id=?,finished_at=?,heartbeat_at=?,error_code=NULL,error_message=NULL WHERE id=? AND status='running'").bind(total).bind(&program_id).bind(stamp).bind(stamp).bind(id).execute(&mut *tx).await.map_err(db)?;
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,?,'completed',?,'Published prepared lessons',?)").bind(id).bind(ordinal).bind(total).bind(stamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn recover_running_jobs(&self) -> Result<()> {
        // Startup also starts other database writers. Reserve the write lock
        // before reading jobs: upgrading a deferred WAL read transaction can
        // fail immediately with SQLITE_BUSY and prevent the app from opening.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let rows = sqlx::query(
            "SELECT id,progress_current FROM learning_generation_jobs WHERE status='running'",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let stamp = now();
        for row in rows {
            let id: String = row.get("id");
            let current: i64 = row.get("progress_current");
            sqlx::query("UPDATE learning_generation_jobs SET status='interrupted',published_result_id=NULL,error_code='interrupted',error_message='Generation stopped before publishing a result.',progress_message='Interrupted during restart',finished_at=?,heartbeat_at=? WHERE id=? AND status='running'").bind(now()).bind(now()).bind(&id).execute(&mut *tx).await.map_err(db)?;
            let ordinal:i64=sqlx::query_scalar("SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=?").bind(&id).fetch_one(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) VALUES(?,?,'interrupted',?,'Interrupted during restart',?)").bind(id).bind(ordinal).bind(current).bind(stamp).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)
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

fn parse_job(r: sqlx::sqlite::SqliteRow) -> Result<LearningGenerationJob> {
    use LearningGenerationJobKind as K;
    use LearningGenerationJobStatus as S;
    let kind = match r.get::<String, _>("kind").as_str() {
        "program" => K::ProgramOutline,
        "diagnostic" => K::AdaptiveFollowUp,
        "curriculum_revision" => K::ProgramOutline,
        "lesson_preparation" => K::LessonPreparation,
        "assessment" => K::AssessmentVariant,
        "practical" => K::PracticalActivity,
        _ => return Err(AppError::Database("Invalid generation job kind".into())),
    };
    let status = match r.get::<String, _>("status").as_str() {
        "pending" => S::Pending,
        "running" => S::Running,
        "completed" => S::Completed,
        "failed" => S::Failed,
        "cancelled" => S::Cancelled,
        "interrupted" => S::Interrupted,
        _ => return Err(AppError::Database("Invalid generation job status".into())),
    };
    let job = LearningGenerationJob {
        id: r.get("id"),
        program_id: r.get("program_id"),
        operation_id: r.get("operation_id"),
        kind,
        payload_sha256: r.get("payload_hash"),
        base_revision_number: r.get::<i64, _>("base_revision_number") as u32,
        status,
        progress_completed: r.get::<i64, _>("progress_current") as u32,
        progress_total: r.get::<i64, _>("progress_total") as u32,
        progress_message: r.get("progress_message"),
        result_id: r.get("published_result_id"),
        error: r.get("error_message"),
        retry_of_job_id: r.get("retry_of_job_id"),
        created_at: r.get("created_at"),
        started_at: r.get("started_at"),
        finished_at: r.get("finished_at"),
    };
    curriculum::validate_generation_job(&job)?;
    Ok(job)
}

fn job_kind_name(kind: LearningGenerationJobKind) -> &'static str {
    match kind {
        LearningGenerationJobKind::ProgramOutline => "program",
        LearningGenerationJobKind::LessonPreparation => "lesson_preparation",
        LearningGenerationJobKind::AssessmentVariant => "assessment",
        LearningGenerationJobKind::AdaptiveFollowUp => "diagnostic",
        LearningGenerationJobKind::PracticalActivity => "practical",
    }
}

fn decode_diagnostic_row(r: &sqlx::sqlite::SqliteRow) -> Result<LearningDiagnosticAttemptDto> {
    let status = match r.get::<String, _>("status").as_str() {
        "active" => LearningDiagnosticStatus::Active,
        "submitted" => LearningDiagnosticStatus::Submitted,
        "skipped" => LearningDiagnosticStatus::Skipped,
        _ => return Err(AppError::Database("Invalid diagnostic status".into())),
    };
    Ok(LearningDiagnosticAttemptDto {
        id: r.get("id"),
        program_id: r.get("program_id"),
        status,
        revision: r.get("revision"),
        prompts: decode(r.get("prompt_json"))?,
        responses: decode(r.get("response_json"))?,
        source_coverage_gaps: decode(r.get("source_coverage_gaps_json"))?,
        findings: decode(r.get("findings_json"))?,
        interpretation: if r.get::<String, _>("evaluation_json") == "[]" {
            "Self-inventory only. Responses are not scored and do not establish mastery.".into()
        } else {
            "Starting-point feedback is provisional. Use it to choose practice or a challenge; it does not establish mastery.".into()
        },
        created_at: r.get("created_at"),
        submitted_at: r.get("submitted_at"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::{
        curriculum::{
            LearningCurriculumLesson, LearningCurriculumLessonState, LearningCurriculumOperation,
            LearningGenerationJobKind, LearningGenerationJobStatus,
        },
        dto::{AcceptLearningProgramRequestDto, LearningProgramDto},
        repository::LearningRepository,
        tests::{fixture, pool},
    };

    async fn active_program(pool: &SqlitePool) -> Result<LearningProgramDto> {
        let repo = LearningRepository::new(pool.clone());
        let program = fixture();
        repo.create(&program).await?;
        repo.accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
        repo.get(&program.summary.id).await
    }

    fn op_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn new_lesson() -> LearningCurriculumLesson {
        LearningCurriculumLesson {
            id: uuid::Uuid::new_v4().to_string(),
            title: "A new neutral lesson".into(),
            objective: "Explain one relevant idea".into(),
            estimated_minutes: 20,
            state: LearningCurriculumLessonState::Outline,
            assessment_started: false,
            replacement_lesson_id: None,
        }
    }

    #[tokio::test]
    async fn real_migration_seeds_uuid_outcomes_and_preview_is_replayable_and_cas_guarded(
    ) -> Result<()> {
        let pool = pool().await?;
        let program = active_program(&pool).await?;
        let repo = LearningCurriculumRepository::new(pool.clone());
        let plan = repo.plan(&program.summary.id).await?;
        let accepted = plan.accepted_revision.expect("seed snapshot");
        let outcome_id = accepted.modules[0].outcome_ids[0].clone();
        assert!(uuid::Uuid::parse_str(&outcome_id).is_ok());
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM learning_outcome_definitions WHERE id=? AND program_id=?",
        )
        .bind(&outcome_id)
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
        assert_eq!(exists, 1);

        let added = new_lesson();
        let request = PreviewLearningCurriculumRevisionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            reason: "Adjust upcoming practice".into(),
            operations: vec![LearningCurriculumOperation::AddLesson {
                module_id: accepted.modules[0].id.clone(),
                after_lesson_id: None,
                lesson: added.clone(),
            }],
        };
        let preview = repo.preview(&request).await?;
        let draft = preview.draft_revision.expect("draft snapshot");
        assert_eq!(
            preview.required_lesson_count_after,
            preview.required_lesson_count_before + 1
        );
        assert!(draft.modules[0]
            .lessons
            .iter()
            .any(|lesson| lesson.id == added.id));
        let replay = repo.preview(&request).await?;
        assert_eq!(replay.draft_revision.unwrap().id, draft.id);

        let mut changed_payload = request.clone();
        changed_payload.reason.push_str(" changed");
        assert!(repo.preview(&changed_payload).await.is_err());
        let mut stale = request;
        stale.operation_id = op_id();
        stale.expected_revision -= 1;
        assert!(repo.preview(&stale).await.is_err());

        let accept = LearningCurriculumRevisionActionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            revision_id: draft.id.clone(),
            expected_revision: program.summary.revision,
        };
        let accepted_plan = repo.accept_revision(&accept).await?;
        assert_eq!(accepted_plan.accepted_revision.unwrap().id, draft.id);
        let revision_after_accept: i64 =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(&program.summary.id)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert_eq!(revision_after_accept, program.summary.revision + 1);
        assert_eq!(
            repo.accept_revision(&accept)
                .await?
                .accepted_revision
                .unwrap()
                .id,
            draft.id
        );
        let revision_after_replay: i64 =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(&program.summary.id)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert_eq!(revision_after_replay, revision_after_accept);
        let stored_snapshot: String = sqlx::query_scalar(
            "SELECT snapshot_json FROM learning_curriculum_revisions WHERE id=?",
        )
        .bind(&draft.id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
        assert_eq!(
            serde_json::from_str::<LearningCurriculumRevision>(&stored_snapshot)
                .map_err(|e| AppError::Serialization(e.to_string()))?,
            draft
        );
        let accepted_at: Option<i64> =
            sqlx::query_scalar("SELECT accepted_at FROM learning_curriculum_revisions WHERE id=?")
                .bind(&draft.id)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert!(
            accepted_at.is_some(),
            "acceptance time survives revision history changes"
        );
        Ok(())
    }

    #[tokio::test]
    async fn diagnostic_lifecycle_is_idempotent_complete_and_does_not_claim_mastery() -> Result<()>
    {
        let pool = pool().await?;
        let program = active_program(&pool).await?;
        let repo = LearningCurriculumRepository::new(pool.clone());
        let start = StartLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
        };
        let active = repo.start_diagnostic(&start).await?;
        assert_eq!(active.status, LearningDiagnosticStatus::Active);
        assert!(active
            .prompts
            .iter()
            .all(|prompt| prompt.prompt.starts_with("Self-inventory:")));
        for prompt in &active.prompts {
            let durable: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_outcome_definitions WHERE program_id=? AND id=? AND title=?")
                .bind(&program.summary.id).bind(&prompt.outcome_id).bind(&prompt.outcome_title)
                .fetch_one(&pool).await.map_err(db)?;
            assert_eq!(
                durable, 1,
                "diagnostic outcome ID and title must come from saved definitions"
            );
        }
        assert_eq!(repo.start_diagnostic(&start).await?.id, active.id);
        let mut changed_start = start.clone();
        changed_start.expected_revision += 1;
        assert!(repo.start_diagnostic(&changed_start).await.is_err());

        let mut incomplete = SubmitLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            diagnostic_id: active.id.clone(),
            expected_revision: program.summary.revision,
            responses: vec![],
            save_only: false,
            expected_diagnostic_revision: None,
        };
        assert!(repo.submit_diagnostic(&incomplete).await.is_err());
        incomplete.responses = active
            .prompts
            .iter()
            .map(|prompt| LearningDiagnosticResponseDto {
                prompt_id: prompt.id.clone(),
                response: "I have some familiarity and a question to explore.".into(),
            })
            .collect();
        let submitted = repo.submit_diagnostic(&incomplete).await?;
        assert_eq!(submitted.status, LearningDiagnosticStatus::Submitted);
        assert_eq!(submitted.source_coverage_gaps.len(), active.prompts.len());
        assert!(submitted
            .interpretation
            .contains("do not establish mastery"));
        assert_eq!(repo.submit_diagnostic(&incomplete).await?.id, active.id);
        let mut changed_answer = incomplete.clone();
        changed_answer.responses[0].response.push_str(" changed");
        assert!(repo.submit_diagnostic(&changed_answer).await.is_err());

        let skipped = repo
            .skip_diagnostic(&SkipLearningDiagnosticRequestDto {
                operation_id: op_id(),
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
            })
            .await?;
        assert_eq!(skipped.status, LearningDiagnosticStatus::Skipped);
        Ok(())
    }

    #[tokio::test]
    async fn completed_and_assessed_lessons_are_protected_by_current_outcome_joins() -> Result<()> {
        let pool = pool().await?;
        let program = active_program(&pool).await?;
        let repo = LearningCurriculumRepository::new(pool.clone());
        let accepted = repo
            .plan(&program.summary.id)
            .await?
            .accepted_revision
            .expect("accepted snapshot");
        let completed_lesson = accepted.modules[1].lessons[0].id.clone();
        sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
            .bind(&completed_lesson)
            .execute(&pool)
            .await
            .map_err(db)?;

        let module = &accepted.modules[0];
        let assessed_lesson = module.lessons[0].id.clone();
        let outcome_id = module.outcome_ids[0].clone();
        let blueprint_id = uuid::Uuid::new_v4().to_string();
        let candidate_id = uuid::Uuid::new_v4().to_string();
        let form_id = uuid::Uuid::new_v4().to_string();
        let stamp = now();
        sqlx::query("INSERT INTO learning_assessment_blueprints(id,program_id,revision,predecessor_revision,purpose,title,instructions,expected_minutes,allowed_aids_json,passing_score,rubric_json,feedback_timing,source_version_ids_json,status,change_reason,created_at) VALUES(?,?,1,NULL,'practice','A neutral assessment','Respond to the prompts.',20,'[]',0.6,'[]','immediate','[]','accepted','Initial',?)")
            .bind(&blueprint_id).bind(&program.summary.id).bind(stamp).execute(&pool).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at) VALUES(?,?,?,'multiple_choice',1,'A neutral prompt','[\"A\",\"B\"]',NULL,'[]','[]','person',NULL,?,?)")
            .bind(&candidate_id).bind(&program.summary.id).bind(&outcome_id).bind(uuid::Uuid::new_v4().to_string()).bind(stamp).execute(&pool).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) VALUES(?,?)")
            .bind(&candidate_id).bind(&outcome_id).execute(&pool).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_assessment_forms(id,program_id,blueprint_id,blueprint_revision,retake_of_form_id,status,revision,title,instructions,expected_minutes,allowed_aids_json,purpose,passing_score,rubric_json,feedback_timing,source_version_ids_json,model_name,created_at,updated_at,submitted_at) VALUES(?,?,?,1,NULL,'active',0,'A neutral form','Respond to the prompts.',20,'[]','practice',0.6,'[]','immediate','[]',NULL,?,?,NULL)")
            .bind(&form_id).bind(&program.summary.id).bind(&blueprint_id).bind(stamp).bind(stamp).execute(&pool).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_assessment_form_items(form_id,ordinal,candidate_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,points,previously_exposed) VALUES(?,0,?,?,'multiple_choice',1,'A neutral prompt','[\"A\",\"B\"]',NULL,'[]',1,0)")
            .bind(&form_id).bind(&candidate_id).bind(&outcome_id).execute(&pool).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_assessment_form_item_outcomes(form_id,item_ordinal,outcome_id) VALUES(?,0,?)")
            .bind(&form_id).bind(&outcome_id).execute(&pool).await.map_err(db)?;

        let rejected_completed = repo
            .preview(&PreviewLearningCurriculumRevisionRequestDto {
                operation_id: op_id(),
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
                reason: "Try changing completed work".into(),
                operations: vec![LearningCurriculumOperation::SkipLesson {
                    lesson_id: completed_lesson,
                }],
            })
            .await;
        assert!(rejected_completed.is_err());
        let rejected_assessed = repo
            .preview(&PreviewLearningCurriculumRevisionRequestDto {
                operation_id: op_id(),
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
                reason: "Try changing assessed module work".into(),
                operations: vec![LearningCurriculumOperation::EditLesson {
                    lesson_id: assessed_lesson,
                    title: "Changed after assessment start".into(),
                    objective: "A changed objective".into(),
                    estimated_minutes: 25,
                }],
            })
            .await;
        assert!(rejected_assessed.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn selected_lesson_jobs_allow_placement_and_retry_failures_without_duplicate_workers(
    ) -> Result<()> {
        let pool = pool().await?;
        let program = active_program(&pool).await?;
        let repo = LearningCurriculumRepository::new(pool.clone());
        let target = program.modules[1].lessons[1].id.clone();
        let request = StartLearningGenerationJobRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":[target]}).to_string(),
            progress_total: 1,
        };
        let first = repo.start_lesson_job(&request).await?;
        assert_eq!(repo.start_lesson_job(&request).await?.id, first.id);
        assert!(repo.begin_job(&first.id).await?);
        repo.fail_job(&first.id, "Synthetic review failure").await?;
        let (left, right) = tokio::join!(
            repo.start_lesson_job(&request),
            repo.start_lesson_job(&request)
        );
        let retry = left?;
        assert_eq!(retry.id, right?.id);
        assert_ne!(retry.id, first.id);
        assert_eq!(retry.retry_of_job_id.as_deref(), Some(first.id.as_str()));
        assert!(repo.begin_job(&retry.id).await?);
        assert!(!repo.begin_job(&retry.id).await?);
        repo.fail_job(&retry.id, "A second synthetic failure")
            .await?;
        let next = repo.start_lesson_job(&request).await?;
        assert_eq!(next.retry_of_job_id.as_deref(), Some(retry.id.as_str()));
        assert_eq!(repo.start_lesson_job(&request).await?.id, next.id);
        let mut invalid = request.clone();
        invalid.operation_id = op_id();
        invalid.request_json = serde_json::json!({"lessonIds":[op_id()]}).to_string();
        assert!(repo.start_lesson_job(&invalid).await.is_err());
        let mut batch = request;
        batch.operation_id = op_id();
        batch.progress_total = 2;
        batch.request_json=serde_json::json!({"lessonIds":[program.modules[1].lessons[1].id,program.modules[0].lessons[0].id]}).to_string();
        assert!(repo.start_job(&batch).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn startup_recovery_waits_for_concurrent_writer_before_reading_jobs() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(directory.path().join("startup-recovery.sqlite"))
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .map_err(db)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
        let program = active_program(&pool).await?;
        let repo = LearningCurriculumRepository::new(pool.clone());
        let job = repo
            .start_job(&StartLearningGenerationJobRequestDto {
                operation_id: op_id(),
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
                kind: LearningGenerationJobKind::LessonPreparation,
                request_json: serde_json::json!({"lessonIds":[program.modules[0].lessons[0].id]})
                    .to_string(),
                progress_total: 1,
            })
            .await?;
        assert!(repo.begin_job(&job.id).await?);
        let mut writer = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        sqlx::query("UPDATE learning_generation_jobs SET progress_message='Concurrent startup write' WHERE id=?")
            .bind(&job.id).execute(&mut *writer).await.map_err(db)?;
        let recovery = repo.recover_running_jobs();
        tokio::pin!(recovery);
        tokio::select! {
            result = &mut recovery => panic!("Recovery must wait for the writer, not abort app startup: {result:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
        }
        writer.commit().await.map_err(db)?;
        recovery.await?;
        let recovered = repo.job(&job.id).await?;
        assert_eq!(recovered.status, LearningGenerationJobStatus::Interrupted);
        assert!(recovered.result_id.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn job_restart_recovery_retry_and_publication_are_durable_and_atomic() -> Result<()> {
        let pool = pool().await?;
        let program = active_program(&pool).await?;
        let curriculum = LearningCurriculumRepository::new(pool.clone());
        let stored = curriculum.plan(&program.summary.id).await?;
        let accepted = stored.accepted_revision.expect("accepted");
        let target = accepted.modules[0].lessons[0].id.clone();
        let request = StartLearningGenerationJobRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":[target]}).to_string(),
            progress_total: 1,
        };
        let job = curriculum.start_job(&request).await?;
        assert_eq!(curriculum.start_job(&request).await?.id, job.id);
        let mut mismatch = request.clone();
        mismatch.request_json = "{\"lessonIds\":[]}".into();
        assert!(curriculum.start_job(&mismatch).await.is_err());
        assert!(curriculum.begin_job(&job.id).await?);
        let draft_body = "{\"blocks\":[{\"body\":\"Unpublished lesson draft\"}]}";
        crate::features::learning::lesson_drafts::run(&curriculum, &job.id, &target, async {
            crate::features::learning::lesson_drafts::authoring(
                crate::features::learning::lesson_drafts::AuthoringContext {
                    scope_hash: "same-lesson-request".into(),
                    prompt: "Original authoring prompt and citation order".into(),
                    sources: program.sources.clone(),
                    reference_hashes: program
                        .sources
                        .iter()
                        .map(|source| {
                            (
                                source.id.clone(),
                                crate::features::learning::content_verification::digest(
                                    &source.excerpt,
                                ),
                            )
                        })
                        .collect(),
                },
            );
            assert!(
                crate::features::learning::lesson_drafts::resume("matching-inputs".into())
                    .await?
                    .is_none()
            );
            crate::features::learning::lesson_drafts::save(draft_body).await?;
            crate::features::learning::lesson_drafts::record_teaching(
                draft_body,
                "teaching-policy".into(),
            )
            .await?;
            crate::features::learning::lesson_drafts::record_claim_inventory(
                draft_body,
                "audited-inventory".into(),
            )
            .await?;
            crate::features::learning::lesson_drafts::record_pending_repair(
                draft_body,
                "failed-checks-to-repair".into(),
            )
            .await?;
            assert!(
                crate::features::learning::lesson_drafts::teaching_completed(
                    draft_body,
                    "teaching-policy"
                )
                .await?
            );
            Ok(())
        })
        .await?;
        assert_eq!(
            curriculum
                .lesson_draft(&job.id, &target)
                .await?
                .unwrap()
                .candidate,
            draft_body
        );
        assert!(LearningRepository::new(pool.clone())
            .get(&program.summary.id)
            .await?
            .modules[0]
            .lessons[0]
            .blocks
            .is_empty());
        curriculum
            .advance_job(&job.id, 1, "Prepared one lesson")
            .await?;

        // Process death must retain the unpublished draft. Retry copies it to a
        // child job, but a teaching receipt never grants publication approval.
        curriculum.recover_running_jobs().await?;
        let interrupted = curriculum.job(&job.id).await?;
        assert_eq!(interrupted.status, LearningGenerationJobStatus::Interrupted);
        assert!(interrupted.finished_at.is_some());
        let retry_request = LearningGenerationJobActionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            job_id: job.id.clone(),
            expected_revision: program.summary.revision,
        };
        let retry = curriculum.retry_job(&retry_request).await?;
        assert_eq!(retry.retry_of_job_id.as_deref(), Some(job.id.as_str()));
        assert_eq!(curriculum.retry_job(&retry_request).await?.id, retry.id);
        let mut replay_mismatch = retry_request.clone();
        replay_mismatch.job_id = retry.id.clone();
        assert!(curriculum.retry_job(&replay_mismatch).await.is_err());

        assert!(curriculum.begin_job(&retry.id).await?);
        let reopened = LearningCurriculumRepository::new(pool.clone());
        crate::features::learning::lesson_drafts::run(&reopened, &retry.id, &target, async {
            let mut augmented = program.sources.clone();
            let mut additional = augmented[0].clone();
            additional.id = uuid::Uuid::new_v4().to_string();
            additional.excerpt = "Additional research never grants publication approval.".into();
            augmented.push(additional);
            let (input_hash, authoring) =
                crate::features::learning::lesson_drafts::reusable_authoring(
                    "same-lesson-request",
                    &augmented,
                )
                .await?
                .expect("additional research retains the authoring context");
            assert_eq!(input_hash, "matching-inputs");
            assert_eq!(
                authoring.prompt,
                "Original authoring prompt and citation order"
            );
            assert_eq!(authoring.sources.len(), program.sources.len());
            assert_eq!(authoring.sources[0].id, program.sources[0].id);
            assert!(
                crate::features::learning::lesson_drafts::reusable_authoring(
                    "different-lesson-request",
                    &augmented
                )
                .await?
                .is_none()
            );
            augmented[0]
                .excerpt
                .push_str(" Changed original reference.");
            assert!(
                crate::features::learning::lesson_drafts::reusable_authoring(
                    "same-lesson-request",
                    &augmented
                )
                .await?
                .is_none()
            );
            assert!(
                crate::features::learning::lesson_drafts::reusable_authoring(
                    "same-lesson-request",
                    &[]
                )
                .await?
                .is_none()
            );
            crate::features::learning::lesson_drafts::authoring(authoring);
            assert_eq!(
                crate::features::learning::lesson_drafts::resume("matching-inputs".into())
                    .await?
                    .as_deref(),
                Some(draft_body)
            );
            let formatted = serde_json::to_string_pretty(&serde_json::from_str::<
                serde_json::Value,
            >(draft_body)?)?;
            assert_eq!(
                crate::features::learning::lesson_drafts::pending_repair(&formatted)
                    .await?
                    .as_deref(),
                Some("failed-checks-to-repair")
            );
            assert_eq!(
                crate::features::learning::lesson_drafts::claim_inventory(&formatted)
                    .await?
                    .as_deref(),
                Some("audited-inventory")
            );
            assert!(
                crate::features::learning::lesson_drafts::claim_inventory("changed draft")
                    .await?
                    .is_none()
            );
            assert!(
                crate::features::learning::lesson_drafts::teaching_completed(
                    draft_body,
                    "teaching-policy"
                )
                .await?
            );
            assert!(
                !crate::features::learning::lesson_drafts::teaching_completed(
                    draft_body,
                    "new-policy"
                )
                .await?
            );
            assert!(
                !crate::features::learning::lesson_drafts::teaching_completed(
                    "changed draft",
                    "teaching-policy"
                )
                .await?
            );
            assert!(crate::features::learning::lesson_drafts::resume(
                "changed-source-or-prompt".into()
            )
            .await?
            .is_none());
            assert!(
                !crate::features::learning::lesson_drafts::teaching_completed(
                    draft_body,
                    "teaching-policy"
                )
                .await?
            );
            crate::features::learning::lesson_drafts::resume("matching-inputs".into()).await?;
            crate::features::learning::lesson_drafts::save("changed draft").await?;
            crate::features::learning::lesson_drafts::save(draft_body).await?;
            assert!(
                crate::features::learning::lesson_drafts::claim_inventory(draft_body)
                    .await?
                    .is_none()
            );
            assert!(
                crate::features::learning::lesson_drafts::pending_repair(draft_body)
                    .await?
                    .is_none()
            );
            assert!(
                !crate::features::learning::lesson_drafts::teaching_completed(
                    draft_body,
                    "teaching-policy"
                )
                .await?
            );
            Ok(())
        })
        .await?;
        assert!(reopened
            .save_lesson_draft(
                &job.id,
                &target,
                crate::features::learning::lesson_drafts::Draft {
                    input_hash: "matching-inputs".into(),
                    candidate: "should not overwrite an interrupted job".into(),
                    teaching_receipt: None,
                    authoring: None,
                    claim_inventory: None,
                    pending_repair: None,
                }
            )
            .await
            .is_err());
        let before_revision: i64 =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(&program.summary.id)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        let module_id: String =
            sqlx::query_scalar("SELECT module_id FROM learning_lessons WHERE id=?")
                .bind(&target)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        let duplicate_question_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) VALUES(?,?,?,'practice','Existing item','[]','[]',0)")
            .bind(&duplicate_question_id).bind(&target).bind(&module_id).execute(&pool).await.map_err(db)?;
        let mut broken = crate::features::learning::dto::PreparedLearningLesson {
            verification: None,
            blocks: vec![crate::features::learning::dto::LearningBlockDto {
                rubric: vec![],
                kind: crate::features::learning::dto::LearningBlockKind::Explanation,
                title: "A staged explanation".into(),
                body: "This should roll back with the transaction.".into(),
                source_ids: vec![],
            }],
            questions: vec![crate::features::learning::dto::LearningQuestionDto {
                id: duplicate_question_id,
                kind: crate::features::learning::dto::LearningAssessmentKind::Practice,
                prompt: "A conflicting item".into(),
                options: vec!["One".into(), "Two".into()],
                source_ids: vec![],
            }],
            keys: vec![],
        };
        broken.verification =
            Some(crate::features::learning::content_verification::tests::attest(&target, &broken));
        assert!(curriculum
            .publish_prepared_lessons(&retry.id, before_revision, &[(target.clone(), broken)])
            .await
            .is_err());
        let preparation: String =
            sqlx::query_scalar("SELECT preparation FROM learning_lessons WHERE id=?")
                .bind(&target)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert_eq!(preparation, "outline");
        let block_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_blocks WHERE lesson_id=?")
                .bind(&target)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert_eq!(
            block_count, 0,
            "the staged block must roll back with the duplicate question"
        );
        let after_failure: i64 =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(&program.summary.id)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert_eq!(after_failure, before_revision);

        let mut good = crate::features::learning::dto::PreparedLearningLesson {
            verification: None,
            blocks: vec![crate::features::learning::dto::LearningBlockDto {
                kind: crate::features::learning::dto::LearningBlockKind::Explanation,
                title: "Verified teaching fixture".into(),
                body: "A persisted explanation.".into(),
                source_ids: vec![],
                rubric: vec![],
            }],
            questions: vec![],
            keys: vec![],
        };
        good.verification =
            Some(crate::features::learning::content_verification::tests::attest(&target, &good));
        curriculum
            .publish_prepared_lessons(&retry.id, before_revision, &[(target.clone(), good)])
            .await?;
        let completed = curriculum.job(&retry.id).await?;
        assert_eq!(completed.status, LearningGenerationJobStatus::Completed);
        assert!(completed.finished_at.is_some());
        let prepared: String =
            sqlx::query_scalar("SELECT preparation FROM learning_lessons WHERE id=?")
                .bind(&target)
                .fetch_one(&pool)
                .await
                .map_err(db)?;
        assert_eq!(prepared, "ready");
        let published_plan = curriculum.plan(&program.summary.id).await?;
        let published_snapshot = published_plan
            .accepted_revision
            .expect("published snapshot");
        assert_eq!(
            published_snapshot.revision_number,
            accepted.revision_number + 1
        );
        assert_eq!(
            published_snapshot.parent_revision_id.as_deref(),
            Some(accepted.id.as_str())
        );
        assert_eq!(
            published_snapshot.modules[0].lessons[0].state,
            LearningCurriculumLessonState::Ready
        );
        Ok(())
    }
}
