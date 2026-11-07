//! The record of a lesson's cuts as the ledger keeps it (claim classes design
//! §3.10): saved with the job, so a restart finds the sections that go and the
//! flow repairs made where it left them.

use super::super::sections::{Going, NOTHING_LEFT};
use super::*;

/// The job checkpoint holding the record.
const CUTS_KEY: &str = "claim-ledger-v1:cuts";
/// The code of the stop for a lesson its sources do not support enough of.
pub(super) const UNSUPPORTED: &str = "sources_do_not_support";

impl ClaimLedger {
    pub(super) async fn load_cuts(&mut self) -> Result<()> {
        self.cuts = crate::features::learning::lesson_drafts::checkpoint(CUTS_KEY)
            .await?
            .and_then(|saved| serde_json::from_value(saved).ok())
            .unwrap_or_default();
        Ok(())
    }

    pub(in crate::features::learning::lessons::content_verification) async fn save_cuts(
        &self,
    ) -> Result<()> {
        crate::features::learning::lesson_drafts::record_checkpoint(
            CUTS_KEY,
            serde_json::to_value(&self.cuts)?,
        )
        .await
    }

    pub(in crate::features::learning::lessons::content_verification) async fn clear_cuts(
        &mut self,
    ) -> Result<()> {
        self.cuts = Default::default();
        self.save_cuts().await
    }

    /// These claims end removed by the machine: the last attempt on each took
    /// its sentence out, or could not and took its section out instead.
    pub(in crate::features::learning::lessons::content_verification) async fn remove(
        &mut self,
        claims: &[ClaimId],
    ) -> Result<()> {
        self.close_records(claims, RepairOutcome::Removed).await
    }

    /// Whether the machine's cuts leave the lesson no statement: it has taken
    /// one out, and every claim is out, is one of `cut` (taken out just now),
    /// or stands in a section that goes.
    pub(in crate::features::learning::lessons::content_verification) fn nothing_left(
        &self,
        cut: &[ClaimId],
    ) -> bool {
        let took = !cut.is_empty()
            || self.entries.iter().any(|entry| {
                matches!(
                    entry.state(),
                    ClaimState::Removed {
                        by: Actor::Machine,
                        ..
                    }
                )
            });
        took && self.entries.iter().all(|entry| {
            is_gone(entry.state())
                || cut.contains(entry.claim_id())
                || self.cuts.going.contains_key(&entry.location().unit_index)
        })
    }

    /// The last attempts took out every statement the lesson had: the claims
    /// they cut end removed, as the next pass would end them, and the lesson
    /// stops with the draft as cut.
    pub(in crate::features::learning::lessons::content_verification) async fn stop_cut_out(
        &mut self,
        cut: &[ClaimId],
        raw: &str,
    ) -> Result<AppError> {
        self.remove(cut).await?;
        crate::features::learning::lesson_drafts::save(raw).await?;
        self.stop_unsupported(raw, NOTHING_LEFT).await
    }

    /// The claims still standing in a unit.
    pub(in crate::features::learning::lessons::content_verification) fn live_in(
        &self,
        unit: usize,
    ) -> Vec<ClaimId> {
        self.entries
            .iter()
            .filter(|entry| !is_gone(entry.state()) && entry.location().unit_index == unit)
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// The claims of the findings of `report` that stay once the units
    /// `going` are out, in its order: each finding the lesson publishes is
    /// still beside its own claim.
    pub(in crate::features::learning::lessons::content_verification) fn keep_findings(
        &mut self,
        report: &LessonVerificationReport,
        going: &Going,
    ) {
        let mut units = report.findings.iter().map(|finding| finding.unit);
        self.findings
            .retain(|_| units.next().is_some_and(|unit| going.after(unit).is_some()));
    }

    /// Moves the claims standing in the units after the ones that go to the
    /// places those units have once the others are out, so a claim keeps its
    /// state, its attempts and its history.
    pub(in crate::features::learning::lessons::content_verification) async fn relocate(
        &mut self,
        going: &Going,
    ) -> Result<()> {
        let moves: Vec<(ClaimId, ClaimLocation, String)> = self
            .entries
            .iter()
            .filter(|entry| !is_gone(entry.state()))
            .filter_map(|entry| {
                let unit = entry.location().unit_index;
                let now = going.after(unit).filter(|now| *now != unit)?;
                Some((
                    entry.claim_id().clone(),
                    ClaimLocation {
                        unit_index: now,
                        ..entry.location().clone()
                    },
                    entry.statement().to_owned(),
                ))
            })
            .collect();
        for (id, location, statement) in moves {
            self.append(
                &id,
                ClaimEvent::Extracted {
                    location,
                    statement,
                },
            )
            .await?;
        }
        Ok(())
    }

    /// The lesson stops: its sources do not support enough of it, `what`
    /// being the part of it that would have to go. The review row says so,
    /// and the draft is kept as it is.
    pub(in crate::features::learning::lessons::content_verification) async fn stop_unsupported(
        &mut self,
        raw: &str,
        what: &str,
    ) -> Result<AppError> {
        let reason = StopReason {
            code: UNSUPPORTED.into(),
            message: format!("The sources do not support enough of this lesson: {what}."),
            fix: "Add sources that cover what this lesson teaches, or change what its outline asks of it, then prepare it again.".into(),
            at: now(),
            request: None,
        };
        self.write_review(LessonReviewState::Stopped, raw, Some(reason.clone()))
            .await?;
        Ok(AppError::Other(reason.message))
    }
}
