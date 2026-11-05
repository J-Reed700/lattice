//! The bank: one drill ready per language, and one on a passage per course, so that starting a drill is instant.
//! Taking the ready drill queues the build of the next, so the one ahead is built on the newest schedule.

use super::build::STEPS;
use super::jobs::{spec_of, DRILL_BUILD};
use super::repository::{self, DrillStatus, StoredDrill};
use super::spec::DrillSpec;
use crate::features::learning::persistence::{db, hash, now};
use lattice_core::domain::learning::drills::{targets, Material};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore, NewJob};
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};

/// Realistic areas of backend and frontend code that a drill can be about. A drill without a domain of its own takes
/// the one of these used least recently for its language.
pub const DOMAINS: &[&str] = &[
    "payments and refunds",
    "inventory and stock reservations",
    "user sessions and login",
    "rate limiting",
    "file uploads",
    "notifications and email",
    "search and filtering",
    "reporting and CSV export",
    "job scheduling",
    "caching",
    "feature flags",
    "webhook delivery",
    "shopping cart and checkout",
    "order fulfilment",
    "audit logging",
    "pagination",
    "roles and permissions",
    "invoicing",
    "subscriptions and billing",
    "data import",
    "configuration loading",
    "form validation in a user interface",
    "state management in a user interface",
    "data tables with sorting and filtering",
    "API clients with retries",
    "message queue consumers",
    "analytics events",
    "image thumbnails",
    "booking and availability",
    "leaderboards",
    "chat messages",
    "URL shortening",
    "password reset",
    "API keys and tokens",
    "background workers",
    "data migrations",
    "currency conversion",
    "dates and time zones",
];

/// The domain used least recently: one never used first, then the one whose latest drill is oldest, ties in the
/// order of [`DOMAINS`]. `used` holds, for each domain a drill was built for, when the latest was.
pub fn least_recently_used(used: &[(String, i64)]) -> &'static str {
    DOMAINS
        .iter()
        .copied()
        .min_by_key(|domain| {
            used.iter()
                .filter(|(name, _)| name == domain)
                .map(|(_, at)| *at)
                .max()
        })
        .unwrap_or("backend services")
}

/// What the bank keeps one ready drill of: code in one language, or passages of one course.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf<'a> {
    Language(&'a str),
    /// The course, by its id.
    Course(&'a str),
}

impl<'a> Shelf<'a> {
    /// The shelf a drill built for `spec` goes on.
    pub fn of(spec: &'a DrillSpec) -> Result<Self> {
        match &spec.material {
            Material::Code { language } => Ok(Self::Language(language)),
            Material::Passage => spec.course().map(Self::Course).ok_or_else(|| {
                AppError::InvalidInput(
                    "A drill on a passage names the course whose sources it is taken from.".into(),
                )
            }),
        }
    }

    /// The subject of the shelf's build jobs, which run one at a time: the language, or the course after
    /// `passage:`.
    pub fn subject(self) -> String {
        match self {
            Self::Language(language) => language.to_owned(),
            Self::Course(program_id) => format!("passage:{program_id}"),
        }
    }
}

/// The domain the next drill of `language` is about.
pub async fn next_domain(pool: &SqlitePool, language: &str) -> Result<String> {
    let used = repository::domain_uses(pool, language).await?;
    Ok(least_recently_used(&used).to_owned())
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

/// Whether two topics, or the lack of one, are the same: case and the space around them do not matter.
fn same_topic(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.trim().to_lowercase() == right.trim().to_lowercase(),
        _ => false,
    }
}

/// The id of the build of `spec`'s shelf that is under way: the oldest pending or running one, or, when there is
/// none, a new one queued for `spec`. A learner who finds the bank empty is told of the build already queued rather
/// than getting another each time they ask. The caller tells the job runtime once the job is committed.
pub async fn ensure_build(pool: &SqlitePool, spec: &DrillSpec) -> Result<String> {
    spec.validate()?;
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let subject = Shelf::of(spec)?.subject();
    let under_way = JobStore::list_for_subject_in(&mut transaction, DRILL_BUILD, &subject)
        .await?
        .into_iter()
        .rev()
        .find(|job| {
            matches!(job.status, JobStatus::Pending | JobStatus::Running)
                && spec_of(job)
                    .is_ok_and(|found| same_topic(found.domain.as_deref(), spec.domain.as_deref()))
        });
    let id = match under_way {
        Some(job) => job.id,
        None => {
            let seed = format!("start:{}", uuid::Uuid::new_v4());
            queue_build_in(&mut transaction, spec, &seed).await?
        }
    };
    transaction.commit().await.map_err(db)?;
    Ok(id)
}

/// A drill given to a learner, and the build queued to replace it.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    pub drill: StoredDrill,
    /// The job that builds the next drill of the shelf. The caller tells the job runtime.
    pub next_build: String,
}

/// Gives out the oldest ready drill of `language`, marked taken, and queues the build of the next for the language
/// at once, in the same transaction: with the taken drill's size and number of patterns, and targets from
/// `drills::targets`. `None` when no drill of the language is ready, and nothing is queued.
///
/// The learner's pattern states arrive with the pattern schedule (step 5). Until then none are passed, so every
/// pattern counts as never practised and each build targets the same patterns.
pub async fn take(pool: &SqlitePool, language: &str) -> Result<Option<Taken>> {
    take_from(pool, Shelf::Language(language)).await
}

/// [`take`] for the drills of `shelf`: on a passage of a course, the next is built for the same course.
pub async fn take_from(pool: &SqlitePool, shelf: Shelf<'_>) -> Result<Option<Taken>> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let taken = take_in(&mut transaction, shelf).await?;
    transaction.commit().await.map_err(db)?;
    Ok(taken)
}

/// [`take_from`] inside the caller's transaction: what it took and queued is the caller's to commit.
pub(in crate::features::learning) async fn take_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
) -> Result<Option<Taken>> {
    take_topic_in(connection, shelf, None).await
}

/// [`take_in`] for a drill on a passage built for `topic`, or for no topic when there is none: a drill built for
/// another topic stays on the shelf. A shelf of code has no topics.
pub(in crate::features::learning) async fn take_topic_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    topic: Option<&str>,
) -> Result<Option<Taken>> {
    let ready = match shelf {
        Shelf::Language(language) => repository::oldest_ready_in(connection, language).await?,
        Shelf::Course(program_id) => {
            repository::oldest_ready_of_course_topic_in(connection, program_id, topic).await?
        }
    };
    let Some(mut drill) = ready else {
        return Ok(None);
    };
    let at = now();
    if !repository::mark_taken_in(connection, &drill.drill.id, at).await? {
        return Ok(None);
    }
    drill.status = DrillStatus::Taken;
    drill.taken_at = Some(at);
    let count = drill.spec.patterns.len().max(1);
    let material = drill.drill.material.clone();
    let next = DrillSpec {
        patterns: targets(&material, &[], at, count)
            .iter()
            .map(|pattern| pattern.id.to_owned())
            .collect(),
        material,
        domain: None,
        lines: drill.spec.lines,
        program_id: match shelf {
            Shelf::Language(_) => None,
            Shelf::Course(program_id) => Some(program_id.to_owned()),
        },
    };
    let next_build = queue_build_in(connection, &next, &format!("next:{}", drill.drill.id)).await?;
    Ok(Some(Taken { drill, next_build }))
}
