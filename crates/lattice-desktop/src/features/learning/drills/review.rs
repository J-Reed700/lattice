//! What the review view's commands do, apart from Tauri: starting a drill, handing a review in, reading an attempt and
//! asking for its grade again (docs/design/2026-11-04-code-review-drills.md section 1). The jobs they queue are
//! returned for the caller to tell the job runtime of once they are committed.

use super::attempts::{self, AttemptMode, Submission};
use super::bank;
use super::dto::{
    CodeReviewFileDto, CodeReviewFindingDto, CodeReviewResultDto, StartedCodeReviewDto,
};
use super::reveal::{learner_drill, result_of};
use super::spec::DrillSpec;
use crate::features::learning::persistence::now;
use lattice_core::domain::learning::drills::targets;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

/// What starting a drill came to, and the jobs to tell the job runtime of.
#[derive(Debug, Clone, PartialEq)]
pub struct Started {
    pub dto: StartedCodeReviewDto,
    pub jobs: Vec<String>,
}

/// An attempt as it stands after the learner acted on it, and the job to tell the job runtime of, if one was queued.
#[derive(Debug, Clone, PartialEq)]
pub struct Acted {
    pub dto: CodeReviewResultDto,
    pub job: Option<String>,
}

/// Starts a drill of `language`: opens an attempt on the oldest ready one, which queues the build of the next. When none
/// is ready, a build is queued and the learner is told it is under way; `lines` and `patterns_count` size that build,
/// whose patterns are the ones the schedule offers.
pub async fn start(
    pool: &SqlitePool,
    language: &str,
    mode: AttemptMode,
    lines: u32,
    patterns_count: u32,
) -> Result<Started> {
    let count = usize::try_from(patterns_count).unwrap_or(usize::MAX);
    let patterns: Vec<String> = targets(language, &[], now(), count)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    let spec = DrillSpec {
        language: language.to_owned(),
        domain: None,
        lines,
        patterns,
        program_id: None,
    };
    spec.validate()?;
    if let Some(begun) = attempts::start(pool, language, mode).await? {
        return Ok(Started {
            dto: StartedCodeReviewDto::Started {
                attempt_id: begun.attempt.id.clone(),
                mode: begun.attempt.mode,
                started_at: begun.attempt.started_at,
                drill: learner_drill(&begun.drill),
            },
            jobs: vec![begun.next_build],
        });
    }
    let job = bank::ensure_build(pool, &spec).await?;
    Ok(Started {
        dto: StartedCodeReviewDto::Building {
            job_id: job.clone(),
        },
        jobs: vec![job],
    })
}

/// Hands in an open attempt: the learner's findings and, when they wrote one, their fix, with the time they spent as the
/// client measured it. The grading job is queued.
pub async fn submit(
    pool: &SqlitePool,
    attempt_id: &str,
    findings: Vec<CodeReviewFindingDto>,
    fix_files: Option<Vec<CodeReviewFileDto>>,
    time_spent_ms: u64,
) -> Result<Acted> {
    let time_spent_ms = i64::try_from(time_spent_ms)
        .map_err(|_| AppError::InvalidInput("The time spent is not a time.".into()))?;
    let queued = attempts::submit(
        pool,
        attempt_id,
        Submission {
            findings: findings.into_iter().map(Into::into).collect(),
            fix_files: fix_files.map(|files| files.into_iter().map(Into::into).collect()),
            time_spent_ms,
        },
    )
    .await?;
    acted(pool, &queued.attempt.id, queued.job).await
}

/// The attempt: where it stands and, once it is graded, the reveal.
pub async fn result(pool: &SqlitePool, attempt_id: &str) -> Result<CodeReviewResultDto> {
    let (attempt, stored) = attempts::get(pool, attempt_id).await?;
    result_of(&attempt, &stored)
}

/// Asks for the grade of an attempt that could not be graded again. The grading job is queued.
pub async fn regrade(pool: &SqlitePool, attempt_id: &str) -> Result<Acted> {
    let queued = attempts::regrade(pool, attempt_id).await?;
    acted(pool, &queued.attempt.id, queued.job).await
}

async fn acted(pool: &SqlitePool, attempt_id: &str, job: Option<String>) -> Result<Acted> {
    Ok(Acted {
        dto: result(pool, attempt_id).await?,
        job,
    })
}
