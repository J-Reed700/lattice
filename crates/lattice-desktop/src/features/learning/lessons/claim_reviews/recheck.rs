//! A source the user adds on the Sources tab, checked for the course's
//! waiting claims (UI design §3.6, §5.3): every open or waiting claim of
//! every lesson the course holds for review, checked against the new source
//! when it has a passage for it, as an attach checks the lesson's other
//! claims.

use lattice_core::application::ports::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{Ledger, LessonReviewState};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

use super::attach::report::ClaimAttachProgressDto;
use super::attach::{check_others, course_material, AttachDeps, Checking, OtherClaims, Others};
use crate::features::learning::curriculum::LearningGenerationJobStatus;
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::dto::WaitingClaimsRecheckDto;
use crate::features::learning::persistence::db;
use crate::features::learning::reference_collection::ReferenceCollection;

/// Every claim a new source is checked for: open or waiting, in any lesson.
const WAITING: OtherClaims<'static> = OtherClaims {
    scope: Others::Lesson,
    except: None,
};

/// The ledgers of the course's held lessons that have a claim waiting:
/// empty when a new source has nothing to be checked for.
pub async fn waiting_lessons(
    store: &dyn ClaimReviewStore,
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<Ledger>> {
    let mut ledgers = Vec::new();
    for lesson_id in held_lessons(pool, program_id).await? {
        let ledger = store.load(&lesson_id).await?;
        if ledger.claims.iter().any(|entry| WAITING.take(entry)) {
            ledgers.push(ledger);
        }
    }
    Ok(ledgers)
}

/// Checks the waiting claims of `lessons` against `version_id`, reporting
/// each claim through `progress`.
pub async fn waiting_claims(
    deps: &AttachDeps<'_>,
    program_id: &str,
    version_id: &str,
    lessons: &[Ledger],
    progress: &(dyn Fn(ClaimAttachProgressDto) + Send + Sync),
) -> Result<WaitingClaimsRecheckDto> {
    let others = WAITING;
    let waiting: usize = lessons
        .iter()
        .map(|ledger| {
            ledger
                .claims
                .iter()
                .filter(|entry| others.take(entry))
                .count()
        })
        .sum();
    let mut checking = Checking {
        report: progress,
        done: 0,
        total: u32::try_from(waiting).unwrap_or(u32::MAX),
    };
    let (sources, course) = course_material(deps, program_id).await?;
    let references = ReferenceCollection::searchable_now(course.as_ref(), &sources).await?;
    let mut recheck = WaitingClaimsRecheckDto::default();
    for ledger in lessons {
        let lesson_deps = AttachDeps {
            job_id: running_job(deps.pool, ledger).await,
            ..*deps
        };
        let (tally, _) = check_others(
            &lesson_deps,
            &references,
            version_id,
            (program_id, &ledger.lesson_id),
            &others,
            false,
            &mut checking,
        )
        .await?;
        recheck.lessons += 1;
        recheck.tried += count(tally.tried);
        recheck.verified += count(tally.verified);
        recheck.not_verified += count(tally.not_verified);
        recheck.no_passage += count(tally.no_passage);
        if tally.stopped.is_some() {
            recheck.stopped = tally.stopped;
            break;
        }
    }
    progress(ClaimAttachProgressDto::Done);
    Ok(recheck)
}

/// The course's lessons held for review: waiting for the user, or being
/// verified.
async fn held_lessons(pool: &SqlitePool, program_id: &str) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT lesson_id FROM learning_lesson_reviews WHERE program_id=? AND state IN (?,?) ORDER BY opened_at,lesson_id",
    )
    .bind(program_id)
    .bind(LessonReviewState::NeedsReview.as_str())
    .bind(LessonReviewState::Verifying.as_str())
    .fetch_all(pool)
    .await
    .map_err(db)
}

/// The lesson's preparation job while it runs, for event provenance.
pub async fn running_job(pool: &SqlitePool, ledger: &Ledger) -> Option<String> {
    let job_id = &ledger.review.as_ref()?.job_id;
    let job = LearningCurriculumRepository::new(pool.clone())
        .job(job_id)
        .await
        .ok()?;
    matches!(
        job.status,
        LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Running
    )
    .then_some(job.id)
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
