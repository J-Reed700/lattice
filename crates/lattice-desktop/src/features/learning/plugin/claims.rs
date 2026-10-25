//! Lesson claim review commands (§7.4): read a lesson's review, act on a
//! claim, attach evidence to claims, keep a teaching finding as written,
//! resume the review, and capture an attached page's frozen copy again.
//! Attaching evidence runs here, in the command's own task, through the
//! scheduler like any model call (§9).

use super::*;
use std::sync::Arc;

use crate::features::learning::lessons::claim_reviews::attach::{
    self, report::ClaimAttachProgressDto, report::ClaimAttachReportDto, report::SnapshotNoteDto,
    AttachDeps, AttachRequest, Others,
};
use crate::features::learning::lessons::claim_reviews::checker_examples::{
    export_file_name, CheckerAccuracyDto, CheckerExamples,
};
use crate::features::learning::lessons::claim_reviews::dto::{
    AttachEvidenceDto, AttachScopeDto, ClaimActionRequestDto, LessonClaimReviewDto,
};
use crate::features::learning::lessons::claim_reviews::recheck::running_job;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::service::ClaimReviewService;
use crate::features::learning::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::application::ports::{ClaimReviewStore, ReferenceSnapshot};
use lattice_core::domain::learning::claim_review::{ClaimId, Ledger, Revision};
use lattice_core::shared::error::AppError;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// The extension of an exported checker-examples file (§15).
const EXPORT_EXTENSION: &str = "jsonl";

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
}

/// Evidence the user attaches, and the claims it is checked for (UI design
/// §5.3).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachLearningClaimEvidenceRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The claim the attach is recorded on; the first of `scope` when a
    /// scope is given.
    pub claim_id: String,
    /// The review's revision the user acted on.
    pub expected_revision: i64,
    pub evidence: AttachEvidenceDto,
    pub scope: AttachScopeDto,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeepLearningTeachingFindingRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The finding's `findingKey` from the review.
    pub finding_key: String,
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
pub struct ExportLearningCheckerExamplesRequestDto {
    /// One course's examples; every course's when null.
    pub program_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportedLearningCheckerExamplesDto {
    pub path: String,
    /// Claim pairs written; retrieval misses are left out.
    pub pairs: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureLearningSourceSnapshotRequestDto {
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
}

/// The lesson claim ledger.
fn claim_ledger(container: &Container) -> Arc<dyn ClaimReviewStore> {
    Arc::new(ClaimReviewRepository::new(container.db_pool().clone()))
}

async fn claim_reviews(container: &Container) -> Result<ClaimReviewService, ApiError> {
    Ok(ClaimReviewService::new(container.db_pool().clone())
        .with_checker_model(container.learning_checker_name().await?))
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_lesson_claim_review(
    program_id: String,
    lesson_id: String,
    container: State<'_, Container>,
) -> Result<Option<LessonClaimReviewDto>, ApiError> {
    Ok(claim_reviews(&container)
        .await?
        .review(&program_id, &lesson_id)
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn resolve_learning_claim(
    request: ResolveLearningClaimRequestDto,
    container: State<'_, Container>,
) -> Result<ResolveLearningClaimResultDto, ApiError> {
    in_course(
        &claim_ledger(&container).load(&request.lesson_id).await?,
        &request.program_id,
    )?;
    let revision = claim_reviews(&container)
        .await?
        .resolve(
            &request.lesson_id,
            &request.claim_id,
            request.action,
            request.expected_revision,
        )
        .await?;
    Ok(ResolveLearningClaimResultDto { revision })
}

/// The lesson's review belongs to the course the request names.
fn in_course(ledger: &Ledger, program_id: &str) -> Result<(), ApiError> {
    if ledger
        .review
        .as_ref()
        .is_some_and(|review| review.program_id != program_id)
    {
        return Err(ApiError::from(AppError::NotFound(
            "Lesson not found in this course".into(),
        )));
    }
    Ok(())
}

/// Attach the user's evidence to a claim (§5.4): save it as a course
/// source, check the claim against it, then the other open claims in scope,
/// reporting each stage on `on_progress`.
#[tauri::command]
#[specta::specta]
pub async fn attach_learning_claim_evidence(
    request: AttachLearningClaimEvidenceRequestDto,
    on_progress: tauri::ipc::Channel<ClaimAttachProgressDto>,
    container: State<'_, Container>,
) -> Result<ClaimAttachReportDto, ApiError> {
    let store = claim_ledger(&container);
    let ledger = store.load(&request.lesson_id).await?;
    in_course(&ledger, &request.program_id)?;
    let job_id = running_job(container.db_pool(), &ledger).await;
    let pin = learning_model(
        &container,
        ModelRequirements::default(),
        "Checking a claim against the evidence you added",
    )
    .await?;
    let web = container.web_service();
    let snapshots = container.reference_snapshots();
    let library = container.library_passages();
    let claim_id = ClaimId(request.claim_id.clone());
    let scoped: Vec<ClaimId> = match &request.scope {
        AttachScopeDto::Claim => Vec::new(),
        AttachScopeDto::Claims { claim_ids } => claim_ids.iter().cloned().map(ClaimId).collect(),
    };
    let progress = move |stage: ClaimAttachProgressDto| {
        let _ = on_progress.send(stage);
    };
    let report = attach::attach_to(
        &AttachDeps {
            pool: container.db_pool(),
            store: store.as_ref(),
            web: web.as_ref(),
            snapshots: snapshots.as_ref(),
            documents: library.as_ref(),
            courses: container.course_indexes(),
            llm: pin.llm.as_ref(),
            job_id,
        },
        AttachRequest {
            program_id: &request.program_id,
            lesson_id: &request.lesson_id,
            claim_id: &claim_id,
            expected_revision: Revision(request.expected_revision),
            evidence: request.evidence.into(),
        },
        Others::Only(&scoped),
        &progress,
    )
    .await?;
    Ok(report)
}

/// "Keep as written": the user settles a teaching finding the repairs did
/// not, without changing the lesson. It stays settled while the part it is
/// about keeps its text.
#[tauri::command]
#[specta::specta]
pub async fn keep_learning_teaching_finding(
    request: KeepLearningTeachingFindingRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    Ok(TeachingFindingsRepository::new(container.db_pool().clone())
        .keep(
            &request.program_id,
            &request.lesson_id,
            &request.finding_key,
        )
        .await?)
}

/// Resume verification (§6.2): a lesson waiting for review continues its
/// job once something changed; a stopped one retries it.
#[tauri::command]
#[specta::specta]
pub async fn resume_learning_lesson_review(
    request: ResumeLearningLessonReviewRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let job = claim_reviews(&container)
        .await?
        .resume(
            &request.program_id,
            &request.lesson_id,
            request.expected_revision,
            request.operation_id,
        )
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

/// The local checker's accuracy on the user's own labelled claims, per
/// checker model (§15).
#[tauri::command]
#[specta::specta]
pub async fn get_learning_checker_accuracy(
    container: State<'_, Container>,
) -> Result<Vec<CheckerAccuracyDto>, ApiError> {
    Ok(CheckerExamples::new(container.db_pool().clone())
        .accuracy()
        .await?)
}

/// Writes the labelled claims as `claim-pairs-v1` lines to a file the user
/// picks, for `live_claim_pairs_eval` (§15, `evals/grounding/README.md`).
/// `None` when the user dismissed the dialog.
#[tauri::command]
#[specta::specta]
pub async fn export_learning_checker_examples<R: Runtime>(
    app: AppHandle<R>,
    request: ExportLearningCheckerExamplesRequestDto,
    container: State<'_, Container>,
) -> Result<Option<ExportedLearningCheckerExamplesDto>, ApiError> {
    let suggested = export_file_name(&chrono::Local::now().format("%Y-%m-%d").to_string());
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Export checker examples")
            .set_file_name(suggested)
            .add_filter("JSON Lines", &[EXPORT_EXTENSION])
            .blocking_save_file()
            .and_then(|picked| picked.into_path().ok())
    })
    .await
    .map_err(|error| {
        AppError::InternalError(format!("The file dialog stopped unexpectedly: {error}"))
    })?;
    let Some(path) = picked else {
        return Ok(None);
    };
    let pairs = CheckerExamples::new(container.db_pool().clone())
        .export(request.program_id.as_deref(), &path)
        .await?;
    Ok(Some(ExportedLearningCheckerExamplesDto {
        path: path.to_string_lossy().into_owned(),
        pairs: u32::try_from(pairs).unwrap_or(u32::MAX),
    }))
}
