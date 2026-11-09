//! The two schedules of review drills (docs/design/2026-11-07-drills-in-the-course.md section 5): when each pattern
//! of a material and each lesson of a course comes back, and which material a lesson fits. Grading writes the
//! schedules in the transaction that records the grade, and the bank reads them.

use super::material::code;
use super::repository::StoredDrill;
use crate::persistence::{db, hash};
use crate::recall::schedule::CardSchedule;
use crate::recall::study_dto::StudyRating;
use lattice_core::domain::learning::drills::{
    per_pattern, IssueRating, LessonStanding, Material, PatternDue, PatternRating,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::json;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::collections::BTreeSet;

/// The columns of a schedule row, after its key.
const SCHEDULE_COLUMNS: &str =
    "stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses";

/// A table of schedules and the two columns that key a row.
struct Table {
    name: &'static str,
    keys: &'static str,
}

/// One schedule for each pattern of a material.
const PATTERNS: Table = Table {
    name: "learning_drill_pattern_schedules",
    keys: "material,pattern_id",
};

/// One schedule for each lesson of a course.
const LESSONS: Table = Table {
    name: "learning_drill_lesson_schedules",
    keys: "program_id,lesson_id",
};

/// The name a material's pattern schedules are kept under: `code:<language>`, or `passage`.
pub(crate) fn schedule_material(material: &Material) -> String {
    match material {
        Material::Code { language } => format!("code:{language}"),
        Material::Passage => "passage".to_owned(),
    }
}

/// The name a material is recorded under when a lesson is found to fit it or not: `code` or `passage`.
pub(crate) fn fit_material(material: &Material) -> &'static str {
    match material {
        Material::Code { .. } => "code",
        Material::Passage => "passage",
    }
}

fn study_rating(rating: PatternRating) -> StudyRating {
    match rating {
        PatternRating::Again => StudyRating::Again,
        PatternRating::Hard => StudyRating::Hard,
        PatternRating::Good => StudyRating::Good,
        PatternRating::Easy => StudyRating::Easy,
    }
}

fn schedule_of(row: &SqliteRow) -> CardSchedule {
    CardSchedule {
        stability: Some(row.get("stability")),
        difficulty: Some(row.get("difficulty")),
        last_reviewed_at: Some(row.get("last_reviewed_at")),
        due_at: row.get("due_at"),
        interval_days: row.get("interval_days"),
        review_count: row.get("review_count"),
        lapses: row.get("lapses"),
    }
}

/// The schedule at `key`, or one never reviewed when there is none.
async fn read_in(
    connection: &mut SqliteConnection,
    table: &Table,
    key: (&str, &str),
    now: i64,
) -> Result<CardSchedule> {
    let row = sqlx::query(&format!(
        "SELECT {SCHEDULE_COLUMNS} FROM {} WHERE ({}) = (?,?)",
        table.name, table.keys
    ))
    .bind(key.0)
    .bind(key.1)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(row.as_ref().map_or(
        CardSchedule {
            stability: None,
            difficulty: None,
            last_reviewed_at: None,
            due_at: now,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        },
        schedule_of,
    ))
}

/// Writes a reviewed schedule at `key`, replacing the one there.
async fn write_in(
    connection: &mut SqliteConnection,
    table: &Table,
    key: (&str, &str),
    schedule: &CardSchedule,
) -> Result<()> {
    let (Some(stability), Some(difficulty), Some(reviewed)) = (
        schedule.stability,
        schedule.difficulty,
        schedule.last_reviewed_at,
    ) else {
        return Err(AppError::InvalidState(
            "A reviewed schedule has no memory.".into(),
        ));
    };
    sqlx::query(&format!(
        "INSERT INTO {name}({keys},{SCHEDULE_COLUMNS}) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT({keys}) DO UPDATE SET stability=excluded.stability,difficulty=excluded.difficulty,last_reviewed_at=excluded.last_reviewed_at,due_at=excluded.due_at,interval_days=excluded.interval_days,review_count=excluded.review_count,lapses=excluded.lapses",
        name = table.name,
        keys = table.keys
    ))
    .bind(key.0)
    .bind(key.1)
    .bind(stability)
    .bind(difficulty)
    .bind(reviewed)
    .bind(schedule.due_at)
    .bind(schedule.interval_days)
    .bind(schedule.review_count)
    .bind(schedule.lapses)
    .execute(&mut *connection)
    .await
    .map(drop)
    .map_err(db)
}

/// Moves the schedule at `key` on by one review rated `rating` at `now`.
async fn review_in(
    connection: &mut SqliteConnection,
    table: &Table,
    key: (&str, &str),
    rating: PatternRating,
    now: i64,
) -> Result<()> {
    let before = read_in(connection, table, key, now).await?;
    let after = before.review(study_rating(rating), now, None)?;
    write_in(connection, table, key, &after).await
}

/// Moves the schedule of `pattern` under `material` on by one review rated `rating` at `now`, landing before `target`
/// while it is ahead, and returns the schedule it now has.
pub(crate) async fn review_pattern_in(
    connection: &mut SqliteConnection,
    material: &str,
    pattern: &str,
    rating: PatternRating,
    now: i64,
    target: Option<i64>,
) -> Result<CardSchedule> {
    let key = (material, pattern);
    let before = read_in(connection, &PATTERNS, key, now).await?;
    let after = before.review(study_rating(rating), now, target)?;
    write_in(connection, &PATTERNS, key, &after).await?;
    Ok(after)
}

/// Moves the schedule of each pattern `ratings` rate by one review, and the lesson's when `lesson` is rated and the
/// drill was built from one. Call inside the transaction that records the grade.
pub(crate) async fn record_in(
    connection: &mut SqliteConnection,
    drill: &StoredDrill,
    ratings: &[IssueRating],
    lesson: Option<PatternRating>,
    now: i64,
) -> Result<()> {
    let material = schedule_material(&drill.drill.material);
    for entry in per_pattern(ratings) {
        review_in(
            connection,
            &PATTERNS,
            (material.as_str(), entry.pattern.as_str()),
            entry.rating,
            now,
        )
        .await?;
    }
    if let (Some(rating), Some(program_id), Some(lesson_id)) =
        (lesson, &drill.program_id, &drill.lesson_id)
    {
        review_in(
            connection,
            &LESSONS,
            (program_id.as_str(), lesson_id.as_str()),
            rating,
            now,
        )
        .await?;
    }
    Ok(())
}

/// Where each pattern of `material` stands: those with a schedule, by pattern id, then those pending with none. A
/// pattern is pending when it is in `taking`, the patterns of the drill being taken, or when a drill of the material
/// that is ready and built from a lesson plants it, or a drill whose attempt is submitted and not yet graded does.
pub(crate) async fn pattern_states_in(
    connection: &mut SqliteConnection,
    material: &Material,
    taking: &[String],
) -> Result<Vec<PatternDue>> {
    let scheduled: Vec<(String, i64)> = sqlx::query_as(
        "SELECT pattern_id,due_at FROM learning_drill_pattern_schedules WHERE material=? ORDER BY pattern_id",
    )
    .bind(schedule_material(material))
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let held: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT p.pattern_id FROM learning_drill_patterns p JOIN learning_drills d ON d.id=p.drill_id WHERE d.material=?1 AND d.language IS ?2 AND ((d.status='ready' AND d.lesson_id IS NOT NULL) OR EXISTS(SELECT 1 FROM learning_drill_attempts a WHERE a.drill_id=d.id AND a.status='submitted')) ORDER BY p.pattern_id",
    )
    .bind(fit_material(material))
    .bind(material.language())
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let pending: BTreeSet<&str> = held.iter().chain(taking).map(String::as_str).collect();
    let mut states: Vec<PatternDue> = scheduled
        .iter()
        .map(|(pattern, due_at)| PatternDue {
            pattern: pattern.clone(),
            due_at: Some(*due_at),
            pending: pending.contains(pattern.as_str()),
        })
        .collect();
    for pattern in pending {
        if scheduled.iter().all(|(known, _)| known != pattern) {
            states.push(PatternDue {
                pattern: pattern.to_owned(),
                due_at: None,
                pending: true,
            });
        }
    }
    Ok(states)
}

/// Where each lesson of the course stands for the next drill of `material`, in course order. Lessons replaced or
/// skipped are left out. A lesson is pending when it is `taking`, or a drill of the course built from it, of any
/// material, is ready or waits for its grade.
pub(crate) async fn lesson_states_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    material: &Material,
    taking: Option<&str>,
) -> Result<Vec<LessonStanding>> {
    let lessons: Vec<(String, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT l.id,l.completed,COALESCE(l.id = p.current_lesson_id,0),s.due_at FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id JOIN learning_programs p ON p.id=l.program_id LEFT JOIN learning_drill_lesson_schedules s ON s.program_id=l.program_id AND s.lesson_id=l.id WHERE l.program_id=? AND l.curriculum_state NOT IN ('replaced','skipped') ORDER BY m.ordinal,l.ordinal",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let held: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT d.lesson_id FROM learning_drills d WHERE d.program_id=? AND d.lesson_id IS NOT NULL AND (d.status='ready' OR EXISTS(SELECT 1 FROM learning_drill_attempts a WHERE a.drill_id=d.id AND a.status='submitted'))",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let mut standings = Vec::with_capacity(lessons.len());
    for (lesson_id, completed, current, due_at) in lessons {
        let fits = fits_in(connection, program_id, &lesson_id, fit_material(material)).await?;
        standings.push(LessonStanding {
            completed: completed != 0,
            current: current != 0,
            due_at,
            pending: taking == Some(lesson_id.as_str()) || held.contains(&lesson_id),
            fits,
            lesson_id,
        });
    }
    Ok(standings)
}

/// Whether the course drills the lesson: it is one of the course's, and not replaced or skipped.
pub(crate) async fn drills_lesson_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=? AND id=? AND curriculum_state NOT IN ('replaced','skipped'))",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_one(connection)
    .await
    .map_err(db)
}

/// Whether the lesson can be drilled in `material`, as `lesson_states_in` says it.
pub(crate) async fn lesson_fits_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    material: &Material,
) -> Result<bool> {
    fits_in(connection, program_id, lesson_id, fit_material(material)).await
}

/// Whether the lesson can be drilled in `material` (`code` or `passage`): unless it is recorded as not fitting on the
/// basis it has now, a passage by the fit table and code by the code fit check.
async fn fits_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    material: &str,
) -> Result<bool> {
    if material == "code" {
        return code_fits_in(connection, program_id, lesson_id).await;
    }
    let recorded: Option<(i64, String)> = sqlx::query_as(
        "SELECT fits,basis FROM learning_drill_lesson_fits WHERE program_id=? AND lesson_id=? AND material=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(material)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    match recorded {
        Some((0, basis)) => Ok(basis != fit_basis_in(connection, program_id, lesson_id).await?),
        _ => Ok(true),
    }
}

/// Whether the lesson can be drilled in code: unless the code fit check turned it down on the basis it has now.
async fn code_fits_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<bool> {
    let facts: Option<(String, String, String)> = sqlx::query_as(
        "SELECT m.title,l.title,l.objective FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.id=? AND l.program_id=?",
    )
    .bind(lesson_id)
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    let Some((module, lesson, objective)) = facts else {
        return Ok(true);
    };
    let basis = code::basis_of(&module, &lesson, &objective)?;
    let turned_down: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM learning_drill_code_checks WHERE program_id=? AND lesson_id=? AND basis=? AND teaches=0)",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(&basis)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    Ok(!turned_down)
}

/// What a decision that the lesson fits a material rests on: a hash of the lesson's blocks and of the course's active
/// source versions. A record made on another basis is ignored.
pub(crate) async fn fit_basis_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<String> {
    let blocks: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT ordinal,kind,title,body,source_ids_json FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
    )
    .bind(lesson_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let sources: Vec<String> = sqlx::query_scalar(
        "SELECT s.active_version_id FROM learning_source_library s JOIN learning_source_versions v ON v.program_id=s.program_id AND v.id=s.active_version_id WHERE s.program_id=? AND s.deleted_at IS NULL AND v.not_a_source IS NULL ORDER BY s.active_version_id",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    hash(&json!({"blocks": blocks, "sources": sources}))
}

/// Records whether the lesson fits `material`, and the basis the decision rests on, replacing an earlier record;
/// nothing, when the lesson is no longer one of the course's.
pub(crate) async fn record_fit_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    material: &Material,
    fits: bool,
    basis: &str,
    now: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_drill_lesson_fits(program_id,lesson_id,material,fits,basis,decided_at) SELECT ?1,?2,?3,?4,?5,?6 WHERE EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=?1 AND id=?2) ON CONFLICT(program_id,lesson_id,material) DO UPDATE SET fits=excluded.fits,basis=excluded.basis,decided_at=excluded.decided_at")
        .bind(program_id)
        .bind(lesson_id)
        .bind(fit_material(material))
        .bind(i64::from(fits))
        .bind(basis)
        .bind(now)
        .execute(&mut *connection)
        .await
        .map(drop)
        .map_err(db)
}
