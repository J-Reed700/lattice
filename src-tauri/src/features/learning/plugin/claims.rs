//! Lesson claim review commands (§7.4): read a lesson's review, act on a
//! claim, resume the review, and capture an attached page's frozen copy
//! again. Attaching evidence runs here, in the command's own task, through
//! the scheduler like any model call (§9).

use super::*;
use std::sync::Arc;

use crate::features::learning::curriculum::LearningGenerationJobStatus;
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::lessons::claim_reviews::attach::{
    self, report::ClaimAttachReportDto, report::SnapshotNoteDto, AttachDeps, AttachRequest,
    Evidence,
};
use crate::features::learning::lessons::claim_reviews::dto::{
    ClaimActionRequestDto, LessonClaimReviewDto,
};
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::application::ports::{ClaimReviewStore, ReferenceSnapshot};
use lattice_core::domain::learning::claim_review::{ClaimId, Ledger, LessonReviewState, Revision};
use lattice_core::shared::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLearningClaimRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    pub claim_id: String,
    pub action: ClaimActionRequestDto,
    /// The review's revision the user acted on.
    pub expected_revision: i64,
    pub operation_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLearningClaimResultDto {
    /// The review's revision after the action: send it with the next one.
    pub revision: i64,
    /// What attaching evidence found; null for the other actions.
    pub attach: Option<ClaimAttachReportDto>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResumeLearningLessonReviewRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The course's revision, as `retry_learning_generation_job` takes it.
    pub expected_revision: i64,
    pub operation_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureLearningSourceSnapshotRequestDto {
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
}

fn not_wired(what: &str) -> ApiError {
    ApiError::from(AppError::ServiceNotAvailable(format!(
        "{what} is not available in this build."
    )))
}

/// The lesson claim ledger.
// Integration with track L1: `Arc::new(ClaimReviewRepository::new(pool))`.
fn claim_ledger(_container: &Container) -> Result<Arc<dyn ClaimReviewStore>, ApiError> {
    Err(not_wired("The claim ledger"))
}

/// A lesson's review as the panel shows it.
// Integration with track L1: the claim review service's assembly.
async fn review_of(
    _container: &Container,
    _ledger: Ledger,
) -> Result<LessonClaimReviewDto, ApiError> {
    Err(not_wired("The claim review"))
}

/// Every action but attaching evidence: confirm, edit, remove, general
/// knowledge, research again, unconfirm, undo (§5.1).
// Integration with track L1: the claim review service's `resolve`.
async fn resolve_by_service(
    _container: &Container,
    _request: &ResolveLearningClaimRequestDto,
) -> Result<Revision, ApiError> {
    Err(not_wired("This claim action"))
}

/// The lesson's preparation job while it runs, for event provenance.
async fn running_job(container: &Container, ledger: &Ledger) -> Option<String> {
    let job_id = &ledger.review.as_ref()?.job_id;
    let job = LearningCurriculumRepository::new(container.db_pool().clone())
        .job(job_id)
        .await
        .ok()?;
    matches!(
        job.status,
        LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Running
    )
    .then_some(job.id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_lesson_claim_review(
    program_id: String,
    lesson_id: String,
    container: State<'_, Container>,
) -> Result<LessonClaimReviewDto, ApiError> {
    let ledger = claim_ledger(&container)?.load(&lesson_id).await?;
    if ledger
        .review
        .as_ref()
        .is_some_and(|review| review.program_id != program_id)
    {
        return Err(ApiError::from(AppError::NotFound(
            "Lesson not found in this course".into(),
        )));
    }
    review_of(&container, ledger).await
}

#[tauri::command]
#[specta::specta]
pub async fn resolve_learning_claim(
    request: ResolveLearningClaimRequestDto,
    container: State<'_, Container>,
) -> Result<ResolveLearningClaimResultDto, ApiError> {
    let Some(evidence) = Evidence::of(&request.action) else {
        let revision = resolve_by_service(&container, &request).await?;
        return Ok(ResolveLearningClaimResultDto {
            revision: revision.0,
            attach: None,
        });
    };
    let store = claim_ledger(&container)?;
    let ledger = store.load(&request.lesson_id).await?;
    let job_id = running_job(&container, &ledger).await;
    let pin = learning_model(
        &container,
        ModelRequirements::default(),
        "Checking a claim against the evidence you added",
    )
    .await?;
    let web = container.web_service();
    let snapshots = container.reference_snapshots();
    let library = container.library_passages();
    // Library documents are ranked by meaning once an embedding model is
    // loaded, and by keyword otherwise.
    let ranking = match container.get_or_load_embedding().await {
        Ok(_) => Some(library.as_ref()),
        Err(_) => None,
    };
    let claim_id = ClaimId(request.claim_id.clone());
    let report = attach::attach(
        &AttachDeps {
            pool: container.db_pool(),
            store: store.as_ref(),
            web: web.as_ref(),
            snapshots: snapshots.as_ref(),
            documents: library.as_ref(),
            ranking,
            llm: pin.llm.as_ref(),
            job_id,
        },
        AttachRequest {
            program_id: &request.program_id,
            lesson_id: &request.lesson_id,
            claim_id: &claim_id,
            expected_revision: Revision(request.expected_revision),
            evidence,
        },
    )
    .await?;
    Ok(ResolveLearningClaimResultDto {
        revision: report.revision,
        attach: Some(report),
    })
}

/// Resume verification (§6.2): a new attempt of the lesson's job, refused
/// while nothing changed since it ended.
#[tauri::command]
#[specta::specta]
pub async fn resume_learning_lesson_review(
    request: ResumeLearningLessonReviewRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let ledger = claim_ledger(&container)?.load(&request.lesson_id).await?;
    let review = ledger
        .review
        .as_ref()
        .filter(|review| review.program_id == request.program_id)
        .ok_or_else(|| {
            ApiError::from(AppError::NotFound(
                "This lesson has no claim review to resume.".into(),
            ))
        })?;
    let repo = LearningCurriculumRepository::new(container.db_pool().clone());
    let job = repo.job(&review.job_id).await?;
    if review.state == LessonReviewState::NeedsReview {
        let ended = job.finished_at.unwrap_or(job.created_at);
        let changed = ledger.claims.iter().any(|claim| {
            claim
                .events()
                .iter()
                .any(|event| event.provenance.at > ended)
        });
        if !changed {
            return Err(ApiError::from(AppError::InvalidState(
                "Nothing changed since the last pass. Resolve a claim or add evidence first."
                    .into(),
            )));
        }
    }
    let job = repo
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: request.operation_id,
            program_id: request.program_id,
            job_id: job.id,
            expected_revision: request.expected_revision,
        })
        .await?;
    container.jobs().submitted(&job.id).await;
    Ok(job)
}

/// "Capture again" on an attached page whose frozen copy failed (§5.4).
#[tauri::command]
#[specta::specta]
pub async fn capture_learning_source_snapshot(
    request: CaptureLearningSourceSnapshotRequestDto,
    container: State<'_, Container>,
) -> Result<SnapshotNoteDto, ApiError> {
    let repo = source_library(&container);
    let version = repo
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: request.program_id.clone(),
            source_id: request.source_id,
            version_id: request.version_id.clone(),
        })
        .await?;
    let Some(url) = version.version.resolved_url else {
        return Ok(SnapshotNoteDto::NotApplicable);
    };
    Ok(
        match container.reference_snapshots().request(&url, true).await? {
            ReferenceSnapshot::Ready { sha256 } => {
                repo.link_snapshot(&request.program_id, &request.version_id, &sha256)
                    .await?;
                SnapshotNoteDto::Ready
            }
            ReferenceSnapshot::Pending => SnapshotNoteDto::Pending,
            ReferenceSnapshot::Failed { reason } => SnapshotNoteDto::Failed { reason },
        },
    )
}
