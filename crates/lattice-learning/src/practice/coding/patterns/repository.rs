//! The patterns' schedules and what is due, read and written on the caller's connection.

use super::{family_name, material_of, plural, problem_material, Standing};
use crate::drills::schedules;
use crate::persistence::db;
use crate::practice::coding::dto::{DueItemDto, DueKind, PatternRatingDto, PracticeDueDto};
use lattice_core::domain::learning::drills::{Material, PatternRating};
use lattice_core::domain::learning::practice::{problem_pattern, ProblemPattern, PROBLEM_PATTERNS};
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;

/// Moves the schedule of each of `pattern_ids` on by one review rated `rating` at `now`, landing before `target`
/// (the interview) while it is ahead. Ids the catalogue no longer holds are left out.
pub async fn record_ratings_in(
    connection: &mut SqliteConnection,
    _program_id: &str,
    language: &str,
    pattern_ids: &[String],
    rating: PatternRating,
    now: i64,
    target: Option<i64>,
) -> Result<Vec<PatternRatingDto>> {
    let material = problem_material(language);
    let mut rated = Vec::with_capacity(pattern_ids.len());
    for id in pattern_ids {
        let Some(pattern) = problem_pattern(id) else {
            continue;
        };
        let schedule =
            schedules::review_pattern_in(connection, &material, pattern.id, rating, now, target)
                .await?;
        rated.push(PatternRatingDto {
            pattern_id: pattern.id.to_owned(),
            name: pattern.name.to_owned(),
            family: family_name(pattern),
            cue: pattern.cue.to_owned(),
            rating,
            due_at: schedule.due_at,
        });
    }
    Ok(rated)
}

/// Pattern ids ranked for the next build: those due, most overdue first; then those never practised, in the
/// catalogue's order; then the rest, soonest due first. Each pick is the first of what is left whose family has not
/// been picked yet; when every family left has been picked, the families start over.
pub async fn targets_in(
    connection: &mut SqliteConnection,
    _program_id: &str,
    language: &str,
    now: i64,
) -> Result<Vec<String>> {
    let scheduled: Vec<(String, i64)> = sqlx::query_as(
        "SELECT pattern_id,due_at FROM learning_drill_pattern_schedules WHERE material=?",
    )
    .bind(problem_material(language))
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let mut ranked: Vec<(Standing, Option<i64>, usize, &'static ProblemPattern)> = PROBLEM_PATTERNS
        .iter()
        .enumerate()
        .map(|(order, pattern)| {
            let due_at = scheduled
                .iter()
                .find(|(id, _)| id == pattern.id)
                .map(|(_, due_at)| *due_at);
            let standing = match due_at {
                None => Standing::New,
                Some(at) if at <= now => Standing::Due,
                Some(_) => Standing::Rested,
            };
            (standing, due_at, order, pattern)
        })
        .collect();
    ranked.sort_by_key(|&(standing, due_at, order, _)| (standing, due_at, order));
    let mut left: Vec<&'static ProblemPattern> = ranked
        .into_iter()
        .map(|(_, _, _, pattern)| pattern)
        .collect();
    let mut chosen: Vec<String> = Vec::with_capacity(left.len());
    let mut picked_families: Vec<&'static ProblemPattern> = Vec::new();
    while !left.is_empty() {
        let next = left.iter().position(|candidate| {
            picked_families
                .iter()
                .all(|picked| picked.family != candidate.family)
        });
        let at = match next {
            Some(at) => at,
            None => {
                picked_families.clear();
                0
            }
        };
        let picked = left.remove(at);
        chosen.push(picked.id.to_owned());
        picked_families.push(picked);
    }
    Ok(chosen)
}

/// What is due at `now` for the course: problem patterns (one item, never naming a pattern), the lessons the
/// drills have scheduled, and the drills' patterns, most overdue first. Coding items (problem patterns and the
/// drills' code patterns) are listed only when `code_fits`: patterns are the learner's across courses, and a course
/// where code does not fit never gets code practice.
pub async fn practice_due_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    now: i64,
    code_fits: bool,
) -> Result<PracticeDueDto> {
    let mut items: Vec<DueItemDto> = Vec::new();

    let (problem_due, problem_first): (i64, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(*),MIN(due_at) FROM learning_drill_pattern_schedules WHERE material LIKE 'problem:%' AND due_at<=?",
    )
    .bind(now)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    if let (true, Ok(count @ 1..), Some(due_at)) =
        (code_fits, u32::try_from(problem_due), problem_first)
    {
        items.push(DueItemDto {
            kind: DueKind::CodingPatterns,
            label: plural(count, "problem pattern", "problem patterns"),
            count,
            due_at,
            lesson_id: None,
            material: None,
        });
    }

    let standings =
        schedules::lesson_states_in(connection, program_id, &Material::Passage, None).await?;
    for standing in standings {
        let Some(due_at) = standing.due_at.filter(|at| *at <= now) else {
            continue;
        };
        if standing.pending {
            continue;
        }
        let title: Option<String> =
            sqlx::query_scalar("SELECT title FROM learning_lessons WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(&standing.lesson_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(db)?;
        let Some(title) = title else {
            continue;
        };
        items.push(DueItemDto {
            kind: DueKind::DrillLesson,
            label: title,
            count: 1,
            due_at,
            lesson_id: Some(standing.lesson_id),
            material: None,
        });
    }

    let names: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT material FROM learning_drill_pattern_schedules WHERE material NOT LIKE 'problem:%' ORDER BY material",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    for name in names {
        let Some(material) = material_of(&name) else {
            continue;
        };
        if !code_fits && material.language().is_some() {
            continue;
        }
        let states = schedules::pattern_states_in(connection, &material, &[]).await?;
        let due: Vec<i64> = states
            .iter()
            .filter(|state| !state.pending)
            .filter_map(|state| state.due_at)
            .filter(|at| *at <= now)
            .collect();
        let (Some(first), Ok(count @ 1..)) = (due.iter().copied().min(), u32::try_from(due.len()))
        else {
            continue;
        };
        let what = material.language().unwrap_or("passage");
        items.push(DueItemDto {
            kind: DueKind::DrillPatterns,
            label: plural(
                count,
                &format!("{what} pattern"),
                &format!("{what} patterns"),
            ),
            count,
            due_at: first,
            lesson_id: None,
            material: Some(name),
        });
    }

    items.sort_by_key(|item| item.due_at);
    Ok(PracticeDueDto { items })
}
