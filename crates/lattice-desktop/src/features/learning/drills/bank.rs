//! The bank: one drill ready per language, and one on a passage per course, so that starting a drill is instant.
//! Taking the ready drill queues the build of the next, so the one ahead is built on the newest schedule.

use super::build::STEPS;
use super::jobs::DRILL_BUILD;
use super::repository::StoredDrill;
use super::spec::DrillSpec;
use crate::features::learning::persistence::{db, hash};
use lattice_core::domain::learning::drills::Material;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{JobStore, NewJob};
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
        Ok(Self {
            program_id: "",
            material: &spec.material,
        })
    }

    /// The subject of the shelf's build jobs, which run one at a time: `drill:` then the course and the material as
    /// its pattern schedules are named, such as `drill:{program_id}:code:python` or `drill:{program_id}:passage`.
    pub fn subject(self) -> String {
        String::new()
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

/// The id of the build of `shelf` under way for `lesson`, or for any lesson when `lesson` is `None`; or, when there is
/// none, a new one queued: from `lesson`, or from the lesson next up, with `lines` and `count` patterns from the
/// schedule. A learner who finds the shelf empty is told of the build already queued rather than getting another
/// each time they ask. Refused when no lesson is asked for and none of the course's can be drilled on the material.
/// The caller tells the job runtime once the job is committed.
pub async fn ensure_build(
    _pool: &SqlitePool,
    _shelf: Shelf<'_>,
    _lesson: Option<&str>,
    _lines: u32,
    _count: usize,
) -> Result<String> {
    Ok(String::new())
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
    _pool: &SqlitePool,
    _shelf: Shelf<'_>,
    _lesson: Option<&str>,
) -> Result<Option<Taken>> {
    Ok(None)
}

/// [`take`] inside the caller's transaction: what it took and queued is the caller's to commit.
pub(in crate::features::learning) async fn take_in(
    _connection: &mut SqliteConnection,
    _shelf: Shelf<'_>,
    _lesson: Option<&str>,
) -> Result<Option<Taken>> {
    Ok(None)
}
