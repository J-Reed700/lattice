//! Recording what a pass checked: the check of each claim `identify` named,
//! when something about the claim is new or a repair awaits its verdict, and
//! the passages its card shows (§3, §4 `ledger.observe`).

use super::super::*;
use super::fields::{field_text, locate};
use super::identify::Carried;
use super::observe::{assessed, shown};
use super::ClaimLedger;
use crate::features::learning::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::domain::learning::claim_review::{
    new_information, reopen_cause, Check, ClaimEvent, ClaimId, ClaimState, Observation,
    TrustAssessment,
};

impl ClaimLedger {
    /// What a pass found for the claims `identify` named: the check of each
    /// (and the gate's assessment), and what the check shows to be new (§4
    /// `ledger.observe`). `ids` are the claims as `identify` returned them:
    /// those of `report`'s findings, in order, then those of its examples.
    pub(in super::super) async fn record_checks(
        &mut self,
        candidate: &Value,
        report: &LessonVerificationReport,
        ids: &[ClaimId],
    ) -> Result<()> {
        let active: HashSet<&str> = report
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        for (finding, id) in report.findings.iter().zip(ids) {
            let quote = locate(
                candidate,
                finding.unit,
                &finding.quote,
                Some(&finding.statement),
            )
            .quote;
            let (check, trust) = self.check_of(finding);
            self.check_claim(id, candidate, quote, (check, trust), &active)
                .await?;
            let passages: Vec<ShownPassage> = finding.evidence.iter().map(shown).collect();
            self.show(id, &passages).await?;
        }
        for id in ids.iter().skip(report.findings.len()) {
            let Some(observation) = report
                .executions
                .iter()
                .find(|observation| self.examples.get(&observation.id) == Some(id))
            else {
                continue;
            };
            let quote = locate(
                candidate,
                observation.unit.unwrap_or(0),
                observation.code(),
                None,
            )
            .quote;
            let check = self.example_check(observation);
            let passages = [ShownPassage {
                source_version_id: format!("execution:{}", observation.id),
                start_byte: 0,
                end_byte: check.reason.len(),
                text: check.reason.clone(),
                retrieval_kind: "execution".into(),
            }];
            self.check_claim(id, candidate, quote, (check, None), &active)
                .await?;
            self.show(id, &passages).await?;
        }
        self.unpublishable = report
            .findings
            .iter()
            .zip(ids)
            .filter(|(finding, _)| !finding.publishable())
            .map(|(_, id)| id.clone())
            .collect();
        crate::features::learning::lesson_progress::claims(self.tally());
        Ok(())
    }

    /// The check of one claim, `quote` being where it was extracted: its
    /// first when the claim is new to the ledger. For a claim seen before, a
    /// check only when something about it is new (a different decision of the
    /// gate counts, source provenance §18.7) or a repair awaited its verdict.
    async fn check_claim(
        &mut self,
        id: &ClaimId,
        candidate: &Value,
        quote: String,
        (check, trust): (Check, Option<TrustAssessment>),
        active: &HashSet<&str>,
    ) -> Result<()> {
        let Some(Carried { before, decided }) = self.carried.remove(id) else {
            return self.append_check(id, check, trust).await;
        };
        // A claim stands on its sentence, not on the quote an extraction
        // picked: while the stored quote is still in its field, its sentence
        // did not change, whatever the claim's state (§3 `relitigate`; claim
        // sources design §18.4 i). A stored passage re-located to one of its
        // sentences takes the new location and nothing else; what else is new
        // about the claim is weighed as for any other.
        let stands = field_text(
            candidate,
            before.location().unit_index,
            &before.location().field,
        )
        .is_some_and(|text| text.contains(&before.location().quote));
        let observation = Observation {
            quote: if stands {
                before.location().quote.clone()
            } else {
                quote
            },
            fingerprint: check.fingerprint.clone(),
            checker_model: self.checker_model.clone(),
            window_tokens: self.window_tokens,
            source_changes: Self::source_changes(&before, active),
            hint: None,
            attached_source_version_id: None,
            gate: trust.as_ref().map(TrustAssessment::verdict),
        };
        let information = new_information(&before, &observation);
        if let Some(change) = &information {
            let cause = self
                .entry(id)
                .and_then(|entry| reopen_cause(entry.state(), change));
            if let Some(cause) = cause {
                self.append(id, ClaimEvent::Reopened { cause }).await?;
            }
        }
        let unchecked = matches!(before.state(), ClaimState::Unchecked);
        // A claim the checker could not answer for is asked again.
        let unanswered = before.unjudged() > 0;
        if unchecked || unanswered || decided || information.is_some() {
            self.append_check(id, check, trust).await?;
        } else if !before.state().is_settled()
            && before
                .basis()
                .is_some_and(|basis| basis.fingerprint == check.fingerprint)
        {
            // A new standing policy/reach can leave the verdict unchanged.
            // Persist its assessment anyway, or a held legacy claim keeps
            // looking unprocessed and Resume remains enabled forever.
            // Never transplant an assessment from reselected evidence.
            if let Some(trust) = trust.filter(|trust| before.trust() != Some(trust)) {
                self.append(id, assessed(trust)).await?;
            }
        }
        Ok(())
    }

    /// A check, then the gate's assessment of it when the gate decided it.
    async fn append_check(
        &mut self,
        id: &ClaimId,
        check: Check,
        trust: Option<TrustAssessment>,
    ) -> Result<()> {
        self.append(id, ClaimEvent::Checked(check)).await?;
        if let Some(trust) = trust {
            self.append(id, assessed(trust)).await?;
        }
        Ok(())
    }
}
