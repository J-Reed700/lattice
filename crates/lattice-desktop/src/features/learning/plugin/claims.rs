//! Lesson claim review commands (§7.4): read a lesson's review, act on a
//! claim, attach evidence to claims, keep teaching findings as written or
//! take that back, resume the review, and capture an attached page's frozen copy again.
//! Attaching evidence runs here, in the command's own task, through the
//! scheduler like any model call (§9).

use super::*;
use std::sync::Arc;

use crate::features::learning::assessment_engine::LearningItemFormat;
use crate::features::learning::lessons::claim_reviews::attach::{
    self, report::ClaimAttachProgressDto, report::ClaimAttachReportDto, report::SnapshotNoteDto,
    AttachDeps, AttachRequest, LessonSourceRequest, Others,
};
use crate::features::learning::lessons::claim_reviews::checker_examples::{
    export_file_name, CheckerAccuracyDto, CheckerExamples,
};
use crate::features::learning::lessons::claim_reviews::dto::{
    AttachEvidenceDto, AttachScopeDto, ClaimActionRequestDto, LessonClaimReviewDto,
};
use crate::features::learning::lessons::claim_reviews::recheck::running_job;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::service::{
    ClaimReviewService, QuestionEdit, QuestionText, SectionEdit,
};
use crate::features::learning::lessons::claim_reviews::teaching_findings::{
    TeachingFindingsRepository, TeachingSettleDto,
};
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

/// One action for several claims at once (`Confirm`, `Remove`, `PutBack` of
/// a sentence as it was, or `Undo` of the last such batch). An Undo takes the
/// claims back in the reverse of the order sent, so send the batch's own
/// order: the last one done is the first one undone.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLearningClaimsRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The claims the action is for.
    pub claim_ids: Vec<String>,
    pub action: ClaimActionRequestDto,
    /// The review's revision the user acted on; for an Undo, the revision
    /// the batch returned.
    pub expected_revision: i64,
    pub operation_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLearningClaimResultDto {
    /// The review's revision after the action: send it with the next one.
    pub revision: i64,
}

/// The teaching notes whose sentences the user takes out of a published
/// lesson's sections.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveLearningTeachingSentencesRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The notes' `findingKey`s from the review.
    pub finding_keys: Vec<String>,
    /// The review's revision the user acted on.
    pub expected_revision: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveLearningTeachingSentencesResultDto {
    /// The review's revision after the removal.
    pub revision: i64,
    /// The claims the removal ended, in the order it ended them: send them to
    /// `resolve_learning_claims` with `undo` and `revision` to take it back.
    pub claim_ids: Vec<String>,
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

/// Evidence the user adds to the lesson as a whole.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachLearningLessonSourceRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The review's revision the user acted on.
    pub expected_revision: i64,
    pub evidence: AttachEvidenceDto,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SettleLearningTeachingFindingsRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// The findings' `findingKey`s from the review.
    pub finding_keys: Vec<String>,
    pub settle: TeachingSettleDto,
}

/// A published lesson's section as the user rewrote it, with the title and
/// text they opened it with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditLearningLessonSectionRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// Zero-based.
    pub section_index: u32,
    pub before_title: String,
    pub before_body: String,
    pub title: String,
    pub body: String,
    /// Answer what saving would do and write nothing.
    pub dry_run: bool,
}

/// What a rewrite of a section does, or did, to the lesson's statements.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LessonSectionEditDto {
    pub changed: bool,
    /// Statements on text the user left as it was.
    pub statements_kept: u32,
    /// Statements on text the user changed.
    pub statements_changed: u32,
    /// Statements on text the user deleted.
    pub statements_removed: u32,
    /// Sentences the user wrote.
    pub sentences_written: u32,
    /// Changed or removed statements that were checked against the sources.
    pub checked_statements_affected: u32,
    /// The review's revision after a save; null for a dry run, no change, or
    /// a lesson without a review.
    pub revision: Option<i64>,
}

/// A published lesson's question as the editor shows it and sends it back.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LessonQuestionDto {
    pub kind: LearningItemFormat,
    pub prompt: String,
    /// Empty for a kind without choices.
    pub choices: Vec<String>,
    /// The index of the right choice; null for a kind without one.
    pub right: Option<u32>,
    pub explanation: String,
    /// One reason per choice, the right choice's empty; empty when the
    /// question has none.
    pub rationales: Vec<String>,
}

impl From<QuestionText> for LessonQuestionDto {
    fn from(question: QuestionText) -> Self {
        Self {
            kind: question.kind,
            prompt: question.prompt,
            choices: question.choices,
            right: question
                .right
                .map(|right| u32::try_from(right).unwrap_or(u32::MAX)),
            explanation: question.explanation,
            rationales: question.rationales,
        }
    }
}

impl From<LessonQuestionDto> for QuestionText {
    fn from(question: LessonQuestionDto) -> Self {
        Self {
            kind: question.kind,
            prompt: question.prompt,
            choices: question.choices,
            right: question
                .right
                .map(|right| usize::try_from(right).unwrap_or(usize::MAX)),
            explanation: question.explanation,
            rationales: question.rationales,
        }
    }
}

/// A published lesson's question as the user rewrote it, with the question
/// they opened.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditLearningLessonQuestionRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    /// Zero-based, among the lesson's questions.
    pub question_index: u32,
    pub before: LessonQuestionDto,
    pub after: LessonQuestionDto,
    /// Answer what saving would do and write nothing.
    pub dry_run: bool,
}

/// What a rewrite of a question does, or did, to the lesson's statements.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LessonQuestionEditDto {
    pub changed: bool,
    /// Statements on text the user left as it was.
    pub statements_kept: u32,
    /// Statements on text the user changed.
    pub statements_changed: u32,
    /// Statements on text the user deleted.
    pub statements_removed: u32,
    /// Sentences the user wrote.
    pub sentences_written: u32,
    /// Changed or removed statements that were checked against the sources.
    pub checked_statements_affected: u32,
    /// The review's revision after a save; null for a dry run, no change, or
    /// a lesson without a review.
    pub revision: Option<i64>,
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
    Arc::new(ClaimReviewRepository::new(container.database().clone()))
}

async fn claim_reviews(container: &Container) -> Result<ClaimReviewService, ApiError> {
    Ok(ClaimReviewService::new(container.database().clone())
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

/// Applies one action to every claim in the request, all or nothing.
#[tauri::command]
#[specta::specta]
pub async fn resolve_learning_claims(
    request: ResolveLearningClaimsRequestDto,
    container: State<'_, Container>,
) -> Result<ResolveLearningClaimResultDto, ApiError> {
    in_course(
        &claim_ledger(&container).load(&request.lesson_id).await?,
        &request.program_id,
    )?;
    let revision = claim_reviews(&container)
        .await?
        .resolve_many(
            &request.lesson_id,
            &request.claim_ids,
            request.action,
            request.expected_revision,
        )
        .await?;
    Ok(ResolveLearningClaimResultDto { revision })
}

/// Takes the sentence of every chosen teaching note out of its section, all
/// or nothing.
#[tauri::command]
#[specta::specta]
pub async fn remove_learning_teaching_sentences(
    request: RemoveLearningTeachingSentencesRequestDto,
    container: State<'_, Container>,
) -> Result<RemoveLearningTeachingSentencesResultDto, ApiError> {
    in_course(
        &claim_ledger(&container).load(&request.lesson_id).await?,
        &request.program_id,
    )?;
    let removed = claim_reviews(&container)
        .await?
        .remove_note_sentences(
            &request.lesson_id,
            &request.finding_keys,
            request.expected_revision,
        )
        .await?;
    Ok(RemoveLearningTeachingSentencesResultDto {
        revision: removed.revision,
        claim_ids: removed.claim_ids,
    })
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
    let job_id = running_job(container.database(), &ledger).await;
    let pin = learning_model(
        &container,
        ModelRequirements::default(),
        "Checking a statement against the evidence you added",
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
            database: container.database(),
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

/// Add the user's evidence to the lesson as a whole: save it as a course
/// source and check every open claim of the lesson against it, reporting each
/// stage on `on_progress`.
#[tauri::command]
#[specta::specta]
pub async fn attach_learning_lesson_source(
    request: AttachLearningLessonSourceRequestDto,
    on_progress: tauri::ipc::Channel<ClaimAttachProgressDto>,
    container: State<'_, Container>,
) -> Result<ClaimAttachReportDto, ApiError> {
    let store = claim_ledger(&container);
    let ledger = store.load(&request.lesson_id).await?;
    in_course(&ledger, &request.program_id)?;
    let job_id = running_job(container.database(), &ledger).await;
    let pin = learning_model(
        &container,
        ModelRequirements::default(),
        "Checking a lesson's statements against the source you added",
    )
    .await?;
    let web = container.web_service();
    let snapshots = container.reference_snapshots();
    let library = container.library_passages();
    let progress = move |stage: ClaimAttachProgressDto| {
        let _ = on_progress.send(stage);
    };
    let report = attach::attach_to_lesson(
        &AttachDeps {
            database: container.database(),
            store: store.as_ref(),
            web: web.as_ref(),
            snapshots: snapshots.as_ref(),
            documents: library.as_ref(),
            courses: container.course_indexes(),
            llm: pin.llm.as_ref(),
            job_id,
        },
        LessonSourceRequest {
            program_id: &request.program_id,
            lesson_id: &request.lesson_id,
            expected_revision: Revision(request.expected_revision),
            evidence: request.evidence.into(),
        },
        &progress,
    )
    .await?;
    Ok(report)
}

/// "Keep as written": the user settles teaching findings the repairs did
/// not, without changing the lesson, or takes that back. A kept finding stays
/// settled while the part it is about keeps its text. All the findings change
/// or none.
#[tauri::command]
#[specta::specta]
pub async fn settle_learning_teaching_findings(
    request: SettleLearningTeachingFindingsRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    Ok(
        TeachingFindingsRepository::new(container.database().clone())
            .settle(
                &request.program_id,
                &request.lesson_id,
                &request.finding_keys,
                request.settle,
            )
            .await?,
    )
}

/// Rewrites a section of a published lesson, its title and its text. The
/// statements on text the user changed end, the others keep their state, and
/// each sentence the user wrote becomes a statement they confirmed. With
/// `dry_run` it only answers what saving would do.
#[tauri::command]
#[specta::specta]
pub async fn edit_learning_lesson_section(
    request: EditLearningLessonSectionRequestDto,
    container: State<'_, Container>,
) -> Result<LessonSectionEditDto, ApiError> {
    let outcome = claim_reviews(&container)
        .await?
        .edit_section(SectionEdit {
            program_id: request.program_id,
            lesson_id: request.lesson_id,
            section_index: usize::try_from(request.section_index).unwrap_or(usize::MAX),
            before_title: request.before_title,
            before_body: request.before_body,
            title: request.title,
            body: request.body,
            dry_run: request.dry_run,
        })
        .await?;
    Ok(LessonSectionEditDto {
        changed: outcome.changed,
        statements_kept: outcome.statements_kept,
        statements_changed: outcome.statements_changed,
        statements_removed: outcome.statements_removed,
        sentences_written: outcome.sentences_written,
        checked_statements_affected: outcome.checked_statements_affected,
        revision: outcome.revision,
    })
}

/// A published lesson's question, as the editor opens it: its kind, prompt,
/// choices, right choice and explanation.
#[tauri::command]
#[specta::specta]
pub async fn get_learning_lesson_question(
    program_id: String,
    lesson_id: String,
    question_index: u32,
    container: State<'_, Container>,
) -> Result<LessonQuestionDto, ApiError> {
    let question = claim_reviews(&container)
        .await?
        .question_text(
            &program_id,
            &lesson_id,
            usize::try_from(question_index).unwrap_or(usize::MAX),
        )
        .await?;
    Ok(question.into())
}

/// Rewrites a question of a published lesson: its prompt, choices, right
/// answer and explanation. The statements on text the user changed end, the
/// others keep their state, and each sentence the user wrote becomes a
/// statement they confirmed. With `dry_run` it only answers what saving would
/// do.
#[tauri::command]
#[specta::specta]
pub async fn edit_learning_lesson_question(
    request: EditLearningLessonQuestionRequestDto,
    container: State<'_, Container>,
) -> Result<LessonQuestionEditDto, ApiError> {
    let outcome = claim_reviews(&container)
        .await?
        .edit_question(QuestionEdit {
            program_id: request.program_id,
            lesson_id: request.lesson_id,
            question_index: usize::try_from(request.question_index).unwrap_or(usize::MAX),
            before: request.before.into(),
            after: request.after.into(),
            dry_run: request.dry_run,
        })
        .await?;
    Ok(LessonQuestionEditDto {
        changed: outcome.changed,
        statements_kept: outcome.statements_kept,
        statements_changed: outcome.statements_changed,
        statements_removed: outcome.statements_removed,
        sentences_written: outcome.sentences_written,
        checked_statements_affected: outcome.checked_statements_affected,
        revision: outcome.revision,
    })
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
    Ok(CheckerExamples::new(container.database().clone())
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
    let pairs = CheckerExamples::new(container.database().clone())
        .export(request.program_id.as_deref(), &path)
        .await?;
    Ok(Some(ExportedLearningCheckerExamplesDto {
        path: path.to_string_lossy().into_owned(),
        pairs: u32::try_from(pairs).unwrap_or(u32::MAX),
    }))
}
