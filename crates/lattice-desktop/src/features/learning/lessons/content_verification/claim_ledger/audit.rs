//! The Verifier audit's place in the loop (§14.3): after the review pass has
//! resolved every claim and before the lesson publishes, when the course
//! opted in. A dispute, or an audit that is on and cannot finish, holds the
//! lesson for the user's review; a provider outage unwinds the preparation,
//! which the job runtime defers, and the next attempt audits only the claims
//! still without a receipt.

use super::*;
use crate::features::learning::lessons::verifier_audit::lesson::{
    self, LessonAudit, LessonAuditRequest,
};
use crate::features::learning::lessons::verifier_audit::material::AuditMaterial;
use crate::features::learning::lessons::verifier_audit::{AuditOutcome, AuditProgress};
use lattice_core::domain::learning::claim_review::StopReason;

/// What the audit lets the loop do with a lesson whose claims are resolved.
pub(in crate::features::learning) enum Audited {
    /// No audit gate: the course has not opted in, or no preparation job
    /// with a Verifier is running.
    Off,
    /// The Verifier agrees with every claim: the lesson may publish.
    Agreed,
    /// The lesson waits for the user: claims the Verifier disputes, or the
    /// stop that kept the audit from finishing (boxed: a stop is far larger
    /// than the other variants).
    Hold(Option<Box<StopReason>>),
}

impl ClaimLedger {
    /// Audits `raw` (parsed as `candidate`) with the job's Verifier.
    pub(in crate::features::learning) async fn audit(
        &self,
        raw: &str,
        candidate: &Value,
    ) -> Result<Audited> {
        let scope = crate::features::learning::lesson_drafts::scope();
        let (Some(sink), Some(verifier), Some(scope)) = (&self.sink, lesson::verifier(), scope)
        else {
            return Ok(Audited::Off);
        };
        let material = AuditMaterial::of_draft(
            &scope.pool,
            &sink.program_id,
            &sink.lesson_id,
            candidate,
            digest(raw),
        )
        .await?;
        let progress = |progress: AuditProgress| {
            crate::features::learning::lesson_progress::phase(
                crate::features::learning::lesson_progress::Phase::Evidence,
                progress.label(),
            );
        };
        let audit = lesson::audit_lesson(
            verifier.as_ref(),
            &scope.pool,
            &LessonAuditRequest {
                program_id: &sink.program_id,
                lesson_id: &sink.lesson_id,
                job_id: Some(&sink.job_id),
                material: &material,
                progress: &progress,
            },
        )
        .await?;
        Ok(match audit {
            LessonAudit::Off => Audited::Off,
            LessonAudit::Refused(stop) => {
                tracing::info!(lesson_id = %sink.lesson_id, code = stop.code(), "lesson.audit.refused");
                Audited::Hold(Some(Box::new(stop.stop_reason(None, now()))))
            }
            LessonAudit::Ran { outcome, web_note } => {
                let summary = outcome.summary();
                tracing::info!(
                    lesson_id = %sink.lesson_id,
                    batches = summary.batches_done,
                    disputes = summary.disputes,
                    input_tokens = summary.input_tokens,
                    output_tokens = summary.output_tokens,
                    web_note = web_note.as_deref().unwrap_or_default(),
                    "lesson.audit.ran"
                );
                if outcome.publishable() {
                    Audited::Agreed
                } else {
                    match outcome {
                        AuditOutcome::Deferred { error, .. } => return Err(error),
                        AuditOutcome::Stopped { stop, summary } => {
                            Audited::Hold(Some(Box::new(stop.stop_reason(
                                Some((summary.batches_done, summary.batches_planned)),
                                now(),
                            ))))
                        }
                        AuditOutcome::Completed(_) => Audited::Hold(None),
                    }
                }
            }
        })
    }

    /// Claims waiting for the user or still the machine's.
    pub(in crate::features::learning) fn awaiting_user(&self) -> usize {
        let counts = ClaimCounts::tally(self.entries.iter());
        counts.queued + counts.open
    }
}
