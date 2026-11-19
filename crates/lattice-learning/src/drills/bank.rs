//! The bank: one drill ready on each shelf, a course's drills of one material, so that starting a drill is instant.
//! Taking the last one queues the build of the lesson next up, so the one ahead is built on the newest schedules.

use super::repository::StoredDrill;
use super::schedules;
use super::spec::DrillSpec;
use crate::persistence::now;
use lattice_core::domain::learning::drills::Material;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;

mod store;
pub use store::queue_build_in;
use store::{next_lesson_in, spec_in, under_way_in};
pub(crate) use store::{next_up_in, take_in};

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
pub async fn queue_build(database: &Database, spec: &DrillSpec, seed: &str) -> Result<String> {
    let mut transaction = database.begin_write().await?;
    let id = queue_build_in(&mut transaction, spec, seed).await?;
    transaction.commit().await?;
    Ok(id)
}

/// The id of the build of `shelf` under way for `lesson`, or for any lesson when `lesson` is `None`; or, when there is
/// none, a new one queued: from `lesson`, or from the lesson next up, with `lines` and `count` patterns from the
/// schedule. A learner who finds the shelf empty is told of the build already queued rather than getting another
/// each time they ask. Refused when no lesson is asked for and none of the course's can be drilled on the material.
/// The caller tells the job runtime once the job is committed.
pub async fn ensure_build(
    database: &Database,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
    lines: u32,
    count: usize,
) -> Result<String> {
    let mut transaction = database.begin_write().await?;
    if let Some(found) = under_way_in(&mut transaction, shelf, lesson).await? {
        transaction.commit().await?;
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
    transaction.commit().await?;
    Ok(id)
}

/// What a start of a shelf with no lesson asked for comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NextUp {
    pub lesson_id: String,
    pub status: NextStatus,
}

/// Whether the lesson a start with no lesson draws on has a drill ready, one being built, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NextStatus {
    /// A drill of it is ready, and the start opens an attempt on it.
    Ready,
    /// A build of it is under way, and the start waits for it.
    Building,
    /// The start queues its build.
    ToBuild,
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
    database: &Database,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
) -> Result<Option<Taken>> {
    let mut transaction = database.begin_write().await?;
    let taken = take_in(&mut transaction, shelf, lesson).await?;
    transaction.commit().await?;
    Ok(taken)
}
