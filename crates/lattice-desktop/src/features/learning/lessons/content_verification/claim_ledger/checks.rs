//! What a pass records of a claim without reading it: the reopenings that need
//! no check, and the check of a code example. A statement's check is a
//! reading, which the acceptance policy decides and `record_reading` records
//! (§3, §4; acceptance design §2.10).

use super::super::*;
use super::fields::{field_text, locate};
use super::identify::Carried;
use super::observe::shown;
use super::ClaimLedger;
use crate::features::learning::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::domain::learning::claim_review::{
    new_information, reopen_cause, Check, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState,
    EvidenceFingerprint, Observation,
};

impl ClaimLedger {
    /// The reopenings a claim `identify` carried on needs no check for, made
    /// before anything is read: a verified claim whose resolving source was
    /// deleted or replaced, and a confirmed claim whose sentence or bound
    /// source changed. A claim reopened is the user's. `active` are the
    /// sources the course has now.
    pub(in super::super) async fn reopen_changed(
        &mut self,
        candidate: &Value,
        active: &HashSet<&str>,
        claims: &[ClaimId],
    ) -> Result<()> {
        for id in claims {
            let Some(Carried { before, quote, .. }) = self.carried.remove(id) else {
                continue;
            };
            let observation = self.unchecked_observation(candidate, &before, quote, active);
            let Some(change) = new_information(&before, &observation) else {
                continue;
            };
            let cause = self
                .entry(id)
                .and_then(|entry| reopen_cause(entry.state(), &change));
            if let Some(cause) = cause {
                self.append(id, ClaimEvent::Reopened { cause }).await?;
            }
        }
        Ok(())
    }

    /// What a pass sees of a claim before it reads it: the quote it stands on
    /// (the stored one while it is still in its field), the sources that are
    /// gone, and nothing a check would bring.
    fn unchecked_observation(
        &self,
        candidate: &Value,
        before: &ClaimLedgerEntry,
        quote: String,
        active: &HashSet<&str>,
    ) -> Observation {
        let location = before.location();
        let stands = field_text(candidate, location.unit_index, &location.field)
            .is_some_and(|text| text.contains(&location.quote));
        Observation {
            quote: if stands {
                location.quote.clone()
            } else {
                quote
            },
            fingerprint: before.basis().map_or_else(
                || EvidenceFingerprint::of(POLICY, &self.checker_model, &[]),
                |basis| basis.fingerprint.clone(),
            ),
            checker_model: self.checker_model.clone(),
            window_tokens: self.window_tokens,
            source_changes: Self::source_changes(before, active),
            hint: None,
            attached_source_version_id: None,
            gate: None,
        }
    }

    /// Shows `evidence` on the claim's card: the passages its latest reading
    /// compared.
    pub(in super::super) async fn show_evidence(
        &self,
        id: &ClaimId,
        evidence: &[EvidencePassage],
    ) -> Result<()> {
        let passages: Vec<ShownPassage> = evidence.iter().map(shown).collect();
        self.show(id, &passages).await
    }

    /// What a pass found for the code examples `identify` named: the check of
    /// each (§4), and what it shows to be new about it.
    pub(in super::super) async fn record_example_checks(
        &mut self,
        candidate: &Value,
        executions: &[execution::Observation],
        active: &HashSet<&str>,
    ) -> Result<()> {
        for observation in executions {
            let Some(id) = self.examples.get(&observation.id).cloned() else {
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
            self.check_claim(&id, candidate, quote, check, active)
                .await?;
            self.show(&id, &passages).await?;
        }
        Ok(())
    }

    /// The check of one claim, `quote` being where it was extracted: its
    /// first when the claim is new to the ledger. For a claim seen before, a
    /// check only when something about it is new or a repair awaited its
    /// verdict.
    async fn check_claim(
        &mut self,
        id: &ClaimId,
        candidate: &Value,
        quote: String,
        check: Check,
        active: &HashSet<&str>,
    ) -> Result<()> {
        let Some(Carried {
            before, decided, ..
        }) = self.carried.remove(id)
        else {
            self.append(id, ClaimEvent::Checked(check)).await?;
            return Ok(());
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
            gate: None,
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
            self.append(id, ClaimEvent::Checked(check)).await?;
        }
        Ok(())
    }
}
