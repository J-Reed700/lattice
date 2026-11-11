use super::*;
use crate::features::learning::drills::attempts::AttemptMode;
use crate::features::learning::drills::dto::{
    CodeReviewFileDto, CodeReviewFindingDto, CodeReviewResultDto, CourseDrillsDto,
    DrillMaterialDto, StartedCodeReviewDto,
};
use crate::features::learning::drills::review::{self, CourseDrill};
use lattice_core::domain::learning::drills::LANGUAGES;

// Code review drills (docs/design/2026-11-04-code-review-drills.md, section 1, and
// docs/design/2026-11-07-drills-in-the-course.md, section 8): what the review view calls.

/// Starts a drill of the course on `material`, from the lesson asked for or, with none, the one next up: an attempt
/// on a ready drill of it or, when none is ready, the build that will make one. `lines` (lines of code, or sentences)
/// and `issues` size that build. The drill comes back as the learner may see it, never with its key or its source.
#[tauri::command]
#[specta::specta]
pub async fn start_course_drill(
    program_id: String,
    lesson_id: Option<String>,
    material: DrillMaterialDto,
    mode: AttemptMode,
    lines: u32,
    issues: u32,
    container: State<'_, Container>,
) -> Result<StartedCodeReviewDto, ApiError> {
    let asked = CourseDrill {
        program_id,
        lesson_id,
        material: material.into(),
    };
    let started = review::start(container.database(), &asked, mode, lines, issues)
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
        container.database(),
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
    review::result(container.database(), &attempt_id)
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
    let acted = review::regrade(container.database(), &attempt_id)
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

/// A course's review drills: the language code drills are offered in (`language`, or the one the course suggests),
/// what a start with no lesson comes to for code and for passages, each lesson's standing by module, and the latest
/// attempts.
#[tauri::command]
#[specta::specta]
pub async fn course_drills(
    program_id: String,
    language: Option<String>,
    container: State<'_, Container>,
) -> Result<CourseDrillsDto, ApiError> {
    review::course_drills(container.database(), &program_id, language.as_deref())
        .await
        .map_err(ApiError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_languages_the_screen_offers_are_the_catalogues_in_its_order() {
        assert_eq!(code_review_languages(), LANGUAGES);
    }
}
