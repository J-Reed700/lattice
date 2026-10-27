//! Trust changes in the claim ledger (source provenance §13, §18.7). An
//! override set or cleared, or a bar the user's verdicts moved, re-decides
//! every gate-decided claim the change touches with lattice-core's `decide`,
//! calling no model; each change is one `StandingAssessed` event, and a human
//! verdict is never touched. [`record_verdict`] and [`forget_verdict`] feed
//! calibration and undo it.

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    end_state, Actor, Check, CheckResult, Checker, ClaimEvent, ClaimLedgerEntry, ClaimState,
    EventProvenance, GateVerdict, Ledger, LessonReview, LessonReviewState, TrustAssessment,
    TrustCause, TrustVerdict,
};
use lattice_core::domain::provenance::{ClaimKind, EffectiveBars};
use lattice_core::shared::error::{AppError, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use super::repository::ClaimReviewRepository;
use crate::features::learning::content_verification::{gate_reason, relevel};
use crate::features::learning::persistence::now;
use crate::features::learning::references::trust::outcomes::{self, VerdictScope};
use crate::features::learning::references::trust::{calibration, overrides};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One claim a trust change would re-decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AffectedClaim {
    pub program_id: String,
    pub lesson_id: String,
    pub claim_id: String,
    /// Verified now: the change may reopen it. Otherwise it is failing its
    /// bar and the change may verify it.
    pub verified: bool,
}

/// What a trust change did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Redecided {
    /// Claims verified (or restored) that were failing their bar.
    pub verified: usize,
    /// Verified claims reopened for the user.
    pub reopened: usize,
    /// Claims still failing, with the new decision's reason.
    pub still_failing: usize,
    /// Claims an override touches whose supports were never judged, so
    /// only a model could level them; left as they were.
    pub not_levelled: usize,
    /// Lessons with a re-decided claim.
    pub lessons: Vec<String>,
}

impl Redecided {
    fn merge(&mut self, other: Redecided) {
        self.verified += other.verified;
        self.reopened += other.reopened;
        self.still_failing += other.still_failing;
        self.not_levelled += other.not_levelled;
        for lesson in other.lessons {
            if !self.lessons.contains(&lesson) {
                self.lessons.push(lesson);
            }
        }
    }
}

/// The ledger's cause for an override change (`overrides::{set, clear}`
/// answer with the versions it covers).
pub fn override_cause(change: &overrides::TrustChange) -> TrustCause {
    TrustCause::Override {
        source_version_ids: change
            .versions
            .iter()
            .map(|covered| covered.source_version_id.clone())
            .collect(),
    }
}

/// Every claim a trust change could re-decide, with the gate's assessment
/// it would re-decide from, read once: the override menu counts what each
/// of several scopes would reach from one read of the ledgers.
pub struct Redecidable {
    claims: Vec<(AffectedClaim, TrustAssessment)>,
}

impl Redecidable {
    pub async fn load(pool: &SqlitePool) -> Result<Self> {
        let store = ClaimReviewRepository::new(pool.clone());
        let mut claims = Vec::new();
        for lesson_id in weighed_lessons(pool).await? {
            let ledger = store.load(&lesson_id).await?;
            let Some(review) = &ledger.review else {
                continue;
            };
            for entry in ledger.claims.iter().filter(|entry| entry.redecidable()) {
                let Some(trust) = entry.trust() else {
                    continue;
                };
                let claim = AffectedClaim {
                    program_id: review.program_id.clone(),
                    lesson_id: lesson_id.clone(),
                    claim_id: entry.claim_id().0.clone(),
                    verified: matches!(entry.state(), ClaimState::ResolvedByEvidence { .. }),
                };
                claims.push((claim, trust.clone()));
            }
        }
        Ok(Self { claims })
    }

    /// The claims `cause` would re-decide.
    pub fn affected<'a>(
        &'a self,
        cause: &'a TrustCause,
    ) -> impl Iterator<Item = &'a AffectedClaim> + 'a {
        self.claims
            .iter()
            .filter(move |(_, trust)| cause.affects(trust))
            .map(|(claim, _)| claim)
    }
}

/// The claims `cause` would re-decide, without changing anything: what
/// the override menu counts before the user commits.
pub async fn affected(pool: &SqlitePool, cause: &TrustCause) -> Result<Vec<AffectedClaim>> {
    Ok(Redecidable::load(pool)
        .await?
        .affected(cause)
        .cloned()
        .collect())
}

/// Re-decides every claim `cause` touches (§18.7), with no model call.
pub async fn trust_changed(pool: &SqlitePool, cause: &TrustCause) -> Result<Redecided> {
    let bars = calibration::bars(pool).await?;
    let mut report = Redecided::default();
    for lesson_id in weighed_lessons(pool).await? {
        report.merge(redecide_lesson(pool, &lesson_id, cause, &bars).await?);
    }
    Ok(report)
}

/// Re-decides the claims an override change covers: call after
/// `overrides::set` or `overrides::clear` with what it returned.
pub async fn override_changed(
    pool: &SqlitePool,
    change: &overrides::TrustChange,
) -> Result<Redecided> {
    trust_changed(pool, &override_cause(change)).await
}

/// Records the calibration outcomes of the user's `verdict` on `entry` (the
/// claim before the verdict), then re-decides the claims of every kind
/// whose bar the outcomes moved.
pub async fn record_verdict(
    pool: &SqlitePool,
    scope: VerdictScope<'_>,
    entry: &ClaimLedgerEntry,
    verdict: TrustVerdict,
) -> Result<Redecided> {
    let moved = outcomes::record(pool, scope, entry, verdict).await?;
    redecide_kinds(pool, moved).await
}

/// Forgets the outcomes of an undone `verdict` and re-decides the claims
/// of every kind whose bar that moved back.
pub async fn forget_verdict(
    pool: &SqlitePool,
    claim_id: &str,
    verdict: TrustVerdict,
) -> Result<Redecided> {
    let moved = outcomes::forget(pool, claim_id, verdict).await?;
    redecide_kinds(pool, moved).await
}

/// Records the Verifier's agreement with claims the gate accepted (§13:
/// "Verifier agrees with an accepted claim" is `true, true`): each claim
/// with a supported Verifier check appended since `before` is scored as it
/// stood in `before`, then the kinds whose bar that moved are re-decided.
/// `trust_outcomes` gives nothing for a claim the gate held back or did not
/// weigh: the Verifier judges entailment, not standing.
pub async fn record_verifier_agreements(
    pool: &SqlitePool,
    scope: VerdictScope<'_>,
    before: &Ledger,
) -> Result<Redecided> {
    let after = ClaimReviewRepository::new(pool.clone())
        .load(scope.lesson_id)
        .await?;
    let mut report = Redecided::default();
    for entry in &after.claims {
        let Some(prior) = before
            .claims
            .iter()
            .find(|prior| prior.claim_id() == entry.claim_id())
        else {
            continue;
        };
        let agreed = entry
            .events()
            .iter()
            .skip(prior.events().len())
            .any(|record| {
                matches!(
                    &record.event,
                    ClaimEvent::Checked(Check {
                        checker: Checker::Verifier { .. },
                        result: CheckResult::Supported { .. },
                        ..
                    })
                )
            });
        if agreed {
            report.merge(record_verdict(pool, scope, prior, TrustVerdict::VerifierAgreed).await?);
        }
    }
    Ok(report)
}

async fn redecide_kinds(pool: &SqlitePool, kinds: Vec<ClaimKind>) -> Result<Redecided> {
    let mut report = Redecided::default();
    for kind in kinds {
        report.merge(trust_changed(pool, &TrustCause::Bar { kind }).await?);
    }
    Ok(report)
}

/// Lessons with a claim the gate weighed: the only ones a trust change can
/// touch.
async fn weighed_lessons(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT DISTINCT c.lesson_id FROM learning_lesson_claim_events e \
         JOIN learning_lesson_claims c ON c.claim_id = e.claim_id \
         WHERE e.kind = 'standing_assessed' ORDER BY c.lesson_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| AppError::Database(error.to_string()))?;
    rows.iter()
        .map(|row| {
            row.try_get::<String, _>("lesson_id")
                .map_err(|error| AppError::Database(error.to_string()))
        })
        .collect()
}

/// The claims of a ledger `cause` can re-decide.
fn touched<'a>(
    claims: &'a [ClaimLedgerEntry],
    cause: &'a TrustCause,
) -> impl Iterator<Item = &'a ClaimLedgerEntry> + 'a {
    claims.iter().filter(move |entry| {
        entry.redecidable() && entry.trust().is_some_and(|trust| cause.affects(trust))
    })
}

/// One lesson's re-decisions, appended under its review's revision. When a
/// running job or the user writes meanwhile, the ledger is read again and
/// what is left is re-decided against it (a claim already re-decided
/// decides the same and is skipped).
async fn redecide_lesson(
    pool: &SqlitePool,
    lesson_id: &str,
    cause: &TrustCause,
    bars: &EffectiveBars,
) -> Result<Redecided> {
    let store = ClaimReviewRepository::new(pool.clone());
    let mut report = Redecided::default();
    'reload: loop {
        let ledger = store.load(lesson_id).await?;
        let Some(review) = ledger.review.clone() else {
            return Ok(report);
        };
        let mut revision = ledger.revision;
        for entry in touched(&ledger.claims, cause) {
            let Some(trust) = entry.trust() else {
                continue;
            };
            let supports = match cause {
                TrustCause::Bar { .. } => trust.supports.clone(),
                TrustCause::Override { .. } => {
                    match relevel(pool, &review.program_id, trust).await {
                        Ok(supports) => supports,
                        Err(AppError::NotFound(_)) => {
                            report.not_levelled += 1;
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                }
            };
            let next = trust.redecide(supports, bars);
            if next.decision == trust.decision && next.supports == trust.supports {
                continue;
            }
            let verdict = next.verdict();
            let event = ClaimEvent::StandingAssessed {
                reason: gate_reason(&next),
                trust: Box::new(next),
                cause: Some(cause.clone()),
            };
            match store
                .append(
                    lesson_id,
                    entry.claim_id(),
                    machine(&review),
                    event,
                    revision,
                )
                .await
            {
                Ok(written) => {
                    revision = written;
                    tally(&mut report, entry.state(), &verdict);
                    if !report.lessons.iter().any(|lesson| lesson == lesson_id) {
                        report.lessons.push(lesson_id.to_owned());
                    }
                }
                Err(AppError::InvalidState(reason)) => {
                    if store.load(lesson_id).await?.revision != revision {
                        continue 'reload;
                    }
                    tracing::warn!(
                        claim = %entry.claim_id().0,
                        %reason,
                        "Trust change not recorded for a claim"
                    );
                }
                Err(error) => return Err(error),
            }
        }
        if report.reopened > 0 {
            hold_if_published(&store, lesson_id).await?;
        }
        return Ok(report);
    }
}

/// A published lesson with a claim reopened needs the user's review again,
/// as a Verifier dispute on demand does; the lesson stays published.
async fn hold_if_published(store: &ClaimReviewRepository, lesson_id: &str) -> Result<()> {
    let ledger = store.load(lesson_id).await?;
    let Some(review) = ledger.review else {
        return Ok(());
    };
    if review.state == LessonReviewState::Ready
        && end_state(&ledger.claims) == LessonReviewState::NeedsReview
    {
        store
            .set_lesson_review(
                lesson_id,
                LessonReview {
                    state: LessonReviewState::NeedsReview,
                    ..review
                },
            )
            .await?;
    }
    Ok(())
}

fn tally(report: &mut Redecided, before: &ClaimState, verdict: &GateVerdict) {
    let verified = matches!(before, ClaimState::ResolvedByEvidence { .. });
    match verdict {
        GateVerdict::Sufficient if !verified => report.verified += 1,
        GateVerdict::Sufficient => {}
        GateVerdict::Fails(_) if verified => report.reopened += 1,
        GateVerdict::Fails(_) => report.still_failing += 1,
    }
}

/// A re-decision's provenance: the machine, no job, the review's checker.
fn machine(review: &LessonReview) -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at: now(),
        job_id: None,
        app_version: APP_VERSION.into(),
        checker_model: Some(review.checker_model.clone()),
    }
}
