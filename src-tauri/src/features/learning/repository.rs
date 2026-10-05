//! SQL persistence for Learning Studio. Domain orchestration never issues SQL.
use super::dto::*;
use crate::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn json<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn decode<T: serde::de::DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_str(s).map_err(|e| AppError::Serialization(e.to_string()))
}
fn status(v: LearningProgramStatus) -> &'static str {
    match v {
        LearningProgramStatus::Draft => "draft",
        LearningProgramStatus::Active => "active",
    }
}
fn prep(v: LearningPreparation) -> &'static str {
    match v {
        LearningPreparation::Outline => "outline",
        LearningPreparation::Ready => "ready",
    }
}
pub(super) fn block_kind(v: LearningBlockKind) -> &'static str {
    match v {
        LearningBlockKind::Explanation => "explanation",
        LearningBlockKind::WorkedExample => "worked_example",
        LearningBlockKind::Reflection => "reflection",
        LearningBlockKind::GuidedPractice => "guided_practice",
        LearningBlockKind::IndependentPractice => "independent_practice",
        LearningBlockKind::Recap => "recap",
    }
}
pub(super) fn assessment(v: LearningAssessmentKind) -> &'static str {
    match v {
        LearningAssessmentKind::Practice => "practice",
        LearningAssessmentKind::Quiz => "quiz",
        LearningAssessmentKind::Test => "test",
    }
}
fn parse_status(s: &str) -> Result<LearningProgramStatus> {
    match s {
        "draft" => Ok(LearningProgramStatus::Draft),
        "active" => Ok(LearningProgramStatus::Active),
        _ => Err(AppError::Database("Invalid learning program status".into())),
    }
}
fn parse_prep(s: &str) -> Result<LearningPreparation> {
    match s {
        "outline" => Ok(LearningPreparation::Outline),
        "ready" => Ok(LearningPreparation::Ready),
        _ => Err(AppError::Database(
            "Invalid lesson preparation state".into(),
        )),
    }
}
fn parse_block(s: &str) -> Result<LearningBlockKind> {
    match s {
        "explanation" => Ok(LearningBlockKind::Explanation),
        "worked_example" => Ok(LearningBlockKind::WorkedExample),
        "reflection" => Ok(LearningBlockKind::Reflection),
        "guided_practice" => Ok(LearningBlockKind::GuidedPractice),
        "independent_practice" => Ok(LearningBlockKind::IndependentPractice),
        "recap" => Ok(LearningBlockKind::Recap),
        _ => Err(AppError::Database("Invalid learning block kind".into())),
    }
}
fn parse_assessment(s: &str) -> Result<LearningAssessmentKind> {
    match s {
        "practice" => Ok(LearningAssessmentKind::Practice),
        "quiz" => Ok(LearningAssessmentKind::Quiz),
        "test" => Ok(LearningAssessmentKind::Test),
        _ => Err(AppError::Database("Invalid assessment kind".into())),
    }
}

#[derive(Clone)]
pub struct LearningRepository {
    pub(super) pool: SqlitePool,
}

impl LearningRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn active_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        super::source_library::LearningSourceLibraryRepository::new(self.pool.clone())
            .prepare_sources(program_id)
            .await
    }

    pub async fn verification_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        super::source_library::LearningSourceLibraryRepository::new(self.pool.clone())
            .verification_sources(program_id)
            .await
    }

    pub async fn acquire_document_sources(
        &self,
        ids: &[String],
        goal: &str,
    ) -> Result<Vec<LearningSourceDto>> {
        use sqlx::{QueryBuilder, Sqlite};
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let terms: Vec<String> = goal
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 4)
            .take(8)
            .map(str::to_lowercase)
            .collect();
        let mut out = Vec::new();
        for id in ids {
            let mut q = QueryBuilder::<Sqlite>::new("SELECT c.id,c.document_id,d.file_name,c.content FROM text_chunks c JOIN documents d ON d.id=c.document_id WHERE c.document_id=");
            q.push_bind(id);
            if !terms.is_empty() {
                q.push(" AND (");
                let mut sep = q.separated(" OR ");
                for term in &terms {
                    sep.push("instr(lower(c.content), ")
                        .push_bind_unseparated(term)
                        .push_unseparated(") > 0");
                }
                q.push(")");
            }
            q.push(" ORDER BY c.chunk_index LIMIT 4");
            let mut rows = q.build().fetch_all(&self.pool).await.map_err(db)?;
            if rows.is_empty() && !terms.is_empty() {
                rows = sqlx::query("SELECT c.id,c.document_id,d.file_name,c.content FROM text_chunks c JOIN documents d ON d.id=c.document_id WHERE c.document_id=? ORDER BY c.chunk_index LIMIT 4")
                    .bind(id).fetch_all(&self.pool).await.map_err(db)?;
            }
            if rows.is_empty() {
                return Err(AppError::InvalidInput(
                    "A selected document has no indexed text yet. Let its import finish first."
                        .into(),
                ));
            }
            for row in rows {
                let chunk_id: String = row.get("id");
                let doc_id: String = row.get("document_id");
                let excerpt: String = row.get("content");
                out.push(LearningSourceDto {
                    id: format!("{doc_id}:{chunk_id}"),
                    title: row.get("file_name"),
                    url: None,
                    excerpt: excerpt.chars().take(2400).collect(),
                    acquired_at: chrono::Utc::now().timestamp_millis(),
                });
            }
        }
        Ok(out)
    }

    pub async fn create(&self, program: &LearningProgramDto) -> Result<()> {
        self.create_with_references(program, &[]).await
    }

    pub(super) async fn create_with_references(
        &self,
        program: &LearningProgramDto,
        references: &[super::sources::InitialReference],
    ) -> Result<()> {
        self.create_outline_draft(program, references, None).await
    }

    pub(super) async fn create_outline_draft(
        &self,
        program: &LearningProgramDto,
        references: &[super::sources::InitialReference],
        draft: Option<&super::outline_draft::OutlineDraft>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let s = &program.summary;
        sqlx::query("INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,current_lesson_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(&s.id).bind(&s.title).bind(&s.goal).bind(status(s.status.clone())).bind(s.revision).bind(&program.prior_knowledge).bind(program.minutes_per_session).bind(&program.model_name).bind(&s.current_lesson_id).bind(s.created_at).execute(&mut *tx).await.map_err(db)?;
        for (mi, m) in program.modules.iter().enumerate() {
            sqlx::query("INSERT INTO learning_modules(id,program_id,ordinal,title,summary,outcomes_json,prerequisite_ids_json,project_json) VALUES(?,?,?,?,?,?,?,?)")
                .bind(&m.id).bind(&s.id).bind(mi as i64).bind(&m.title).bind(&m.summary).bind(json(&m.outcomes)?).bind(json(&m.prerequisite_module_ids)?).bind(json(&m.project)?).execute(&mut *tx).await.map_err(db)?;
            for (li, l) in m.lessons.iter().enumerate() {
                sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation,completed) VALUES(?,?,?,?,?,?,?,?,?)")
                    .bind(&l.id).bind(&s.id).bind(&m.id).bind(li as i64).bind(&l.title).bind(&l.objective).bind(l.estimated_minutes).bind(prep(l.preparation.clone())).bind(l.completed as i64).execute(&mut *tx).await.map_err(db)?;
            }
        }
        for source in &program.sources {
            insert_source(
                &mut tx,
                &s.id,
                source,
                references.iter().find(|r| r.source_id == source.id),
            )
            .await?;
        }
        if let Some(draft) = draft {
            super::outline_draft_repository::insert(&mut tx, program, draft).await?;
        }
        tx.commit().await.map_err(db)
    }

    pub async fn list(&self) -> Result<Vec<LearningProgramSummaryDto>> {
        let rows=sqlx::query("SELECT p.*, (SELECT count(*) FROM learning_modules m WHERE m.program_id=p.id) module_count, (SELECT count(*) FROM learning_lessons l WHERE l.program_id=p.id) lesson_count, (SELECT count(*) FROM learning_lessons l WHERE l.program_id=p.id AND l.completed=1) completed_lessons FROM learning_programs p ORDER BY p.created_at DESC,p.id")
            .fetch_all(&self.pool).await.map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningProgramSummaryDto {
                    id: r.get("id"),
                    title: r.get("title"),
                    goal: r.get("goal"),
                    status: parse_status(r.get("status"))?,
                    revision: r.get("revision"),
                    module_count: r.get("module_count"),
                    lesson_count: r.get("lesson_count"),
                    completed_lessons: r.get("completed_lessons"),
                    current_lesson_id: r.get("current_lesson_id"),
                    created_at: r.get("created_at"),
                })
            })
            .collect()
    }

    pub async fn has_source_history(&self, program_id: &str) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_sources WHERE program_id=?)")
            .bind(program_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)
    }

    pub async fn get(&self, id: &str) -> Result<LearningProgramDto> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let program = Self::get_on(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(program)
    }

    /// Read through the caller's snapshot, including when composing a pack export.
    pub(super) async fn get_on(
        connection: &mut SqliteConnection,
        id: &str,
    ) -> Result<LearningProgramDto> {
        let p = sqlx::query("SELECT * FROM learning_programs WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        let lessons = sqlx::query(
            "SELECT * FROM learning_lessons WHERE program_id=? ORDER BY module_id,ordinal",
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
        // Batch children once per program instead of issuing two queries per lesson.
        let mut blocks_by_lesson: HashMap<String, Vec<LearningBlockDto>> = HashMap::new();
        for b in sqlx::query("SELECT b.* FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? ORDER BY b.lesson_id,b.ordinal")
            .bind(id).fetch_all(&mut *connection).await.map_err(db)? {
            blocks_by_lesson.entry(b.get("lesson_id")).or_default().push(LearningBlockDto {
                kind: parse_block(b.get("kind"))?,
                title: b.get("title"),
                body: b.get("body"),
                source_ids: decode(b.get("source_ids_json"))?,
                rubric: decode(b.get("rubric_json"))?,
            });
        }
        let mut questions_by_lesson: HashMap<String, Vec<LearningQuestionDto>> = HashMap::new();
        for q in sqlx::query("SELECT q.* FROM learning_questions q JOIN learning_lessons l ON l.id=q.lesson_id WHERE l.program_id=? ORDER BY q.lesson_id,q.kind,q.ordinal")
            .bind(id).fetch_all(&mut *connection).await.map_err(db)? {
            questions_by_lesson.entry(q.get("lesson_id")).or_default().push(LearningQuestionDto {
                id: q.get("id"),
                kind: parse_assessment(q.get("kind"))?,
                prompt: q.get("prompt"),
                options: decode(q.get("options_json"))?,
                source_ids: decode(q.get("source_ids_json"))?,
            });
        }
        let mut modules = Vec::new();
        for m in sqlx::query("SELECT * FROM learning_modules WHERE program_id=? ORDER BY ordinal")
            .bind(id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
        {
            let mid: String = m.get("id");
            let mut ls = Vec::new();
            for l in lessons
                .iter()
                .filter(|l| l.get::<String, _>("module_id") == mid)
            {
                let lid: String = l.get("id");
                let block_vec = blocks_by_lesson.remove(&lid).unwrap_or_default();
                let qvec = questions_by_lesson.remove(&lid).unwrap_or_default();
                ls.push(LearningLessonDto {
                    id: lid,
                    title: l.get("title"),
                    objective: l.get("objective"),
                    estimated_minutes: l.get("estimated_minutes"),
                    preparation: parse_prep(l.get("preparation"))?,
                    blocks: block_vec,
                    questions: qvec,
                    completed: l.get::<i64, _>("completed") != 0,
                });
            }
            modules.push(LearningModuleDto {
                id: mid,
                title: m.get("title"),
                summary: m.get("summary"),
                outcomes: decode(m.get("outcomes_json"))?,
                prerequisite_module_ids: decode(m.get("prerequisite_ids_json"))?,
                project: decode(m.get("project_json"))?,
                lessons: ls,
            });
        }
        let sources = sqlx::query("SELECT s.* FROM learning_sources s WHERE s.program_id=? AND (
                EXISTS(SELECT 1 FROM learning_source_library l WHERE l.program_id=s.program_id AND l.active_version_id=s.id AND l.deleted_at IS NULL)
                OR EXISTS(SELECT 1 FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=s.program_id AND EXISTS(SELECT 1 FROM json_each(b.source_ids_json) WHERE value=s.id))
                OR EXISTS(SELECT 1 FROM learning_questions q JOIN learning_lessons l ON l.id=q.lesson_id WHERE l.program_id=s.program_id AND EXISTS(SELECT 1 FROM json_each(q.source_ids_json) WHERE value=s.id))
                OR EXISTS(SELECT 1 FROM learning_attempts a,json_each(a.results_json) r,json_each(r.value,'$.sourceIds') src WHERE a.program_id=s.program_id AND src.value=s.id)
                OR EXISTS(SELECT 1 FROM learning_card_drafts d WHERE d.program_id=s.program_id AND EXISTS(SELECT 1 FROM json_each(d.source_ids_json) WHERE value=s.id))
                OR EXISTS(SELECT 1 FROM learning_card_origins o WHERE o.program_id=s.program_id AND EXISTS(SELECT 1 FROM json_each(o.source_ids_json) WHERE value=s.id))
            ) ORDER BY s.rowid")
                .bind(id).fetch_all(&mut *connection).await.map_err(db)?
                .into_iter().map(|r| LearningSourceDto {
                    id: r.get("id"),
                    title: r.get("title"),
                    url: r.get("url"),
                    excerpt: r.get("excerpt"),
                    acquired_at: r.get("acquired_at"),
                })
                .collect();
        let attempts = sqlx::query(
            "SELECT * FROM learning_attempts WHERE program_id=? ORDER BY submitted_at,id",
        )
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .into_iter()
        .map(|r| {
            Ok(LearningAttemptDto {
                id: r.get("id"),
                module_id: r.get("module_id"),
                lesson_id: r.get("lesson_id"),
                kind: parse_assessment(r.get("kind"))?,
                correct: r.get("correct"),
                total: r.get("total"),
                results: decode(r.get("results_json"))?,
                submitted_at: r.get("submitted_at"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let module_count: i64 = modules.len() as i64;
        let lesson_count = modules.iter().map(|m| m.lessons.len() as i64).sum();
        let completed = modules
            .iter()
            .flat_map(|m| &m.lessons)
            .filter(|l| l.completed)
            .count() as i64;
        Ok(LearningProgramDto {
            outline_review: Self::outline_draft_on(connection, id)
                .await?
                .map(|draft| draft.review),
            summary: LearningProgramSummaryDto {
                id: p.get("id"),
                title: p.get("title"),
                goal: p.get("goal"),
                status: parse_status(p.get("status"))?,
                revision: p.get("revision"),
                module_count,
                lesson_count,
                completed_lessons: completed,
                current_lesson_id: p.get("current_lesson_id"),
                created_at: p.get("created_at"),
            },
            prior_knowledge: p.get("prior_knowledge"),
            minutes_per_session: p.get("minutes_per_session"),
            model_name: p.get("model_name"),
            modules,
            sources,
            attempts,
        })
    }

    pub async fn accept(&self, req: &AcceptLearningProgramRequestDto) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_outline_drafts WHERE program_id=? AND json_extract(state_json,'$.review.status') IS NOT 'passed')").bind(&req.program_id).fetch_one(&mut *tx).await?;
        if pending {
            return Err(AppError::InvalidInput(
                "Resolve the saved outline findings before accepting this course.".into(),
            ));
        }
        let r=sqlx::query("UPDATE learning_programs SET title=?,status='active',revision=revision+1 WHERE id=? AND revision=? AND status='draft'").bind(req.title.trim()).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if r.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is no longer a draft; reload and retry.".into(),
            ));
        }
        tx.commit().await.map_err(db)
    }

    pub async fn prepare(
        &self,
        program_id: &str,
        lesson_id: &str,
        expected_revision: i64,
        prepared: &PreparedLearningLesson,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT preparation FROM learning_lessons WHERE id=? AND program_id=?")
                .bind(lesson_id)
                .bind(program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Lesson not found in this program".into()))?;
        if row.get::<String, _>("preparation") == "ready" {
            tx.rollback().await.map_err(db)?;
            return Err(AppError::InvalidInput(
                "Ready lessons are immutable.".into(),
            ));
        }
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(program_id).bind(expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        super::content_verification::persist_report(&mut tx, program_id, lesson_id, prepared)
            .await?;
        for (i, b) in prepared.blocks.iter().enumerate() {
            sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)").bind(lesson_id).bind(i as i64).bind(block_kind(b.kind.clone())).bind(&b.title).bind(&b.body).bind(json(&b.source_ids)?).bind(json(&b.rubric)?).execute(&mut *tx).await.map_err(db)?;
        }
        for (i, q) in prepared.questions.iter().enumerate() {
            sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) SELECT ?,?,module_id,?,?,?,?,? FROM learning_lessons WHERE id=?")
                .bind(&q.id).bind(lesson_id).bind(assessment(q.kind.clone())).bind(&q.prompt).bind(json(&q.options)?).bind(json(&q.source_ids)?).bind(i as i64).bind(lesson_id).execute(&mut *tx).await.map_err(db)?;
        }
        for k in &prepared.keys {
            sqlx::query("INSERT INTO learning_answer_keys(question_id,correct_index,explanation) VALUES(?,?,?)").bind(&k.question_id).bind(k.correct_index as i64).bind(&k.explanation).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=? AND program_id=?")
            .bind(lesson_id)
            .bind(program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn complete(&self, req: &CompleteLearningLessonRequestDto) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let changed=sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=? AND program_id=? AND preparation='ready' AND completed=0").bind(&req.lesson_id).bind(&req.program_id).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Only an incomplete, prepared lesson in this program can be completed.".into(),
            ));
        }
        let next:Option<String>=sqlx::query_scalar("SELECT id FROM learning_lessons WHERE program_id=? AND completed=0 ORDER BY (SELECT ordinal FROM learning_modules WHERE id=module_id),ordinal LIMIT 1").bind(&req.program_id).fetch_optional(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE learning_programs SET current_lesson_id=? WHERE id=?")
            .bind(next)
            .bind(&req.program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    pub async fn submit(
        &self,
        req: &SubmitLearningAttemptRequestDto,
    ) -> Result<LearningAttemptDto> {
        let mut normalized_answers = req.answers.clone();
        normalized_answers.sort_by(|a, b| a.question_id.cmp(&b.question_id));
        let payload = json(&(
            req.program_id.as_str(),
            req.module_id.as_str(),
            req.lesson_id.as_deref(),
            assessment(req.kind.clone()),
            normalized_answers,
        ))?;
        let hash = format!("{:x}", sha2::Sha256::digest(payload.as_bytes()));
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row) = sqlx::query("SELECT * FROM learning_attempts WHERE id=?")
            .bind(&req.attempt_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
        {
            if row.get::<String, _>("payload_hash") != hash {
                return Err(AppError::InvalidInput(
                    "This attempt ID was already used with different answers.".into(),
                ));
            }
            tx.rollback().await.map_err(db)?;
            return Ok(LearningAttemptDto {
                id: row.get("id"),
                module_id: row.get("module_id"),
                lesson_id: row.get("lesson_id"),
                kind: parse_assessment(row.get("kind"))?,
                correct: row.get("correct"),
                total: row.get("total"),
                results: decode(row.get("results_json"))?,
                submitted_at: row.get("submitted_at"),
            });
        }
        let p = sqlx::query("SELECT revision,status FROM learning_programs WHERE id=?")
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if p.get::<i64, _>("revision") != req.expected_revision
            || p.get::<String, _>("status") != "active"
        {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let (question_rows, scope_lesson): (Vec<sqlx::sqlite::SqliteRow>, Option<String>) =
            match req.kind {
                LearningAssessmentKind::Practice | LearningAssessmentKind::Quiz => {
                    let lesson_id = req.lesson_id.as_ref().ok_or_else(|| {
                        AppError::InvalidInput(
                            "Practice and quiz attempts require a lesson ID.".into(),
                        )
                    })?;
                    let exists:i64=sqlx::query_scalar("SELECT count(*) FROM learning_lessons WHERE id=? AND program_id=? AND module_id=? AND preparation='ready'").bind(lesson_id).bind(&req.program_id).bind(&req.module_id).fetch_one(&mut *tx).await.map_err(db)?;
                    if exists != 1 {
                        return Err(AppError::InvalidInput(
                            "Lesson does not belong to this active program module or is not ready."
                                .into(),
                        ));
                    }
                    (sqlx::query("SELECT q.*,k.correct_index,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id WHERE q.lesson_id=? AND q.kind=? ORDER BY q.ordinal").bind(lesson_id).bind(assessment(req.kind.clone())).fetch_all(&mut *tx).await.map_err(db)?,Some(lesson_id.clone()))
                }
                LearningAssessmentKind::Test => {
                    if req.lesson_id.is_some() {
                        return Err(AppError::InvalidInput(
                            "Module tests do not accept a lesson ID.".into(),
                        ));
                    }
                    let module_exists: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM learning_modules WHERE id=? AND program_id=?",
                    )
                    .bind(&req.module_id)
                    .bind(&req.program_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
                    if module_exists != 1 {
                        return Err(AppError::InvalidInput(
                            "Module does not belong to this program.".into(),
                        ));
                    }
                    let not_ready: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM learning_lessons WHERE module_id=? AND preparation!='ready'",
                    )
                    .bind(&req.module_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
                    if not_ready != 0 {
                        return Err(AppError::InvalidInput(
                            "Prepare every lesson in the module before starting its test.".into(),
                        ));
                    }
                    (sqlx::query("SELECT q.*,k.correct_index,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id JOIN learning_lessons l ON l.id=q.lesson_id WHERE q.module_id=? AND q.kind='test' AND l.preparation='ready' ORDER BY l.ordinal,q.ordinal").bind(&req.module_id).fetch_all(&mut *tx).await.map_err(db)?,None)
                }
            };
        if question_rows.is_empty() {
            return Err(AppError::InvalidInput(
                "No ready questions exist for this assessment.".into(),
            ));
        }
        if req.answers.len() != question_rows.len() {
            return Err(AppError::InvalidInput(
                "Submit one answer for every question in the assessment.".into(),
            ));
        }
        let expected: std::collections::HashSet<String> =
            question_rows.iter().map(|q| q.get("id")).collect();
        let mut seen = std::collections::HashSet::new();
        for a in &req.answers {
            if !seen.insert(a.question_id.clone()) {
                return Err(AppError::InvalidInput(
                    "Duplicate question answers are not allowed.".into(),
                ));
            }
            if !expected.contains(&a.question_id) {
                return Err(AppError::InvalidInput(
                    "Answer includes a question outside this assessment.".into(),
                ));
            }
        }
        let mut results = Vec::new();
        let mut correct = 0i64;
        for q in question_rows {
            let id: String = q.get("id");
            let answer = req
                .answers
                .iter()
                .find(|a| a.question_id == id)
                .ok_or_else(|| AppError::InvalidInput("Missing question answer.".into()))?;
            let options: Vec<String> = decode(q.get("options_json"))?;
            let correct_index: i64 = q.get("correct_index");
            if answer.selected_index >= options.len()
                || usize::try_from(correct_index)
                    .ok()
                    .filter(|i| *i < options.len())
                    .is_none()
            {
                return Err(AppError::InvalidInput(
                    "Selected answer is outside the available options.".into(),
                ));
            }
            if answer.selected_index as i64 == correct_index {
                correct += 1;
            }
            results.push(LearningQuestionResultDto {
                question_id: id,
                prompt: q.get("prompt"),
                options,
                selected_index: answer.selected_index,
                correct_index: correct_index as usize,
                explanation: q.get("explanation"),
                source_ids: decode(q.get("source_ids_json"))?,
            });
        }
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let submitted_at = chrono::Utc::now().timestamp_millis();
        let total = results.len() as i64;
        sqlx::query("INSERT INTO learning_attempts(id,program_id,module_id,lesson_id,kind,correct,total,answers_json,results_json,submitted_at,payload_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&req.attempt_id).bind(&req.program_id).bind(&req.module_id).bind(&scope_lesson).bind(assessment(req.kind.clone())).bind(correct).bind(total).bind(json(&req.answers)?).bind(json(&results)?).bind(submitted_at).bind(hash).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(LearningAttemptDto {
            id: req.attempt_id.clone(),
            module_id: req.module_id.clone(),
            lesson_id: scope_lesson,
            kind: req.kind.clone(),
            correct,
            total,
            results,
            submitted_at,
        })
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM learning_programs WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn ensure_lesson_note(
        &self,
        program_id: &str,
        lesson_id: &str,
        title: &str,
        content: &str,
    ) -> Result<(String, bool)> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (journal_id, _) = ensure_memory_resources(&mut tx, program_id).await?;
        validate_memory_lesson(&mut tx, program_id, lesson_id, false).await?;
        if let Some(note_id) = sqlx::query_scalar::<_, String>(
            "SELECT note_id FROM learning_lesson_note_links WHERE program_id=? AND lesson_id=?",
        )
        .bind(program_id)
        .bind(lesson_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            tx.commit().await.map_err(db)?;
            return Ok((note_id, false));
        }
        let note_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let note = crate::features::daily_notes::repository::WorkspaceNoteRecord {
            id: note_id.clone(),
            title: title.trim().to_owned(),
            journal_id: Some(journal_id),
            content: content.to_owned(),
            linked_document_ids: "[]".into(),
            linked_conversation_ids: "[]".into(),
            highlights_json: "[]".into(),
            sticky_notes_json: "[]".into(),
            conversation_snapshots_json: "[]".into(),
            sources_json: "[]".into(),
            created_at: now.clone(),
            updated_at: now,
        };
        crate::features::daily_notes::repository::DailyNotesRepository::insert_in_transaction(
            &mut tx, &note,
        )
        .await?;
        sqlx::query("INSERT INTO learning_lesson_note_links(program_id,lesson_id,note_id,created_at) VALUES(?,?,?,?)")
            .bind(program_id).bind(lesson_id).bind(&note_id).bind(chrono::Utc::now().timestamp_millis())
            .execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok((note_id, true))
    }

    pub async fn save_generated_drafts(
        &self,
        program_id: &str,
        lesson_id: &str,
        drafts: &[super::generation::GeneratedLearningCardDraft],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        ensure_memory_resources(&mut tx, program_id).await?;
        validate_memory_lesson(&mut tx, program_id, lesson_id, true).await?;
        let now = chrono::Utc::now().timestamp_millis();
        for draft in drafts {
            validate_draft_sources(&mut tx, program_id, lesson_id, &draft.source_ids, true).await?;
            sqlx::query("INSERT INTO learning_card_drafts(id,program_id,lesson_id,question,answer,explanation,source_ids_json,origin,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'generated','pending',?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(lesson_id).bind(&draft.question).bind(&draft.answer).bind(&draft.explanation).bind(json(&draft.source_ids)?).bind(now).bind(now)
                .execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    pub async fn save_manual_draft(
        &self,
        request: &SaveLearningCardDraftRequestDto,
    ) -> Result<String> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        ensure_memory_resources(&mut tx, &request.program_id).await?;
        validate_memory_lesson(&mut tx, &request.program_id, &request.lesson_id, true).await?;
        let existing_origin = if let Some(id) = request.draft_id.as_deref() {
            sqlx::query_scalar::<_, String>("SELECT origin FROM learning_card_drafts WHERE id=? AND program_id=? AND lesson_id=? AND status='pending'")
                .bind(id).bind(&request.program_id).bind(&request.lesson_id).fetch_optional(&mut *tx).await.map_err(db)?
        } else {
            None
        };
        let is_generated = existing_origin.as_deref() == Some("generated");
        validate_draft_sources(
            &mut tx,
            &request.program_id,
            &request.lesson_id,
            &request.source_ids,
            is_generated,
        )
        .await?;
        let now = chrono::Utc::now().timestamp_millis();
        let source_json = json(&request.source_ids)?;
        let draft_id = match request.draft_id.as_deref() {
            Some(draft_id) => {
                let result = sqlx::query("UPDATE learning_card_drafts SET question=?,answer=?,explanation=?,source_ids_json=?,updated_at=? WHERE id=? AND program_id=? AND lesson_id=? AND status='pending'")
                    .bind(request.question.trim()).bind(request.answer.trim()).bind(request.explanation.trim()).bind(source_json).bind(now).bind(draft_id).bind(&request.program_id).bind(&request.lesson_id)
                    .execute(&mut *tx).await.map_err(db)?;
                if result.rows_affected() != 1 {
                    return Err(AppError::NotFound(
                        "Editable learning card draft not found.".into(),
                    ));
                }
                draft_id.to_owned()
            }
            None => {
                let draft_id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO learning_card_drafts(id,program_id,lesson_id,question,answer,explanation,source_ids_json,origin,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'manual','pending',?,?)")
                    .bind(&draft_id).bind(&request.program_id).bind(&request.lesson_id).bind(request.question.trim()).bind(request.answer.trim()).bind(request.explanation.trim()).bind(source_json).bind(now).bind(now)
                    .execute(&mut *tx).await.map_err(db)?;
                draft_id
            }
        };
        tx.commit().await.map_err(db)?;
        Ok(draft_id)
    }

    pub async fn discard_card_draft(&self, program_id: &str, draft_id: &str) -> Result<()> {
        let result = sqlx::query("UPDATE learning_card_drafts SET status='discarded',updated_at=? WHERE id=? AND program_id=? AND status='pending'")
            .bind(chrono::Utc::now().timestamp_millis()).bind(draft_id).bind(program_id).execute(&self.pool).await.map_err(db)?;
        if result.rows_affected() == 0 {
            let status: Option<String> = sqlx::query_scalar(
                "SELECT status FROM learning_card_drafts WHERE id=? AND program_id=?",
            )
            .bind(draft_id)
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
            if status.as_deref() != Some("discarded") {
                return Err(AppError::NotFound(
                    "Pending learning card draft not found.".into(),
                ));
            }
        }
        Ok(())
    }

    pub async fn accept_card_draft(&self, program_id: &str, draft_id: &str) -> Result<String> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (_journal_id, deck_id) = ensure_memory_resources(&mut tx, program_id).await?;
        let row = sqlx::query("SELECT d.*,l.title lesson_title FROM learning_card_drafts d JOIN learning_lessons l ON l.id=d.lesson_id WHERE d.id=? AND d.program_id=?")
            .bind(draft_id).bind(program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning card draft not found.".into()))?;
        let status: String = row.get("status");
        if status == "accepted" {
            let card_id: Option<String> = row.try_get("accepted_card_id").map_err(db)?;
            let card_id = card_id
                .ok_or_else(|| AppError::NotFound("Accepted Study card was deleted.".into()))?;
            tx.commit().await.map_err(db)?;
            return Ok(card_id);
        }
        if status != "pending" {
            return Err(AppError::InvalidInput(
                "Discarded learning card drafts cannot be accepted.".into(),
            ));
        }
        let lesson_id: String = row.get("lesson_id");
        validate_memory_lesson(&mut tx, program_id, &lesson_id, true).await?;
        let source_ids: Vec<String> = decode(row.get("source_ids_json"))?;
        let generated = row.get::<String, _>("origin") == "generated";
        validate_draft_sources(&mut tx, program_id, &lesson_id, &source_ids, generated).await?;
        let mut citations = Vec::new();
        for source_id in &source_ids {
            let source = sqlx::query(
                "SELECT id,title,url,excerpt FROM learning_sources WHERE program_id=? AND id=?",
            )
            .bind(program_id)
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| {
                AppError::InvalidInput("Learning card refers to a missing source snapshot.".into())
            })?;
            let source_id: String = source.get("id");
            let url: Option<String> = source.get("url");
            let (document_id, chunk_id) = if url.is_none() {
                source_id
                    .split_once(':')
                    .map(|(doc, chunk)| (doc.to_owned(), chunk.to_owned()))
                    .unwrap_or_default()
            } else {
                (String::new(), source_id.clone())
            };
            let file_path = if !document_id.is_empty() {
                sqlx::query_scalar::<_, String>("SELECT file_path FROM documents WHERE id=?")
                    .bind(&document_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?
                    .unwrap_or_default()
            } else {
                String::new()
            };
            citations.push(crate::features::study::dto::StudySourceDto {
                chunk_id,
                document_id,
                file_name: source.get("title"),
                file_path,
                excerpt: source.get("excerpt"),
                url,
            });
        }
        if citations.is_empty() {
            citations.push(crate::features::study::dto::StudySourceDto {
                chunk_id: format!("learning-memory:{draft_id}"),
                document_id: String::new(),
                file_name: if generated {
                    row.get("lesson_title")
                } else {
                    "Personal learning card".into()
                },
                file_path: String::new(),
                excerpt: if generated {
                    "AI-authored recall from the prepared lesson; no external source."
                } else {
                    "Learner-authored recall card without an external source."
                }
                .into(),
                url: None,
            });
        }
        let now = chrono::Utc::now().timestamp_millis();
        let card_id = uuid::Uuid::new_v4().to_string();
        let answer: String = row.get("answer");
        let source = citations
            .first()
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("Learning card needs a source record.".into()))?;
        let card = crate::features::study::dto::StudyCardDto {
            id: card_id.clone(),
            format: crate::features::study::dto::StudyCardFormat::QuestionAnswer,
            scheduler_version: crate::features::study::dto::StudySchedulerVersion::ExpandingV1,
            deck_id: deck_id.clone(),
            question: row.get("question"),
            answer,
            options: vec![],
            correct_index: 0,
            explanation: row.get("explanation"),
            source,
            citations: citations.clone(),
            topic: row.get("lesson_title"),
            due_at: now,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        };
        crate::features::study::repository::StudyRepository::insert_learning_card_in_transaction(
            &mut tx, &card,
        )
        .await?;
        let origin: String = row.get("origin");
        sqlx::query("INSERT INTO learning_card_origins(card_id,program_id,lesson_id,origin,source_ids_json,accepted_at) VALUES(?,?,?,?,?,?)")
            .bind(&card_id).bind(program_id).bind(&lesson_id).bind(&origin).bind(json(&source_ids)?).bind(now)
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE learning_card_drafts SET status='accepted',accepted_card_id=?,updated_at=? WHERE id=? AND program_id=? AND status='pending'")
            .bind(&card_id).bind(now).bind(draft_id).bind(program_id).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(card_id)
    }

    pub async fn memory_state(&self, program_id: &str) -> Result<LearningMemoryState> {
        let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        if exists == 0 {
            return Err(AppError::NotFound("Learning program not found.".into()));
        }
        let root = sqlx::query("SELECT journal_id,deck_id FROM learning_memory WHERE program_id=?")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let (journal_id, deck_id) = match root {
            Some(r) => (
                r.try_get::<Option<String>, _>("journal_id").map_err(db)?,
                r.try_get::<Option<String>, _>("deck_id").map_err(db)?,
            ),
            None => (None, None),
        };
        let lesson_notes = sqlx::query("SELECT lesson_id,note_id FROM learning_lesson_note_links WHERE program_id=? ORDER BY rowid")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| (r.get("lesson_id"),r.get("note_id"))).collect();
        let drafts = sqlx::query("SELECT * FROM learning_card_drafts WHERE program_id=? AND status='pending' ORDER BY created_at,id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| Ok(LearningCardDraftDto {
                id:r.get("id"),lesson_id:r.get("lesson_id"),question:r.get("question"),answer:r.get("answer"),explanation:r.get("explanation"),
                source_ids:decode(r.get("source_ids_json"))?,origin:parse_draft_origin(r.get("origin"))?,created_at:r.get("created_at"),
            })).collect::<Result<Vec<_>>>()?;
        let accepted_cards = sqlx::query("SELECT card_id,lesson_id,origin,source_ids_json,accepted_at FROM learning_card_origins WHERE program_id=? ORDER BY accepted_at,card_id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| Ok(LearningCardOriginDto {
                card_id:r.get("card_id"),lesson_id:r.get("lesson_id"),origin:parse_draft_origin(r.get("origin"))?,source_ids:decode(r.get("source_ids_json"))?,accepted_at:r.get("accepted_at"),
            })).collect::<Result<Vec<_>>>()?;
        Ok(LearningMemoryState {
            journal_id,
            deck_id,
            lesson_notes,
            drafts,
            accepted_cards,
        })
    }
}

fn parse_draft_origin(value: &str) -> Result<LearningCardDraftOrigin> {
    match value {
        "generated" => Ok(LearningCardDraftOrigin::Generated),
        "manual" => Ok(LearningCardDraftOrigin::Manual),
        _ => Err(AppError::Database(
            "Invalid learning card draft origin.".into(),
        )),
    }
}

async fn validate_memory_lesson(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    lesson_id: &str,
    require_ready: bool,
) -> Result<()> {
    let row = sqlx::query("SELECT p.status,l.preparation FROM learning_programs p JOIN learning_lessons l ON l.program_id=p.id WHERE p.id=? AND l.id=?")
        .bind(program_id).bind(lesson_id).fetch_optional(&mut **tx).await.map_err(db)?
        .ok_or_else(|| AppError::NotFound("Lesson does not belong to this learning program.".into()))?;
    if row.get::<String, _>("status") != "active" {
        return Err(AppError::InvalidInput(
            "Accept the program before using its memory tools.".into(),
        ));
    }
    if require_ready && row.get::<String, _>("preparation") != "ready" {
        return Err(AppError::InvalidInput(
            "Prepare this lesson before using its memory tools.".into(),
        ));
    }
    Ok(())
}

async fn validate_draft_sources(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    lesson_id: &str,
    source_ids: &[String],
    generated: bool,
) -> Result<()> {
    let cited_blocks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=? AND json_array_length(source_ids_json)>0")
        .bind(lesson_id).fetch_one(&mut **tx).await.map_err(db)?;
    if generated && source_ids.is_empty() && cited_blocks > 0 {
        return Err(AppError::InvalidInput(
            "Generated card drafts must cite lesson sources.".into(),
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for source_id in source_ids {
        if !unique.insert(source_id) {
            return Err(AppError::InvalidInput(
                "A learning card cannot cite a source more than once.".into(),
            ));
        }
        let exists: i64 =
            sqlx::query_scalar("SELECT count(*) FROM learning_sources WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(source_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(db)?;
        if exists == 0 {
            return Err(AppError::InvalidInput(
                "Learning card cites a source outside this program.".into(),
            ));
        }
        if generated {
            let references: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM learning_blocks WHERE lesson_id=? AND EXISTS (SELECT 1 FROM json_each(learning_blocks.source_ids_json) WHERE value=?)) + (SELECT count(*) FROM learning_questions WHERE lesson_id=? AND EXISTS (SELECT 1 FROM json_each(learning_questions.source_ids_json) WHERE value=?))")
                .bind(lesson_id).bind(source_id).bind(lesson_id).bind(source_id).fetch_one(&mut **tx).await.map_err(db)?;
            if references == 0 {
                return Err(AppError::InvalidInput(
                    "Generated card sources must be referenced by its lesson.".into(),
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn ensure_memory_resources(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
) -> Result<(String, String)> {
    // First write serializes concurrent ensures before checking resource links.
    sqlx::query("INSERT OR IGNORE INTO learning_memory(program_id,journal_id,deck_id,created_at) VALUES(?,NULL,NULL,?)")
        .bind(program_id).bind(chrono::Utc::now().timestamp_millis()).execute(&mut **tx).await.map_err(db)?;
    let program =
        sqlx::query("SELECT title,goal,status,model_name FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found.".into()))?;
    if program.get::<String, _>("status") != "active" {
        return Err(AppError::InvalidInput(
            "Accept the program before creating its memory workspace.".into(),
        ));
    }
    // The nullable root lets either canonical resource be deleted independently.
    let root = sqlx::query("SELECT journal_id,deck_id FROM learning_memory WHERE program_id=?")
        .bind(program_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)?;
    let base_title: String = program.get("title");
    let safe_id: String = program_id.chars().take(8).collect();

    let journal_id: String = match root
        .try_get::<Option<String>, _>("journal_id")
        .map_err(db)?
    {
        Some(id) => id,
        None => {
            let id = format!("journal_learning_{program_id}");
            let name = format!(
                "Studio · {} · {safe_id}",
                base_title.chars().take(70).collect::<String>()
            );
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT OR IGNORE INTO journals(id,name,description,icon,is_archived,sort_order,created_at,updated_at) VALUES(?,?,?,?,0,0,?,?)")
                .bind(&id).bind(&name).bind("Learning Studio notebook").bind("book-open").bind(&now).bind(&now)
                .execute(&mut **tx).await.map_err(db)?;
            let canonical_id: String =
                sqlx::query_scalar("SELECT id FROM journals WHERE id=? OR name=?")
                    .bind(&id)
                    .bind(&name)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(db)?;
            sqlx::query("UPDATE learning_memory SET journal_id=? WHERE program_id=?")
                .bind(&canonical_id)
                .bind(program_id)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
            canonical_id
        }
    };

    let deck_id: String = match root.try_get::<Option<String>, _>("deck_id").map_err(db)? {
        Some(id) => id,
        None => {
            let id = format!("learning-studio-{program_id}");
            crate::features::study::repository::StudyRepository::insert_learning_deck_in_transaction(
                tx, &id, &base_title, &program.get::<String,_>("goal"),
                "Recall concepts from Learning Studio lessons", "learning-studio",
                chrono::Utc::now().timestamp_millis(),
            ).await?;
            sqlx::query("UPDATE learning_memory SET deck_id=? WHERE program_id=?")
                .bind(&id)
                .bind(program_id)
                .execute(&mut **tx)
                .await
                .map_err(db)?;
            id
        }
    };
    Ok((journal_id, deck_id))
}

use sha2::Digest;

pub(super) async fn insert_source(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    source: &LearningSourceDto,
    reference: Option<&super::sources::InitialReference>,
) -> Result<()> {
    let text = reference
        .map(|r| r.captured.text.as_str())
        .unwrap_or(&source.excerpt)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned();
    let preview = source
        .excerpt
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned();
    sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,?,?,?)").bind(&source.id).bind(program_id).bind(source.title.trim()).bind(&source.url).bind(&preview).bind(source.acquired_at).execute(&mut **tx).await.map_err(db)?;
    let kind = if source.url.is_some() {
        "web"
    } else {
        "document"
    };
    let origin = if source.url.is_some() {
        source.url.clone().unwrap_or_default()
    } else {
        source
            .id
            .split_once(':')
            .map(|(id, _)| id.to_owned())
            .unwrap_or_else(|| source.id.clone())
    };
    let origin = reference.map(|r| r.origin.clone()).unwrap_or(origin);
    let policy = if source.url.is_some() {
        "manual"
    } else {
        "fixed"
    };
    let requested_url = reference
        .and_then(|r| r.captured.requested_url.as_deref())
        .or(source.url.as_deref());
    let resolved_url = reference
        .and_then(|r| r.captured.resolved_url.as_deref())
        .or(source.url.as_deref());
    let digest = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
    sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,NULL,0,?,?)")
        .bind(&source.id).bind(program_id).bind(kind).bind(origin).bind(requested_url).bind(policy).bind(&source.id).bind(source.acquired_at).bind(source.acquired_at).execute(&mut **tx).await.map_err(db)?;
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,1,?,NULL,?,?,?,?,?,?,?,?,?)")
        .bind(&source.id).bind(program_id).bind(&source.id).bind(source.title.trim()).bind(requested_url).bind(resolved_url).bind(&text).bind(text.chars().take(2400).collect::<String>()).bind(&digest).bind(text.split_whitespace().count() as i64).bind(reference.is_none() as i64).bind(reference.map(|r| r.captured.extraction_version.as_str()).unwrap_or("program_seed_bounded_v1")).bind(source.acquired_at).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
