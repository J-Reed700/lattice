//! The bank's unit-of-work functions: they take and queue drills on the caller's connection, and commit nothing.

use super::{NextStatus, NextUp, Shelf, Taken};
use crate::drills::build::STEPS;
use crate::drills::jobs::{spec_of, DRILL_BUILD};
use crate::drills::repository::{self, DrillStatus};
use crate::drills::schedules;
use crate::drills::spec::DrillSpec;
use crate::persistence::{hash, now};
use lattice_core::domain::learning::drills::{next_lessons, targets};
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore, NewJob};
use serde_json::json;
use sqlx::SqliteConnection;

/// Queues the drill build inside the caller's transaction; `queue_build` is the unit of work of its own.
pub async fn queue_build_in(
    connection: &mut SqliteConnection,
    spec: &DrillSpec,
    seed: &str,
) -> Result<String> {
    spec.validate()?;
    let requested = json!({"spec": spec});
    let payload_hash = hash(&requested)?;
    let admitted = JobStore::submit_in(
        connection,
        &NewJob {
            kind: DRILL_BUILD.into(),
            subject_id: Some(Shelf::of(spec)?.subject()),
            operation_id: uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_OID,
                format!("{DRILL_BUILD}:{seed}").as_bytes(),
            )
            .to_string(),
            payload_hash,
            requested,
            progress_total: STEPS,
            message: "Waiting to build a drill".into(),
        },
    )
    .await?;
    Ok(admitted.id)
}

/// A build of a shelf under way.
pub(super) struct UnderWay {
    pub(super) job_id: String,
    /// The lesson it builds from.
    pub(super) lesson_id: String,
}

/// The build of `shelf` under way: the oldest pending or running one whose spec reads and names a lesson, built from
/// `lesson`, or from any lesson when `lesson` is `None`.
pub(super) async fn under_way_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
) -> Result<Option<UnderWay>> {
    let jobs = JobStore::list_for_subject_in(connection, DRILL_BUILD, &shelf.subject()).await?;
    Ok(jobs.into_iter().rev().find_map(|job| {
        if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
            return None;
        }
        let lesson_id = spec_of(&job).ok()?.lesson_id?;
        if lesson.is_some_and(|wanted| wanted != lesson_id) {
            return None;
        }
        Some(UnderWay {
            job_id: job.id,
            lesson_id,
        })
    }))
}

/// The lesson of `shelf`'s course next up for a drill on its material, `taking` (the lesson of a drill being taken)
/// counted as pending; `None` when no lesson of the course can be drilled on it.
pub(super) async fn next_lesson_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    taking: Option<&str>,
    now: i64,
) -> Result<Option<String>> {
    let states =
        schedules::lesson_states_in(connection, shelf.program_id, shelf.material, taking).await?;
    Ok(next_lessons(&states, now)
        .first()
        .map(|lesson| (*lesson).to_owned()))
}

/// The spec of a drill of `shelf` built from `lesson_id`: `lines`, and `count` patterns from the pattern schedule with
/// `taking` held back.
pub(super) async fn spec_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    lesson_id: String,
    lines: u32,
    count: usize,
    taking: &[String],
    now: i64,
) -> Result<DrillSpec> {
    let states = schedules::pattern_states_in(connection, shelf.material, taking).await?;
    Ok(DrillSpec {
        material: shelf.material.clone(),
        lines,
        patterns: targets(shelf.material, &states, now, count)
            .iter()
            .map(|pattern| pattern.id.to_owned())
            .collect(),
        program_id: Some(shelf.program_id.to_owned()),
        lesson_id: Some(lesson_id),
    })
}

/// What a start of `shelf` with no lesson asked for comes to at `now`, tried in the order the start tries them: the
/// oldest ready drill of the shelf, then the build under way, then the lesson next up. `None` when no lesson of the
/// course can be drilled on the shelf's material.
pub(crate) async fn next_up_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    now: i64,
) -> Result<Option<NextUp>> {
    if let Some(lesson_id) = repository::oldest_ready_on(connection, shelf, None)
        .await?
        .and_then(|drill| drill.lesson_id)
    {
        return Ok(Some(NextUp {
            lesson_id,
            status: NextStatus::Ready,
        }));
    }
    if let Some(found) = under_way_in(connection, shelf, None).await? {
        return Ok(Some(NextUp {
            lesson_id: found.lesson_id,
            status: NextStatus::Building,
        }));
    }
    Ok(next_lesson_in(connection, shelf, None, now)
        .await?
        .map(|lesson_id| NextUp {
            lesson_id,
            status: NextStatus::ToBuild,
        }))
}

/// `take` inside the caller's transaction: what it took and queued is the caller's to commit.
pub(crate) async fn take_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
) -> Result<Option<Taken>> {
    let Some(mut drill) = repository::oldest_ready_on(connection, shelf, lesson).await? else {
        return Ok(None);
    };
    let at = now();
    if !repository::mark_taken_in(connection, &drill.drill.id, at).await? {
        return Ok(None);
    }
    drill.status = DrillStatus::Taken;
    drill.taken_at = Some(at);
    if repository::any_ready_in(connection, shelf).await?
        || under_way_in(connection, shelf, None).await?.is_some()
    {
        return Ok(Some(Taken {
            drill,
            next_build: None,
        }));
    }
    let Some(next) = next_lesson_in(connection, shelf, drill.lesson_id.as_deref(), at).await?
    else {
        return Ok(Some(Taken {
            drill,
            next_build: None,
        }));
    };
    let mut taking: Vec<String> = Vec::new();
    for issue in &drill.drill.key {
        if !taking.contains(&issue.pattern) {
            taking.push(issue.pattern.clone());
        }
    }
    let spec = spec_in(
        connection,
        shelf,
        next,
        drill.spec.lines,
        drill.spec.patterns.len().max(1),
        &taking,
        at,
    )
    .await?;
    let next_build = queue_build_in(connection, &spec, &format!("next:{}", drill.drill.id)).await?;
    Ok(Some(Taken {
        drill,
        next_build: Some(next_build),
    }))
}
