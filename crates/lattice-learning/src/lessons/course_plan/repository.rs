//! The course plan's tables: `learning_course_plan_lessons`,
//! `learning_course_plan_entry_history`, and the course plan's rows of
//! `learning_jobs`.

use super::edit::{ENTRY_CHANGED, LESSON_LOCKED, LESSON_WRITTEN, REVISION_CHANGED};
use super::job::COURSE_PLAN;
use super::view::OutlineLesson;
use super::PlannedLesson;
use crate::curriculum_repository::LearningCurriculumRepository;
use crate::persistence::{db, now};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobRecord, JobStore};
use sqlx::SqliteConnection;
use std::collections::{HashMap, HashSet};

/// The outline revision a program's plan is keyed on.
pub(super) async fn content_revision(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(connection)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Learning program not found".into()))
}

/// A program's status and revision.
pub(super) async fn program_state(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<(String, i64)>> {
    sqlx::query_as("SELECT status,revision FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(connection)
        .await
        .map_err(db)
}

/// Each lesson's id, preparation and completed flag, in course order.
pub(super) async fn outline_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(String, String, i64)>> {
    sqlx::query_as(
        "SELECT l.id,l.preparation,l.completed FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? ORDER BY m.ordinal,l.ordinal",
    )
    .bind(program_id)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// The saved entries of a revision: lesson id, entry JSON, entry revision.
pub(super) async fn saved_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Vec<(String, String, i64)>> {
    sqlx::query_as(
        "SELECT lesson_id,entry_json,entry_revision FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=?",
    )
    .bind(program_id)
    .bind(outline_revision)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// The entries of a revision as lesson id and entry JSON.
pub(super) async fn entry_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Vec<(String, String)>> {
    sqlx::query_as(
        "SELECT lesson_id,entry_json FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=?",
    )
    .bind(program_id)
    .bind(outline_revision)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// The newest replaced entry of each lesson: lesson id and entry JSON.
pub(super) async fn newest_replaced_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Vec<(String, String)>> {
    sqlx::query_as(
        "SELECT h.lesson_id,h.entry_json FROM learning_course_plan_entry_history h WHERE h.program_id=? AND h.outline_revision=? AND h.entry_revision=(SELECT max(entry_revision) FROM learning_course_plan_entry_history WHERE program_id=h.program_id AND outline_revision=h.outline_revision AND lesson_id=h.lesson_id)",
    )
    .bind(program_id)
    .bind(outline_revision)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// Saves a lesson's entry unless it already has one.
pub(super) async fn insert_entry(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lesson_id: &str,
    entry_json: &str,
    created_at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_course_plan_lessons(program_id,outline_revision,lesson_id,entry_json,created_at) VALUES(?,?,?,?,?) ON CONFLICT(program_id,outline_revision,lesson_id) DO NOTHING")
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .bind(entry_json)
        .bind(created_at)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Replaces a lesson's entry, keeping the old one in its history.
pub(super) async fn replace_entry(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lesson_id: &str,
    entry_json: &str,
    replaced_at: i64,
) -> Result<()> {
    let (old, revision): (String, i64) = sqlx::query_as(
        "SELECT entry_json,entry_revision FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(outline_revision)
    .bind(lesson_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    sqlx::query("INSERT INTO learning_course_plan_entry_history(program_id,outline_revision,lesson_id,entry_revision,entry_json,replaced_at) VALUES(?,?,?,?,?,?)")
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .bind(revision)
        .bind(old)
        .bind(replaced_at)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json=?,entry_revision=entry_revision+1 WHERE program_id=? AND outline_revision=? AND lesson_id=?")
        .bind(entry_json)
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Puts back the entry the newest change replaced and drops it from the
/// history. False when there is nothing to put back.
pub(super) async fn restore_entry(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lesson_id: &str,
) -> Result<bool> {
    let newest: Option<(i64, String)> = sqlx::query_as(
        "SELECT entry_revision,entry_json FROM learning_course_plan_entry_history WHERE program_id=? AND outline_revision=? AND lesson_id=? ORDER BY entry_revision DESC LIMIT 1",
    )
    .bind(program_id)
    .bind(outline_revision)
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    let Some((replaced, entry)) = newest else {
        return Ok(false);
    };
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json=?,entry_revision=entry_revision+1 WHERE program_id=? AND outline_revision=? AND lesson_id=?")
        .bind(entry)
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM learning_course_plan_entry_history WHERE program_id=? AND outline_revision=? AND lesson_id=? AND entry_revision=?")
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .bind(replaced)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(true)
}

/// Every job of a program with the outline revision it was made for.
pub(super) async fn job_revisions(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(String, Option<i64>)>> {
    sqlx::query_as("SELECT job_id,content_revision FROM learning_jobs WHERE program_id=?")
        .bind(program_id)
        .fetch_all(connection)
        .await
        .map_err(db)
}

/// Records the program, base revision and outline revision of a new job.
pub(super) async fn insert_job_row(
    connection: &mut SqliteConnection,
    job_id: &str,
    program_id: &str,
    base_revision: i64,
    outline_revision: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) VALUES(?,?,?,?)")
        .bind(job_id)
        .bind(program_id)
        .bind(base_revision)
        .bind(outline_revision)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Gives a new attempt the program and revisions of the job it retries.
pub(super) async fn copy_job_row(
    connection: &mut SqliteConnection,
    new_job_id: &str,
    old_job_id: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) SELECT ?,program_id,base_revision_number,content_revision FROM learning_jobs WHERE job_id=?")
        .bind(new_job_id)
        .bind(old_job_id)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

pub(super) async fn outline_lessons(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<OutlineLesson>> {
    let rows = outline_rows(connection, program_id).await?;
    Ok(rows
        .into_iter()
        .map(|(id, preparation, completed)| OutlineLesson {
            id,
            open: preparation == "outline" && completed == 0,
        })
        .collect())
}

/// The lessons an unfinished `lesson_preparation` job of the current outline names.
pub(super) async fn locked_lessons(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<HashSet<String>> {
    let revision = content_revision(&mut *connection, program_id).await?;
    Ok(
        LearningCurriculumRepository::unfinished_preparations(connection, program_id, revision)
            .await?
            .into_iter()
            .flat_map(|(_, request)| request.lesson_ids)
            .collect(),
    )
}

/// The saved entries of a revision with their entry revisions, by lesson.
pub(super) async fn saved_entries(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<HashMap<String, (PlannedLesson, i64)>> {
    let rows = saved_rows(connection, program_id, outline_revision).await?;
    rows.into_iter()
        .map(|(id, entry, revision)| {
            let entry: PlannedLesson = serde_json::from_str(&entry).map_err(|error| {
                AppError::InternalError(format!(
                    "The course plan entry for lesson {id} cannot be read: {error}"
                ))
            })?;
            Ok((id, (entry, revision)))
        })
        .collect()
}

/// The newest replaced entry of each lesson.
pub(super) async fn newest_replaced(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<HashMap<String, PlannedLesson>> {
    let rows = newest_replaced_rows(connection, program_id, outline_revision).await?;
    rows.into_iter()
        .map(|(id, entry)| {
            let entry: PlannedLesson = serde_json::from_str(&entry).map_err(|error| {
                AppError::InternalError(format!(
                    "The replaced course plan entry for lesson {id} cannot be read: {error}"
                ))
            })?;
            Ok((id, entry))
        })
        .collect()
}

/// The newest plan job of a program for an outline revision.
pub(super) async fn latest_attempt(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Option<JobRecord>> {
    let revisions = job_revisions(&mut *connection, program_id).await?;
    let jobs = JobStore::list_for_subject_in(connection, COURSE_PLAN, program_id).await?;
    Ok(jobs.into_iter().find(|job| {
        revisions
            .iter()
            .any(|(id, revision)| *id == job.id && *revision == Some(outline_revision))
    }))
}

/// Whether the outline is still `outline_revision` and each of `lessons` is
/// in it, still open, not being written, and at the revision the caller saw.
pub(super) async fn check_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lessons: &[(&str, i64)],
) -> Result<()> {
    if content_revision(connection, program_id).await? != outline_revision {
        return Err(AppError::InvalidState(REVISION_CHANGED.into()));
    }
    let outline = outline_lessons(connection, program_id).await?;
    let locked = locked_lessons(connection, program_id).await?;
    let saved = saved_entries(connection, program_id, outline_revision).await?;
    for (lesson_id, expected) in lessons {
        let lesson = outline
            .iter()
            .find(|lesson| lesson.id == *lesson_id)
            .ok_or_else(|| AppError::NotFound("That lesson is not in this course.".into()))?;
        if locked.contains(*lesson_id) {
            return Err(AppError::InvalidState(LESSON_LOCKED.into()));
        }
        if !lesson.open {
            return Err(AppError::InvalidState(LESSON_WRITTEN.into()));
        }
        let (_, revision) = saved
            .get(*lesson_id)
            .ok_or_else(|| AppError::NotFound("This lesson has no planned topics yet.".into()))?;
        if revision != expected {
            return Err(AppError::InvalidState(ENTRY_CHANGED.into()));
        }
    }
    Ok(())
}

/// Replace a lesson's entry, keeping the old one in its history.
pub(super) async fn replace_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    topics: &PlannedLesson,
) -> Result<()> {
    replace_entry(
        connection,
        program_id,
        outline_revision,
        &topics.lesson_id,
        &serde_json::to_string(topics)?,
        now(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curriculum::LearningGenerationJobKind;
    use crate::dto::AcceptLearningProgramRequestDto;
    use crate::plan_dto::StartLearningGenerationJobRequestDto;
    use crate::repository::LearningRepository;

    #[tokio::test]
    async fn a_queued_preparation_locks_its_lesson() -> Result<()> {
        let pool = crate::tests::pool().await?;
        let programs = LearningRepository::new(pool.clone());
        let fixture = crate::tests::fixture();
        programs.create(&fixture).await?;
        programs
            .accept(&AcceptLearningProgramRequestDto {
                program_id: fixture.summary.id.clone(),
                expected_revision: 0,
                title: fixture.summary.title.clone(),
            })
            .await?;
        let program = programs.get(&fixture.summary.id).await?;
        let lesson = program.modules[0].lessons[0].id.clone();
        let job = LearningCurriculumRepository::new(pool.clone())
            .start_job(&StartLearningGenerationJobRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
                kind: LearningGenerationJobKind::LessonPreparation,
                request_json: serde_json::json!({"lessonIds":[lesson]}).to_string(),
                progress_total: 1,
            })
            .await?;
        assert_eq!(
            job.status,
            crate::curriculum::LearningGenerationJobStatus::Pending
        );
        let mut connection = pool.acquire().await.map_err(db)?;
        let locked = locked_lessons(&mut connection, &program.summary.id).await?;
        assert!(locked.contains(&lesson), "{locked:?}");
        Ok(())
    }
}
