//! In-place corrections to lesson and module titles, objectives, summaries, and time estimates.
use super::{lesson_activity_started, LearningCurriculumRepository};
use crate::{
    curriculum::{self, LearningCurriculumRevision},
    persistence::{decode, encode},
};
use lattice_core::shared::{
    error::{AppError, Result},
    hash::sha256_hex,
    persistence::WriteTx,
};
use sqlx::{Row, SqliteConnection};
use std::collections::HashSet;

pub(crate) const LESSON_REASON: &str = "Corrected a lesson's title, objective or time.";
pub(crate) const MODULE_REASON: &str = "Corrected a module's title or summary.";

pub(crate) struct LessonRow {
    pub program_id: String,
    pub title: String,
    pub objective: String,
    pub estimated_minutes: u32,
    pub plan_fixed: bool,
    pub draft: bool,
}

pub(crate) async fn lesson_row(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<Option<LessonRow>> {
    let Some(row) = sqlx::query(
        "SELECT l.program_id, l.module_id, l.title, l.objective, l.estimated_minutes, l.completed, p.status \
         FROM learning_lessons l \
         JOIN learning_programs p ON p.id = l.program_id \
         WHERE l.id = ?",
    )
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await?
    else {
        return Ok(None);
    };
    let program_id: String = row.get("program_id");
    let module_id: String = row.get("module_id");
    let completed = row.get::<i64, _>("completed") != 0;
    let activity_started =
        lesson_activity_started(&mut *connection, &program_id, &module_id, lesson_id).await?;
    let estimated_minutes =
        u32::try_from(row.get::<i64, _>("estimated_minutes").max(1)).unwrap_or(u32::MAX);
    Ok(Some(LessonRow {
        program_id,
        title: row.get("title"),
        objective: row.get("objective"),
        estimated_minutes,
        plan_fixed: completed || activity_started,
        draft: row.get::<String, _>("status") == "draft",
    }))
}

pub(crate) struct ModuleRow {
    pub program_id: String,
    pub title: String,
    pub summary: String,
    pub draft: bool,
}

pub(crate) async fn module_row(
    connection: &mut SqliteConnection,
    module_id: &str,
) -> Result<Option<ModuleRow>> {
    let Some(row) = sqlx::query(
        "SELECT m.program_id, m.title, m.summary, p.status \
         FROM learning_modules m \
         JOIN learning_programs p ON p.id = m.program_id \
         WHERE m.id = ?",
    )
    .bind(module_id)
    .fetch_optional(&mut *connection)
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(ModuleRow {
        program_id: row.get("program_id"),
        title: row.get("title"),
        summary: row.get("summary"),
        draft: row.get::<String, _>("status") == "draft",
    }))
}

pub(crate) struct LessonCorrection<'a> {
    pub lesson_id: &'a str,
    pub title: &'a str,
    pub objective: &'a str,
    pub estimated_minutes: Option<u32>,
    pub at: i64,
}

pub(crate) async fn correct_lesson(
    tx: &mut WriteTx,
    program_id: &str,
    correction: &LessonCorrection<'_>,
) -> Result<()> {
    record_correction_revision(tx, program_id, LESSON_REASON, correction.at, |snapshot| {
        let Some(lesson) = snapshot
            .modules
            .iter_mut()
            .flat_map(|module| module.lessons.iter_mut())
            .find(|lesson| lesson.id == correction.lesson_id)
        else {
            return false;
        };
        lesson.title = correction.title.to_owned();
        lesson.objective = correction.objective.to_owned();
        if let Some(minutes) = correction.estimated_minutes {
            lesson.estimated_minutes = minutes;
        }
        true
    })
    .await?;
    sqlx::query(
        "UPDATE learning_lessons \
         SET title=?, objective=?, estimated_minutes=COALESCE(?, estimated_minutes) \
         WHERE id=? AND program_id=?",
    )
    .bind(correction.title)
    .bind(correction.objective)
    .bind(correction.estimated_minutes.map(i64::from))
    .bind(correction.lesson_id)
    .bind(program_id)
    .execute(&mut **tx)
    .await?;
    bump_program_revision(tx, program_id).await
}

pub(crate) struct ModuleCorrection<'a> {
    pub module_id: &'a str,
    pub title: &'a str,
    pub summary: &'a str,
    pub at: i64,
}

pub(crate) async fn correct_module(
    tx: &mut WriteTx,
    program_id: &str,
    correction: &ModuleCorrection<'_>,
) -> Result<()> {
    record_correction_revision(tx, program_id, MODULE_REASON, correction.at, |snapshot| {
        let Some(module) = snapshot
            .modules
            .iter_mut()
            .find(|module| module.id == correction.module_id)
        else {
            return false;
        };
        module.title = correction.title.to_owned();
        module.purpose = correction.summary.to_owned();
        true
    })
    .await?;
    sqlx::query("UPDATE learning_modules SET title=?, summary=? WHERE id=? AND program_id=?")
        .bind(correction.title)
        .bind(correction.summary)
        .bind(correction.module_id)
        .bind(program_id)
        .execute(&mut **tx)
        .await?;
    bump_program_revision(tx, program_id).await
}

async fn bump_program_revision(tx: &mut WriteTx, program_id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=?")
        .bind(program_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn record_correction_revision(
    tx: &mut WriteTx,
    program_id: &str,
    reason: &str,
    at: i64,
    apply: impl Fn(&mut LearningCurriculumRevision) -> bool,
) -> Result<()> {
    LearningCurriculumRepository::seed_on(tx, program_id).await?;
    let accepted_row = sqlx::query(
        "SELECT id, snapshot_json, required_lesson_count, resume_lesson_id \
         FROM learning_curriculum_revisions \
         WHERE program_id=? AND status='accepted'",
    )
    .bind(program_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;

    let earlier_id: String = accepted_row.get("id");
    let required_lesson_count: i64 = accepted_row.get("required_lesson_count");
    let resume_lesson_id: Option<String> = accepted_row.get("resume_lesson_id");
    let mut snapshot: LearningCurriculumRevision = decode(accepted_row.get("snapshot_json"))?;
    if !apply(&mut snapshot) {
        return Err(AppError::InvalidState(
            "This is not in the course plan.".into(),
        ));
    }

    let max_revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision), 0) FROM learning_curriculum_revisions WHERE program_id=?",
    )
    .bind(program_id)
    .fetch_one(&mut **tx)
    .await?;
    let next_revision = u32::try_from(max_revision.saturating_add(1))
        .map_err(|_| AppError::InvalidState("Curriculum revision overflow.".into()))?;
    let next_id = uuid::Uuid::new_v4().to_string();
    snapshot.id = next_id.clone();
    snapshot.revision_number = next_revision;
    snapshot.parent_revision_id = Some(earlier_id.clone());
    snapshot.reason = reason.to_owned();
    snapshot.created_at = at;
    curriculum::validate_curriculum(&snapshot)?;
    let snapshot_json = encode(&snapshot)?;
    let snapshot_sha256 = sha256_hex(&snapshot_json);

    sqlx::query("UPDATE learning_curriculum_revisions SET status='superseded' WHERE id=?")
        .bind(&earlier_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO learning_curriculum_revisions(\
         id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,\
         required_lesson_count,resume_lesson_id,created_at,accepted_at\
         ) VALUES(?,?,?,?,'accepted',?,?,?,?,?,?,?)",
    )
    .bind(&next_id)
    .bind(program_id)
    .bind(i64::from(next_revision))
    .bind(&earlier_id)
    .bind(reason)
    .bind(snapshot_json)
    .bind(snapshot_sha256)
    .bind(required_lesson_count)
    .bind(resume_lesson_id)
    .bind(at)
    .bind(at)
    .execute(&mut **tx)
    .await?;

    let draft_row = sqlx::query(
        "SELECT id, snapshot_json FROM learning_curriculum_revisions \
         WHERE program_id=? AND status='draft' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(program_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(draft_row) = draft_row {
        let draft_id: String = draft_row.get("id");
        let mut draft_snapshot: LearningCurriculumRevision =
            decode(draft_row.get("snapshot_json"))?;
        if apply(&mut draft_snapshot) {
            curriculum::validate_curriculum(&draft_snapshot)?;
            let draft_json = encode(&draft_snapshot)?;
            let draft_sha256 = sha256_hex(&draft_json);
            sqlx::query(
                "UPDATE learning_curriculum_revisions SET snapshot_json=?, snapshot_sha256=? WHERE id=?",
            )
            .bind(draft_json)
            .bind(draft_sha256)
            .bind(draft_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Lessons named by the program's unfinished preparation jobs under its current outline revision.
pub(crate) async fn lessons_being_prepared(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<u32> {
    let Some(content_revision) =
        sqlx::query_scalar::<_, i64>("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *connection)
            .await?
    else {
        return Ok(0);
    };
    let jobs = LearningCurriculumRepository::unfinished_preparations(
        connection,
        program_id,
        content_revision,
    )
    .await?;
    let distinct: HashSet<String> = jobs
        .into_iter()
        .flat_map(|(_, request)| request.lesson_ids)
        .collect();
    Ok(u32::try_from(distinct.len()).unwrap_or(u32::MAX))
}
