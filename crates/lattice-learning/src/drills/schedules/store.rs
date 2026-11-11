//! The schedules' unit-of-work functions: they read and write the schedules on the caller's connection, and commit
//! nothing.

use super::repository::{self, Memory, Table, LESSONS, PATTERNS};
use super::{fit_material, schedule_material, study_rating};
use crate::drills::material::code;
use crate::drills::repository::StoredDrill;
use crate::persistence::{db, hash};
use crate::recall::schedule::CardSchedule;
use lattice_core::domain::learning::drills::{
    per_pattern, IssueRating, LessonStanding, Material, PatternDue, PatternRating,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::driver::DriverPool;
use serde_json::json;
use sqlx::SqliteConnection;
use std::collections::BTreeSet;

/// The schedule at `key`, or one never reviewed when there is none.
async fn read_in(
    connection: &mut SqliteConnection,
    table: &Table,
    key: (&str, &str),
    now: i64,
) -> Result<CardSchedule> {
    Ok(repository::schedule_in(connection, table, key)
        .await?
        .unwrap_or(CardSchedule {
            stability: None,
            difficulty: None,
            last_reviewed_at: None,
            due_at: now,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        }))
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
    let memory = Memory {
        stability,
        difficulty,
        reviewed,
    };
    repository::write_schedule_in(connection, table, key, &memory, schedule).await
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
    let scheduled =
        repository::scheduled_patterns_in(connection, &schedule_material(material)).await?;
    let held =
        repository::held_patterns_in(connection, fit_material(material), material.language())
            .await?;
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
    let lessons = repository::lesson_rows_in(connection, program_id).await?;
    let held = repository::held_lessons_in(connection, program_id).await?;
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
    let recorded = repository::recorded_fit_in(connection, program_id, lesson_id, material).await?;
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
    let facts = repository::lesson_facts_in(connection, program_id, lesson_id).await?;
    let Some((module, lesson, objective)) = facts else {
        return Ok(true);
    };
    let basis = code::basis_of(&module, &lesson, &objective)?;
    let turned_down =
        repository::code_turned_down_in(connection, program_id, lesson_id, &basis).await?;
    Ok(!turned_down)
}

/// What a decision that the lesson fits a material rests on: a hash of the lesson's blocks and of the course's active
/// source versions. A record made on another basis is ignored.
pub(crate) async fn fit_basis_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<String> {
    let blocks = repository::blocks_in(connection, lesson_id).await?;
    let sources = repository::active_source_versions_in(connection, program_id).await?;
    hash(&json!({"blocks": blocks, "sources": sources}))
}

/// [`fit_basis_in`] as a unit of work of its own.
pub(crate) async fn fit_basis(
    pool: &impl DriverPool,
    program_id: &str,
    lesson_id: &str,
) -> Result<String> {
    let mut tx = pool.begin_read().await.map_err(db)?;
    let basis = fit_basis_in(&mut tx, program_id, lesson_id).await?;
    tx.commit().await.map_err(db)?;
    Ok(basis)
}

/// [`record_fit_in`] as a unit of work of its own.
pub(crate) async fn record_fit(
    pool: &impl DriverPool,
    program_id: &str,
    lesson_id: &str,
    material: &Material,
    fits: bool,
    basis: &str,
    now: i64,
) -> Result<()> {
    let mut tx = pool.begin_write().await.map_err(db)?;
    record_fit_in(&mut tx, program_id, lesson_id, material, fits, basis, now).await?;
    tx.commit().await.map_err(db)
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
    repository::record_fit_in(
        connection,
        program_id,
        lesson_id,
        fit_material(material),
        fits,
        basis,
        now,
    )
    .await
}
