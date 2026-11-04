//! Identifying a pass: which claim each statement it extracted is, and what is
//! new about it that needs no check. `match_pass` computes it and records
//! nothing; `identify` records it (acceptance design §2.10).

use super::super::*;
use super::fields::{field_text, locate};
use super::{is_gone, successor, ClaimLedger, RepairRecord};
use lattice_core::domain::learning::claim_review::{
    identities, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ExtractedClaim, RepairOutcome,
};
use lattice_core::shared::text::sentences;

/// The statement a code example's claim makes (§2.1: an example that cannot
/// run is a claim like any other).
fn example_statement(observation_id: &str) -> String {
    format!("The code example {observation_id} runs to completion with the output it shows.")
}

/// A statement of a pass before anything is checked: where it stands and what
/// it says, the class it was sorted into, and, for a code example's claim,
/// the observation it stands for.
pub(in super::super) struct Extracted {
    pub(in super::super) claim: ExtractedClaim,
    pub(in super::super) class: Option<Classed>,
    pub(in super::super) example: Option<String>,
}

/// Which claim each statement of a pass continues, computed and not yet
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) struct Matched {
    /// Per statement, in order: the claim it continues, a repair's heir
    /// included; `None` for a statement new to the ledger.
    continues: Vec<Option<ClaimId>>,
    /// The claims no statement continues.
    orphaned: Vec<ClaimId>,
    /// A statement a repair wrote beside its parent's successor → the parent
    /// and the attempts the child inherits.
    children: HashMap<usize, (ClaimId, u8)>,
    /// A repaired claim with no successor → the text its repair wrote in its
    /// field (`None` when the repair only took text out).
    rewritten: HashMap<ClaimId, Option<String>>,
}

impl Matched {
    /// The claim statement `index` continues; `None` when it is new.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in super::super) fn continues(&self, index: usize) -> Option<&ClaimId> {
        self.continues.get(index)?.as_ref()
    }
}

/// A claim `identify` carried on, as `record_checks` weighs it: its entry as
/// it stood before the pass, and whether the pass decided a repair that
/// awaited it.
pub(super) struct Carried {
    pub(super) before: ClaimLedgerEntry,
    pub(super) decided: bool,
}

impl ClaimLedger {
    /// The statements `report` found, as `match_pass` and `identify` take
    /// them: each finding, then each code example that failed or that the
    /// ledger tracks.
    pub(in super::super) fn extracted(
        &self,
        candidate: &Value,
        report: &LessonVerificationReport,
    ) -> Vec<Extracted> {
        let mut extracted: Vec<Extracted> = report
            .findings
            .iter()
            .map(|finding| Extracted {
                claim: ExtractedClaim {
                    location: locate(
                        candidate,
                        finding.unit,
                        &finding.quote,
                        Some(&finding.statement),
                    ),
                    statement: finding.statement.clone(),
                },
                class: finding.class,
                example: None,
            })
            .collect();
        let located = extracted
            .iter()
            .filter(|found| sentences(&found.claim.location.quote).len() == 1)
            .count();
        tracing::info!(
            "claim.locate: {located} of {} claims located to one sentence; {} kept their passage",
            extracted.len(),
            extracted.len() - located
        );
        for observation in &report.executions {
            let statement = example_statement(&observation.id);
            let tracked = self
                .entries
                .iter()
                .any(|entry| entry.statement() == statement && !is_gone(entry.state()));
            if observation.passed() && !tracked {
                continue;
            }
            extracted.push(Extracted {
                claim: ExtractedClaim {
                    location: locate(
                        candidate,
                        observation.unit.unwrap_or(0),
                        observation.code(),
                        None,
                    ),
                    statement,
                },
                class: None,
                example: Some(observation.id.clone()),
            });
        }
        extracted
    }

    /// Which claim each extracted statement (and failed example) of a pass
    /// continues, a repair's heirs included, and the claims none continues:
    /// computed, nothing recorded. The pending repairs and the lesson before
    /// them are read, not taken.
    pub(in super::super) fn match_pass(
        &self,
        candidate: &Value,
        extracted: &[Extracted],
    ) -> Matched {
        let reference = self.pending.previous.as_ref().unwrap_or(candidate);
        let prior_ids: Vec<ClaimId> = self
            .entries
            .iter()
            .filter(|entry| !is_gone(entry.state()) || self.edited_now.contains(entry.claim_id()))
            .map(|entry| entry.claim_id().clone())
            .collect();
        let texts: HashMap<(usize, String), String> = prior_ids
            .iter()
            .filter_map(|id| self.entry(id))
            .filter_map(|entry| {
                let location = entry.location();
                field_text(reference, location.unit_index, &location.field)
                    .map(|text| ((location.unit_index, location.field.clone()), text))
            })
            .collect();
        let claims: Vec<ExtractedClaim> = extracted
            .iter()
            .map(|statement| statement.claim.clone())
            .collect();
        let mut assigned = {
            let prior: Vec<&ClaimLedgerEntry> =
                prior_ids.iter().filter_map(|id| self.entry(id)).collect();
            identities(&prior, &claims, |unit, field| {
                texts.get(&(unit, field.to_owned())).map(String::as_str)
            })
        };
        // A repaired sentence the matching rules lost (a rewrite that changed
        // its statement where no single span can be named) continues as the
        // new text its repair wrote in the same field, never as a sentence
        // that was there before; the other claims that repair wrote are its
        // children. Both keep the claim's attempts, so a claim the machine
        // keeps rewriting still reaches the user (§4 "Bounded"; claim
        // sources design §9).
        let owned: Vec<bool> = assigned.assigned.iter().map(Option::is_some).collect();
        let heirs = successor::heirs(
            &self.pending.repairs,
            &assigned.orphaned,
            (&claims, &owned),
            (self.pending.previous.as_ref(), candidate),
            |id| self.entry(id).map(|entry| entry.location().field.clone()),
            field_text,
        );
        for (parent, index) in &heirs.successors {
            if let Some(position) = assigned.orphaned.iter().position(|id| id == parent) {
                if let Some(slot) = assigned.assigned.get_mut(*index) {
                    *slot = Some(assigned.orphaned.remove(position));
                }
            }
        }
        Matched {
            continues: assigned.assigned,
            orphaned: assigned.orphaned,
            children: heirs.children,
            rewritten: heirs.rewritten,
        }
    }

    /// Records what `matched` found: which claim each statement is, and what
    /// is new about it that needs no check. Returns the claims, in the order
    /// of `extracted`. The outcome of a repair that awaited this pass is
    /// recorded here, and `record_checks` is left what it weighs of each
    /// claim carried on.
    pub(in super::super) async fn identify(
        &mut self,
        _candidate: &Value,
        extracted: Vec<Extracted>,
        matched: Matched,
    ) -> Result<Vec<ClaimId>> {
        let Matched {
            continues,
            orphaned,
            children,
            rewritten,
        } = matched;
        let records: HashMap<ClaimId, RepairRecord> = std::mem::take(&mut self.pending.repairs)
            .into_iter()
            .map(|record| (record.claim_id.clone(), record))
            .collect();
        self.carried.clear();
        let mut claim_ids = Vec::with_capacity(extracted.len());
        let mut findings = Vec::new();
        let mut examples = HashMap::new();
        for (index, (statement, prior)) in extracted.into_iter().zip(continues).enumerate() {
            let Extracted {
                claim,
                class,
                example,
            } = statement;
            let edited = prior.as_ref().is_some_and(|id| self.edited_now.remove(id));
            let id = match prior {
                Some(id) if edited => {
                    let successor = ClaimId::generate();
                    self.append(
                        &id,
                        ClaimEvent::Superseded {
                            successor: Some(successor.clone()),
                            by: Actor::User,
                        },
                    )
                    .await?;
                    self.start(&successor, claim, class, None).await?;
                    successor
                }
                Some(id) => {
                    self.carry_on(&id, claim, class, records.get(&id)).await?;
                    id
                }
                None => {
                    let id = ClaimId::generate();
                    let born = children.get(&index).cloned();
                    self.start(&id, claim, class, born).await?;
                    id
                }
            };
            match example {
                Some(observation_id) => {
                    examples.insert(observation_id, id.clone());
                }
                None => findings.push(id.clone()),
            }
            claim_ids.push(id);
        }
        for id in orphaned {
            if self.edited_now.contains(&id) {
                continue;
            }
            let after = rewritten.get(&id).cloned().flatten();
            self.orphan(&id, records.get(&id), after).await?;
        }
        // An edited claim no re-extracted claim continues: removed by the edit.
        for id in std::mem::take(&mut self.edited_now) {
            self.append(
                &id,
                ClaimEvent::Superseded {
                    successor: None,
                    by: Actor::User,
                },
            )
            .await?;
        }
        self.pending.previous = None;
        if self.sink.is_some() {
            self.save_pending().await?;
        }
        self.findings = findings;
        self.examples = examples;
        Ok(claim_ids)
    }

    /// A claim seen for the first time: extracted, then classified, and born
    /// of a repair when one wrote it beside its parent's successor (`born`:
    /// the parent and the attempts it left, claim sources design §9.1).
    async fn start(
        &mut self,
        id: &ClaimId,
        claim: ExtractedClaim,
        class: Option<Classed>,
        born: Option<(ClaimId, u8)>,
    ) -> Result<()> {
        self.append(
            id,
            ClaimEvent::Extracted {
                location: claim.location,
                statement: claim.statement,
            },
        )
        .await?;
        self.classify_claim(id, class).await?;
        if let Some((parent, attempts_left)) = born {
            self.append(
                id,
                ClaimEvent::BornOfRepair {
                    parent,
                    attempts_left,
                },
            )
            .await?;
        }
        Ok(())
    }

    /// Records the class a claim was checked as, when it takes the place of
    /// the one the ledger holds (claim classes design §3.1).
    async fn classify_claim(&mut self, id: &ClaimId, class: Option<Classed>) -> Result<()> {
        let Some(Classed { class, by }) = class else {
            return Ok(());
        };
        if !class.prevails(by, self.entry(id).and_then(ClaimLedgerEntry::classified)) {
            return Ok(());
        }
        self.append(id, ClaimEvent::Classified { class, by })
            .await?;
        Ok(())
    }

    /// A claim seen before: record where it stands now, its class, and the
    /// outcome of a repair that awaited this pass, and keep what
    /// `record_checks` weighs of it.
    async fn carry_on(
        &mut self,
        id: &ClaimId,
        claim: ExtractedClaim,
        class: Option<Classed>,
        repair: Option<&RepairRecord>,
    ) -> Result<()> {
        let Some(before) = self.entry(id).cloned() else {
            return self.start(id, claim, class, None).await;
        };
        let quote = claim.location.quote.clone();
        if claim.location != *before.location() || claim.statement != before.statement() {
            self.append(
                id,
                ClaimEvent::Extracted {
                    location: claim.location,
                    statement: claim.statement,
                },
            )
            .await?;
        }
        self.classify_claim(id, class).await?;
        let mut decided = false;
        if let Some(record) = repair {
            decided = self
                .append(
                    id,
                    ClaimEvent::RepairAttempted {
                        attempt: record.attempt,
                        escalated: record.escalated,
                        before: record.before.clone(),
                        after: Some(quote),
                        outcome: RepairOutcome::Rewritten,
                        goal: None,
                    },
                )
                .await?;
        }
        self.carried.insert(id.clone(), Carried { before, decided });
        Ok(())
    }

    /// A claim no statement continues, with the outcome of the repair that
    /// awaited this pass (`rewritten`: the text it wrote).
    async fn orphan(
        &mut self,
        id: &ClaimId,
        repair: Option<&RepairRecord>,
        rewritten: Option<String>,
    ) -> Result<()> {
        let gone = ClaimEvent::Superseded {
            successor: None,
            by: Actor::Machine,
        };
        let Some(record) = repair else {
            self.append(id, gone).await?;
            return Ok(());
        };
        // The repair rewrote the claim's part of its field and nothing it
        // wrote continues the claim: rewritten, its sentence gone with
        // the rewrite; it took text out only: removed (§4: undo restores
        // it).
        match rewritten {
            Some(after) => {
                let event = ClaimEvent::RepairAttempted {
                    attempt: record.attempt,
                    escalated: record.escalated,
                    before: record.before.clone(),
                    after: Some(after),
                    outcome: RepairOutcome::Rewritten,
                    goal: None,
                };
                self.append(id, event).await?;
                self.append(id, gone).await?;
            }
            None => {
                let event = ClaimEvent::RepairAttempted {
                    attempt: record.attempt,
                    escalated: record.escalated,
                    before: record.before.clone(),
                    after: None,
                    outcome: RepairOutcome::Removed,
                    goal: None,
                };
                // A claim that passed on the sentence has no repair to
                // record: it went with the sentence.
                if !self.append(id, event).await? {
                    self.append(id, gone).await?;
                }
            }
        }
        Ok(())
    }
}
