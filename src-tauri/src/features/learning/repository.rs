//! SQL persistence for Learning Studio. Domain orchestration never issues SQL.
use super::dto::*;
use super::persistence::{db, decode, encode as json, hash_text};
use super::recall::{
    study_dto::{StudyCardDto, StudyCardFormat, StudySourceDto},
    study_repository::StudyRepository,
};
use super::references::trust::capture;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

mod memory;
mod progress;
mod tangents;

pub(crate) use memory::ensure_memory_resources;

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
fn attempt_from_row(r: &sqlx::sqlite::SqliteRow) -> Result<LearningAttemptDto> {
    let coverage = match (r.get("covered_lessons"), r.get("module_lessons")) {
        (Some(covered_lessons), Some(module_lessons)) => Some(LearningModuleTestCoverageDto {
            covered_lessons,
            module_lessons,
        }),
        _ => None,
    };
    Ok(LearningAttemptDto {
        id: r.get("id"),
        module_id: r.get("module_id"),
        lesson_id: r.get("lesson_id"),
        kind: parse_assessment(r.get("kind"))?,
        correct: r.get("correct"),
        total: r.get("total"),
        results: decode(r.get("results_json"))?,
        submitted_at: r.get("submitted_at"),
        coverage,
    })
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
        .iter()
        .map(attempt_from_row)
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

    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM learning_programs WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}

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
    let digest = hash_text(&text);
    sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at,added_by) VALUES(?,?,?,?,?,?,?,NULL,0,?,?,?)")
        .bind(&source.id).bind(program_id).bind(kind).bind(origin).bind(requested_url).bind(policy).bind(&source.id).bind(source.acquired_at).bind(source.acquired_at).bind(lattice_core::domain::learning::claim_review::SourceAddedBy::Outline.as_str()).execute(&mut **tx).await.map_err(db)?;
    let publisher = reference.and_then(|r| r.captured.publisher.as_deref());
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,1,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&source.id).bind(program_id).bind(&source.id).bind(source.title.trim()).bind(publisher).bind(requested_url).bind(resolved_url).bind(&text).bind(text.chars().take(2400).collect::<String>()).bind(&digest).bind(text.split_whitespace().count() as i64).bind(reference.is_none() as i64).bind(reference.map(|r| r.captured.extraction_version.as_str()).unwrap_or("program_seed_bounded_v1")).bind(source.acquired_at).execute(&mut **tx).await.map_err(db)?;
    let signals = reference.map_or_else(
        || capture::not_extracted(capture::SEED_WITHOUT_CAPTURE),
        |r| r.captured.signals.clone(),
    );
    let kind = if kind == "web" {
        LearningSourceKind::Web
    } else {
        LearningSourceKind::Document
    };
    let version = capture::CapturedVersion {
        program_id,
        source_id: &source.id,
        version_id: &source.id,
        kind: &kind,
        title: source.title.trim(),
        resolved_url,
        text: &text,
        acquired_at_ms: source.acquired_at,
    };
    capture::record(tx, &version, &signals).await?;
    Ok(())
}
