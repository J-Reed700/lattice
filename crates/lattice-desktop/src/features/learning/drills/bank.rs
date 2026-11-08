//! The bank: one drill ready on each shelf, a course's drills of one material, so that starting a drill is instant.
//! Taking the last one queues the build of the lesson next up, so the one ahead is built on the newest schedules.

use super::build::STEPS;
use super::jobs::{spec_of, DRILL_BUILD};
use super::repository::{self, DrillStatus, StoredDrill};
use super::schedules;
use super::spec::DrillSpec;
use crate::features::learning::persistence::{db, hash, now};
use lattice_core::domain::learning::drills::{next_lessons, targets, Material};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore, NewJob};
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};

/// What the bank keeps one ready drill of: a course's drills of one material, code in a language or passages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shelf<'a> {
    pub program_id: &'a str,
    pub material: &'a Material,
}

impl<'a> Shelf<'a> {
    /// The shelf a drill built for `spec` goes on: its course's, of its material.
    pub fn of(spec: &'a DrillSpec) -> Result<Self> {
        let program_id = spec.course().ok_or_else(|| {
            AppError::InvalidInput("A drill names the course it is built for.".into())
        })?;
        Ok(Self {
            program_id,
            material: &spec.material,
        })
    }

    /// The subject of the shelf's build jobs, which run one at a time: `drill:` then the course and the material as
    /// its pattern schedules are named, such as `drill:{program_id}:code:python` or `drill:{program_id}:passage`.
    pub fn subject(self) -> String {
        format!(
            "drill:{}:{}",
            self.program_id,
            schedules::schedule_material(self.material)
        )
    }
}

/// Queues a `learning.drill_build` job for `spec` and returns its id. The operation ID derives from `seed`, so
/// asking again with the same seed finds the same job, and the same seed with another spec is refused. The caller
/// tells the job runtime once the job is committed.
pub async fn queue_build(pool: &SqlitePool, spec: &DrillSpec, seed: &str) -> Result<String> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let id = queue_build_in(&mut transaction, spec, seed).await?;
    transaction.commit().await.map_err(db)?;
    Ok(id)
}

/// [`queue_build`] inside the caller's transaction.
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
struct UnderWay {
    job_id: String,
    /// The lesson it builds from.
    lesson_id: String,
}

/// The build of `shelf` under way: the oldest pending or running one whose spec reads and names a lesson, built from
/// `lesson`, or from any lesson when `lesson` is `None`.
async fn under_way_in(
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
async fn next_lesson_in(
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
async fn spec_in(
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

/// The id of the build of `shelf` under way for `lesson`, or for any lesson when `lesson` is `None`; or, when there is
/// none, a new one queued: from `lesson`, or from the lesson next up, with `lines` and `count` patterns from the
/// schedule. A learner who finds the shelf empty is told of the build already queued rather than getting another
/// each time they ask. Refused when no lesson is asked for and none of the course's can be drilled on the material.
/// The caller tells the job runtime once the job is committed.
pub async fn ensure_build(
    pool: &SqlitePool,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
    lines: u32,
    count: usize,
) -> Result<String> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    if let Some(found) = under_way_in(&mut transaction, shelf, lesson).await? {
        transaction.commit().await.map_err(db)?;
        return Ok(found.job_id);
    }
    let at = now();
    let lesson_id = match lesson {
        Some(lesson) => lesson.to_owned(),
        None => next_lesson_in(&mut transaction, shelf, None, at)
            .await?
            .ok_or_else(|| {
                AppError::InvalidInput(match shelf.material {
                    Material::Code { .. } => {
                        "None of the course's lessons can be drilled in code.".into()
                    }
                    Material::Passage => {
                        "None of the course's lessons can be drilled on a passage of its sources."
                            .into()
                    }
                })
            })?,
    };
    let spec = spec_in(&mut transaction, shelf, lesson_id, lines, count, &[], at).await?;
    let seed = format!("start:{}", uuid::Uuid::new_v4());
    let id = queue_build_in(&mut transaction, &spec, &seed).await?;
    transaction.commit().await.map_err(db)?;
    Ok(id)
}

/// What a start of a shelf with no lesson asked for comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::learning) struct NextUp {
    pub lesson_id: String,
    pub status: NextStatus,
}

/// Whether the lesson a start with no lesson draws on has a drill ready, one being built, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::learning) enum NextStatus {
    /// A drill of it is ready, and the start opens an attempt on it.
    Ready,
    /// A build of it is under way, and the start waits for it.
    Building,
    /// The start queues its build.
    ToBuild,
}

/// What a start of `shelf` with no lesson asked for comes to at `now`, tried in the order the start tries them: the
/// oldest ready drill of the shelf, then the build under way, then the lesson next up. `None` when no lesson of the
/// course can be drilled on the shelf's material.
pub(in crate::features::learning) async fn next_up_in(
    _connection: &mut SqliteConnection,
    _shelf: Shelf<'_>,
    _now: i64,
) -> Result<Option<NextUp>> {
    Ok(None)
}

/// A drill given to a learner, and the build queued to follow it, if one was.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    pub drill: StoredDrill,
    /// The job that builds the next drill of the shelf, when one was queued. The caller tells the job runtime.
    pub next_build: Option<String>,
}

/// Gives out the oldest ready drill of `shelf` built from a lesson, from `lesson` when one is asked for, marked
/// taken. When no other drill of the shelf is ready and no build of the shelf is pending or running, the build of
/// the next is queued in the same transaction: from the lesson next up, with the taken drill's size and number of
/// patterns, and targets from `drills::targets`. `None` when no such drill is ready, and nothing is queued.
///
/// The lesson and the patterns come from the schedules: the taken drill's lesson and patterns, and those of drills
/// ready or waiting for their grade, are pending, so they come last.
pub async fn take(
    pool: &SqlitePool,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
) -> Result<Option<Taken>> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let taken = take_in(&mut transaction, shelf, lesson).await?;
    transaction.commit().await.map_err(db)?;
    Ok(taken)
}

/// [`take`] inside the caller's transaction: what it took and queued is the caller's to commit.
pub(in crate::features::learning) async fn take_in(
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
