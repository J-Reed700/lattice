use super::*;
use crate::features::learning::drills::attempts::AttemptMode;
use crate::features::learning::drills::dto::{
    CodeReviewFileDto, CodeReviewFindingDto, CodeReviewResultDto, StartedCodeReviewDto,
};
use crate::features::learning::drills::review;
use lattice_core::domain::learning::drills::LANGUAGES;

// Code review drills (docs/design/2026-11-04-code-review-drills.md, section 1): what the review view calls.

/// Starts a drill of `language`: an attempt on a ready one, or, when none is ready, the build that will make one.
/// `lines` and `patterns_count` size that build. The drill comes back as the learner may see it, never with its key.
#[tauri::command]
#[specta::specta]
pub async fn start_code_review_drill(
    language: String,
    mode: AttemptMode,
    lines: u32,
    patterns_count: u32,
    container: State<'_, Container>,
) -> Result<StartedCodeReviewDto, ApiError> {
    let started = review::start(container.db_pool(), &language, mode, lines, patterns_count)
        .await
        .map_err(ApiError::from)?;
    for job in &started.jobs {
        container.jobs().submitted(job).await;
    }
    Ok(started.dto)
}

/// Hands in a review: the learner's findings, their fix when they wrote one and the time they spent. The grading job
/// is queued; the attempt reads as submitted until it is graded.
#[tauri::command]
#[specta::specta]
pub async fn submit_code_review(
    attempt_id: String,
    findings: Vec<CodeReviewFindingDto>,
    fix_files: Option<Vec<CodeReviewFileDto>>,
    time_spent_ms: u64,
    container: State<'_, Container>,
) -> Result<CodeReviewResultDto, ApiError> {
    let acted = review::submit(
        container.db_pool(),
        &attempt_id,
        findings,
        fix_files,
        time_spent_ms,
    )
    .await
    .map_err(ApiError::from)?;
    if let Some(job) = &acted.job {
        container.jobs().submitted(job).await;
    }
    Ok(acted.dto)
}

/// Where an attempt stands. Once it is graded, the reveal comes with it.
#[tauri::command]
#[specta::specta]
pub async fn code_review_result(
    attempt_id: String,
    container: State<'_, Container>,
) -> Result<CodeReviewResultDto, ApiError> {
    review::result(container.db_pool(), &attempt_id)
        .await
        .map_err(ApiError::from)
}

/// Grades an attempt that could not be graded again.
#[tauri::command]
#[specta::specta]
pub async fn regrade_code_review(
    attempt_id: String,
    container: State<'_, Container>,
) -> Result<CodeReviewResultDto, ApiError> {
    let acted = review::regrade(container.db_pool(), &attempt_id)
        .await
        .map_err(ApiError::from)?;
    if let Some(job) = &acted.job {
        container.jobs().submitted(job).await;
    }
    Ok(acted.dto)
}

/// The languages a drill can be written in, in the catalogue's order.
#[tauri::command]
#[specta::specta]
pub fn code_review_languages() -> Vec<String> {
    LANGUAGES.iter().map(|id| (*id).to_owned()).collect()
}
