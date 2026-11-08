//! One lesson's audit as a preparation job and "Audit this lesson now" run
//! it (§14.3, §14.6): the course's opt-in and consent, the pinned Verifier,
//! the web tools when the course allows them, then [`super::run`].
//!
//! The Verifier comes from the container, which learning's lessons never
//! name: a job receives it through [`scope`], a task-local set around the
//! preparation like research's web, and the command passes its own.

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use lattice_core::application::ports::llm_port::PinnedModel;
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::Result;

use super::material::AuditMaterial;
use super::policy::consent;
use super::runs::{ProgramGate, SqliteAuditRuns};
use super::tools::VerifierWeb;
use super::{AuditDeps, AuditOutcome, AuditProgress, AuditRequest, AuditStop};
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::trust_changed;
use crate::features::learning::persistence::now;
use crate::features::learning::references::trust::outcomes::VerdictScope;
use lattice_core::application::ports::claim_review::ClaimReviewStore;

/// The Verifier as the app configures it.
#[async_trait]
pub(crate) trait VerifierAccess: Send + Sync {
    /// The Verifier reference in Settings, unloaded (consent names its
    /// provider); `None` when no Verifier is set.
    async fn model(&self) -> Result<Option<ModelRef>>;
    /// The Verifier, loaded and pinned for one audit.
    async fn pin(&self) -> Result<PinnedModel>;
    /// The web tools for `program_id`'s audit, when a web service is there.
    fn web(&self, program_id: &str) -> Option<Arc<dyn VerifierWeb>>;
}

tokio::task_local! { static VERIFIER: Arc<dyn VerifierAccess>; }

/// Runs a preparation with `verifier` reachable from its review loop;
/// without one, lessons publish without the audit gate.
pub(crate) async fn scope<T>(
    verifier: Option<Arc<dyn VerifierAccess>>,
    future: impl Future<Output = T>,
) -> T {
    match verifier {
        Some(verifier) => VERIFIER.scope(verifier, future).await,
        None => future.await,
    }
}

/// The running preparation's Verifier, if it has one.
pub(crate) fn verifier() -> Option<Arc<dyn VerifierAccess>> {
    VERIFIER.try_with(Arc::clone).ok()
}

pub(crate) struct LessonAuditRequest<'a> {
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub job_id: Option<&'a str>,
    pub material: &'a AuditMaterial,
    pub progress: &'a (dyn Fn(AuditProgress) + Send + Sync),
}

#[derive(Debug)]
pub(crate) enum LessonAudit {
    /// The course has not opted in: there is no audit gate.
    Off,
    /// The audit is on but cannot send anything; nothing was sent.
    Refused(AuditStop),
    /// The audit ran, or stopped at a batch boundary. `web_note` says why
    /// web search was allowed and not used.
    Ran {
        outcome: AuditOutcome,
        web_note: Option<String>,
    },
}

/// Audits the lesson when its course opted in and the Verifier may be sent
/// its text.
///
/// # Errors
///
/// Storage failures, and the audit's own (see [`super::run`]).
pub(crate) async fn audit_lesson(
    verifier: &dyn VerifierAccess,
    pool: &SqlitePool,
    request: &LessonAuditRequest<'_>,
) -> Result<LessonAudit> {
    let programs = LearningCurriculumRepository::new(pool.clone());
    let setting = programs.verifier_audit(request.program_id).await?;
    if !setting.on {
        return Ok(LessonAudit::Off);
    }
    let Some(model) = verifier.model().await? else {
        return Ok(LessonAudit::Refused(AuditStop::NoVerifier));
    };
    if let Some(stop) =
        AuditStop::from_consent(&consent(&model, setting.consent_provider.as_deref()))
    {
        return Ok(LessonAudit::Refused(stop));
    }
    let pinned = match verifier.pin().await {
        Ok(pinned) => pinned,
        Err(error) => {
            return Ok(LessonAudit::Refused(AuditStop::Unavailable {
                message: error.to_string(),
            }))
        }
    };
    // Web search is used only when the course allows it, a web service can
    // save what it fetches, and the Verifier can call tools; otherwise the
    // audit runs on the cited passages and notes why (§14.6).
    let tools = pinned.llm.supports_tool_calling();
    let web = verifier
        .web(request.program_id)
        .filter(|_| setting.web_search && tools);
    let web_note = (setting.web_search && web.is_none()).then(|| {
        if tools {
            "Web search is not available, so the audit used the cited passages only.".to_owned()
        } else {
            "The Verifier cannot call tools, so the audit used the cited passages only.".to_owned()
        }
    });
    let store = ClaimReviewRepository::new(pool.clone());
    let runs = SqliteAuditRuns::new(pool.clone());
    let gate = ProgramGate {
        programs,
        program_id: request.program_id.to_owned(),
        model: pinned.resolved.model.clone(),
    };
    let audit = AuditRequest {
        lesson_id: request.lesson_id,
        job_id: request.job_id,
        app_version: env!("CARGO_PKG_VERSION"),
        draft_sha256: &request.material.draft_sha256,
        verifier_model: &pinned.resolved.model,
        units: &request.material.units,
        passages: &request.material.passages,
    };
    let deps = AuditDeps {
        store: &store,
        runs: &runs,
        gate: &gate,
        web: web.as_deref(),
        progress: request.progress,
        clock: &now,
    };
    let before = store.load(request.lesson_id).await?;
    let outcome = super::run(pinned.llm.as_ref(), &audit, &deps).await;
    // The checks the audit appended stay on the ledger even when it stops
    // with an error, so their agreements are calibrated either way (§13).
    trust_changed::record_verifier_agreements(
        pool,
        VerdictScope {
            program_id: request.program_id,
            lesson_id: request.lesson_id,
        },
        &before,
    )
    .await?;
    Ok(LessonAudit::Ran {
        outcome: outcome?,
        web_note,
    })
}
