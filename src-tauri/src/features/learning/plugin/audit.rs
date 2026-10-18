//! Verifier audit commands (lesson claim review §14.7): read and set a
//! course's opt-in with its consent, read a lesson's latest audit, and audit
//! a lesson now. The audit itself runs in `lessons::verifier_audit`, which
//! sees only an `LLMPort`, the claim ledger's port and its own stores.

use super::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::features::learning::curriculum_repository::{
    LearningCurriculumRepository, ProgramVerifierAudit,
};
use crate::features::learning::lessons::verifier_audit::{
    self as audit,
    policy::{consent, Consent},
    runs::{AuditRun, BatchOutcome, ProgramGate, RunState, SqliteAuditRuns},
    tools::VerifierWeb,
    AuditDeps, AuditOutcome, AuditPassage, AuditRequest, AuditStop, AuditUnit,
};
use crate::features::learning::persistence::now;
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::application::ports::ClaimReviewStore;
use lattice_core::application::services::chat_model_resolution::{
    cloud_provider_id, cloud_provider_name,
};
use lattice_core::domain::learning::claim_review::{ClaimId, LessonReview, LessonReviewState};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result as CoreResult};

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

/// What one lesson's audit sends: its draft's digest, its units in the
/// report's unit order, and each claim's passages as its card shows them.
struct AuditMaterial {
    draft_sha256: String,
    units: Vec<AuditUnit>,
    passages: HashMap<ClaimId, Vec<AuditPassage>>,
}

// Integration with track L1: the review's `draft_sha256`, the draft's units
// rendered as the report indexes them (`claim_ledger::field_text`), and
// `ClaimReviewRepository::shown_passages` with each source version's title.
async fn audit_material(
    _container: &Container,
    _program_id: &str,
    _lesson_id: &str,
) -> Result<AuditMaterial, ApiError> {
    Err(not_wired("The lesson's audit material"))
}

/// The Verifier's web tools. `None` until track S's capture service saves a
/// fetched page with `added_by = 'verifier'`; the audit then runs on the
/// cited passages only and says so (§14.6).
// Integration with track S: search through `container.web_service()`, save
// through the source library's web capture with `SourceAddedBy::Verifier`.
fn verifier_web(_container: &Container) -> Option<Arc<dyn VerifierWeb>> {
    None
}

/// Where an audit with `model` sends lesson text, in words.
fn destination(model: &ModelRef) -> String {
    match model {
        ModelRef::Cloud { provider, model } => {
            format!("{} ({model})", cloud_provider_name(*provider))
        }
        ModelRef::Local { model_id } => format!("a model on this computer ({model_id})"),
        ModelRef::Ollama { model } => format!("your Ollama server ({model})"),
        ModelRef::LlamaCpp { model } => format!("your llama.cpp server ({model})"),
    }
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

async fn verifier_audit_dto(
    container: &Container,
    program_id: &str,
) -> CoreResult<LearningVerifierAuditDto> {
    let pool = container.db_pool().clone();
    let setting = LearningCurriculumRepository::new(pool.clone())
        .verifier_audit(program_id)
        .await?;
    let verifier = container.verifier_model().await?;
    let usage = SqliteAuditRuns::new(pool).course_usage(program_id).await?;
    let (consent_required_for, blocked) = match &verifier {
        None => (
            None,
            Some(AuditStop::NoVerifier.message(None)).filter(|_| setting.on),
        ),
        Some(model) => {
            let required = match model {
                ModelRef::Cloud { provider, .. } => Some(cloud_provider_name(*provider).to_owned()),
                _ => None,
            };
            let state = consent(model, setting.consent_provider.as_deref());
            let blocked = AuditStop::from_consent(&state)
                .filter(|_| setting.on)
                .map(|stop| stop.message(None));
            (required, blocked)
        }
    };
    Ok(LearningVerifierAuditDto {
        program_id: program_id.to_owned(),
        on: setting.on,
        web_search: setting.web_search,
        consent_at: setting.consent_at,
        consent_provider: setting.consent_provider,
        destination: verifier.as_ref().map(destination),
        verifier,
        consent_required_for,
        blocked,
        usage: LearningAuditUsageDto {
            audits: signed(usage.audits),
            input_tokens: signed(usage.input_tokens),
            output_tokens: signed(usage.output_tokens),
        },
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_program_verifier_audit(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningVerifierAuditDto, ApiError> {
    verifier_audit_dto(container.inner(), &program_id)
        .await
        .map_err(ApiError::from)
}

/// Turns the course's audit on or off. Turning it on for a cloud Verifier
/// needs `consent_confirmed`: the user confirmed the dialog naming the
/// provider. Turning it off clears the consent and stops a running audit at
/// its next batch.
#[tauri::command]
#[specta::specta]
pub async fn set_learning_program_verifier_audit(
    program_id: String,
    on: bool,
    web_search: bool,
    consent_confirmed: bool,
    container: State<'_, Container>,
) -> Result<LearningVerifierAuditDto, ApiError> {
    let container = container.inner();
    let programs = LearningCurriculumRepository::new(container.db_pool().clone());
    let mut setting = ProgramVerifierAudit {
        on,
        web_search,
        ..ProgramVerifierAudit::default()
    };
    if on {
        let Some(model) = container.verifier_model().await? else {
            return Err(AppError::InvalidConfig(
                "No Verifier model is set. Choose one in Settings → Models under Verifier.".into(),
            )
            .into());
        };
        match consent(&model, None) {
            Consent::Missing { provider } => {
                if !consent_confirmed {
                    return Err(AppError::InvalidInput(format!(
                        "Confirm sending lesson text to {} before turning the audit on.",
                        cloud_provider_name(provider)
                    ))
                    .into());
                }
                setting.consent_at = Some(now());
                setting.consent_provider = Some(cloud_provider_id(provider).to_owned());
            }
            Consent::NotNeeded | Consent::Given { .. } | Consent::OtherProvider { .. } => {}
        }
    }
    programs.set_verifier_audit(&program_id, &setting).await?;
    verifier_audit_dto(container, &program_id)
        .await
        .map_err(ApiError::from)
}

fn lesson_audit_dto(run: AuditRun) -> LearningLessonAuditDto {
    let last = run.batches.last();
    LearningLessonAuditDto {
        lesson_id: run.key.lesson_id,
        state: match run.state {
            RunState::Running => LearningLessonAuditState::Running,
            RunState::Completed => LearningLessonAuditState::Completed,
            RunState::Stopped => LearningLessonAuditState::Stopped,
        },
        verifier_model: run.key.verifier_model,
        batches_done: signed(
            run.batches
                .iter()
                .filter(|batch| batch.outcome == BatchOutcome::Done)
                .count() as u64,
        ),
        batches_planned: last.map_or(0, |batch| signed(batch.of as u64)),
        input_tokens: signed(run.input_tokens),
        output_tokens: signed(run.output_tokens),
        last_error: last
            .filter(|batch| batch.outcome == BatchOutcome::Failed)
            .and_then(|batch| batch.error.clone()),
        web_note: run
            .batches
            .iter()
            .rev()
            .find_map(|batch| batch.web_note.clone()),
        started_at: run.started_at,
        finished_at: run.finished_at,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_lesson_audit(
    program_id: String,
    lesson_id: String,
    container: State<'_, Container>,
) -> Result<Option<LearningLessonAuditDto>, ApiError> {
    tracing::debug!(program_id, lesson_id, "learning.audit.read");
    SqliteAuditRuns::new(container.db_pool().clone())
        .latest(&lesson_id)
        .await
        .map(|run| run.map(lesson_audit_dto))
        .map_err(ApiError::from)
}

fn stopped(stop: &AuditStop, progress: Option<(usize, usize)>) -> LearningLessonAuditResultDto {
    LearningLessonAuditResultDto {
        outcome: LearningAuditOutcome::Stopped,
        message: Some(stop.message(progress)),
        fix: Some(stop.fix()),
        disputes: 0,
        publishable: matches!(stop, AuditStop::TurnedOff),
        audit: None,
    }
}

/// Audits a lesson on demand ("Audit this lesson now"), for a course with
/// the audit on. A dispute sends a ready lesson back to Needs review.
#[tauri::command]
#[specta::specta]
pub async fn audit_learning_lesson_now(
    program_id: String,
    lesson_id: String,
    operation_id: String,
    container: State<'_, Container>,
) -> Result<LearningLessonAuditResultDto, ApiError> {
    let container = container.inner();
    let pool = container.db_pool().clone();
    let programs = LearningCurriculumRepository::new(pool.clone());
    let setting = programs.verifier_audit(&program_id).await?;
    if !setting.on {
        return Err(AppError::InvalidInput(
            "Turn the Verifier audit on for this course before auditing a lesson.".into(),
        )
        .into());
    }
    let Some(model) = container.verifier_model().await? else {
        return Ok(stopped(&AuditStop::NoVerifier, None));
    };
    if let Some(stop) =
        AuditStop::from_consent(&consent(&model, setting.consent_provider.as_deref()))
    {
        return Ok(stopped(&stop, None));
    }
    let pinned = match container.verifier_llm(ModelRequirements::default()).await {
        Ok(pinned) => pinned,
        Err(error) => {
            let stop = AuditStop::Unavailable {
                message: error.to_string(),
            };
            return Ok(stopped(&stop, None));
        }
    };
    let store = claim_ledger(container)?;
    let material = audit_material(container, &program_id, &lesson_id).await?;
    // Web search is used only when the course allows it, a web service can
    // save what it fetches, and the Verifier can call tools; otherwise the
    // audit runs on the cited passages and notes why (§14.6).
    let web = verifier_web(container)
        .filter(|_| setting.web_search && pinned.llm.supports_tool_calling());
    let web_note = (setting.web_search && web.is_none()).then(|| {
        if pinned.llm.supports_tool_calling() {
            "Web search is not available, so the audit used the cited passages only.".to_owned()
        } else {
            "The Verifier cannot call tools, so the audit used the cited passages only.".to_owned()
        }
    });
    let runs = SqliteAuditRuns::new(pool);
    let gate = ProgramGate {
        programs,
        program_id: program_id.clone(),
        model: pinned.resolved.model.clone(),
    };
    let progress = |progress: audit::AuditProgress| {
        tracing::info!(operation_id, lesson_id, step = %progress.label(), "learning.audit.progress");
    };
    let request = AuditRequest {
        lesson_id: &lesson_id,
        job_id: None,
        app_version: env!("CARGO_PKG_VERSION"),
        draft_sha256: &material.draft_sha256,
        verifier_model: &pinned.resolved.model,
        units: &material.units,
        passages: &material.passages,
    };
    let deps = AuditDeps {
        store: store.as_ref(),
        runs: &runs,
        gate: &gate,
        web: web.as_deref(),
        progress: &progress,
        clock: &now,
    };
    let outcome = audit::run(pinned.llm.as_ref(), &request, &deps).await?;
    let summary = outcome.summary().clone();
    let progress = Some((summary.batches_done, summary.batches_planned));
    let (result, message, fix) = match &outcome {
        AuditOutcome::Completed(_) => (LearningAuditOutcome::Completed, None, None),
        AuditOutcome::Stopped { stop, .. } => (
            LearningAuditOutcome::Stopped,
            Some(stop.message(progress)),
            Some(stop.fix()),
        ),
        AuditOutcome::Deferred { error, .. } => (
            LearningAuditOutcome::Deferred,
            Some(format!(
                "Waiting to retry the audit · batch {} of {} · last error {error}",
                summary.batches_done + 1,
                summary.batches_planned
            )),
            Some("Audit this lesson again once the provider is reachable.".to_owned()),
        ),
    };
    if !outcome.publishable() {
        hold_for_review(store.as_ref(), &lesson_id, &outcome, progress).await?;
    }
    let audit = runs.latest(&lesson_id).await?.map(|run| {
        let mut dto = lesson_audit_dto(run);
        dto.web_note = dto.web_note.or_else(|| web_note.clone());
        dto
    });
    Ok(LearningLessonAuditResultDto {
        outcome: result,
        message,
        fix,
        disputes: signed(summary.disputes as u64),
        publishable: outcome.publishable(),
        audit,
    })
}

/// Sends the lesson's review back to Needs review, with the stop's reason
/// when the audit stopped. Disputed claims already wait in the ledger.
async fn hold_for_review(
    store: &dyn ClaimReviewStore,
    lesson_id: &str,
    outcome: &AuditOutcome,
    progress: Option<(usize, usize)>,
) -> CoreResult<()> {
    let Some(review) = store.load(lesson_id).await?.review else {
        return Ok(());
    };
    let stop_reason = match outcome {
        AuditOutcome::Stopped { stop, .. } => Some(stop.stop_reason(progress, now())),
        AuditOutcome::Completed(_) | AuditOutcome::Deferred { .. } => review.stop_reason.clone(),
    };
    store
        .set_lesson_review(
            lesson_id,
            LessonReview {
                state: LessonReviewState::NeedsReview,
                stop_reason,
                ..review
            },
        )
        .await?;
    Ok(())
}
