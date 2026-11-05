//! Identifying a pass: which claim each statement it extracted is, and what is
//! new about it that needs no check. `match_pass` computes it and records
//! nothing; `identify` records it (acceptance design §2.10).

use super::super::*;
use super::fields::{field_text, locate};
use super::{is_example, is_gone, successor, ClaimLedger, RepairRecord};
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal, Outcome, Step};
use lattice_core::domain::learning::claim_review::{
    identities, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ExtractedClaim, RepairOutcome,
};
use lattice_core::shared::text::sentences;

/// Why a rewrite that left the sentence as it was is recorded as not used.
const UNCHANGED: &str = "The repair left the sentence as it was.";
/// Why a narrowing whose new sentence still states its claim is recorded as
/// not used.
const STILL_STATED: &str = "The narrowed sentence still states it.";

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
    /// A repaired claim with no successor → the text its repair wrote in its
    /// field (`None` when the repair only took text out).
    rewritten: HashMap<ClaimId, Option<String>>,
}

impl Matched {
    /// The claim statement `index` continues; `None` when it is new.
    pub(in super::super) fn continues(&self, index: usize) -> Option<&ClaimId> {
        self.continues.get(index)?.as_ref()
    }

    /// The match of the statements `keep` holds, in their order, the others
    /// taken out of it. A statement beyond `keep` is kept.
    pub(in super::super) fn keep(self, keep: &[bool]) -> Self {
        let kept = |index: &usize| keep.get(*index).copied().unwrap_or(true);
        Self {
            continues: self
                .continues
                .into_iter()
                .enumerate()
                .filter(|(index, _)| kept(index))
                .map(|(_, claim)| claim)
                .collect(),
            orphaned: self.orphaned,
            rewritten: self.rewritten,
        }
    }
}

/// A claim `identify` carried on, as the reopenings and an example's check
/// weigh it: its entry as it stood before the pass, whether the pass decided
/// a repair that awaited it, and the quote the pass extracted it from.
pub(super) struct Carried {
    pub(super) before: ClaimLedgerEntry,
    pub(super) decided: bool,
    pub(super) quote: String,
}

impl ClaimLedger {
    /// The statements of a pass, as `match_pass` and `identify` take them: each
    /// of `claims`, a section's index and what it states, then each code
    /// example that failed or that the ledger tracks. A statement has no class
    /// yet.
    pub(in super::super) fn extracted(
        &self,
        candidate: &Value,
        claims: &[(usize, Claim)],
        executions: &[execution::Observation],
    ) -> Vec<Extracted> {
        let mut extracted: Vec<Extracted> = claims
            .iter()
            .map(|(unit, claim)| Extracted {
                claim: ExtractedClaim {
                    location: locate(candidate, *unit, &claim.quote, Some(&claim.statement)),
                    statement: claim.statement.clone(),
                },
                class: None,
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
        for observation in executions {
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

    /// The class of each of a pass's first `statements` statements when the
    /// match gives one: that of the claim it continues, a repair's successor
    /// included, as the ledger holds it. None for a statement new to the
    /// ledger, which is sorted.
    pub(in super::super) fn classes_given(
        &self,
        matched: &Matched,
        statements: usize,
    ) -> Vec<Option<Classed>> {
        (0..statements)
            .map(|index| {
                let entry = matched.continues(index).and_then(|id| self.entry(id))?;
                entry.classified().map(|(class, by)| Classed { class, by })
            })
            .collect()
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
        // that was there before, and keeps the claim's place in its graph
        // (claim sources design §9). Nothing else a repair wrote is a claim
        // of it (acceptance design §2.6).
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
            rewritten: heirs.rewritten,
        }
    }

    /// Records what `matched` found: which claim each statement is, and what
    /// is new about it that needs no check. Returns the claims, in the order
    /// of `extracted`. The outcome of a repair that awaited this pass is
    /// recorded here, and what the reopenings and an example's check weigh of
    /// each claim carried on is kept for them. A claim no statement continues
    /// while its sentence stands, and no repair awaits it, is not ended: it is
    /// a fault of the pass (`unplaced`), which `fault` stops the lesson for.
    pub(in super::super) async fn identify(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        extracted: Vec<Extracted>,
        matched: Matched,
    ) -> Result<Vec<ClaimId>> {
        let Matched {
            continues,
            orphaned,
            rewritten,
        } = matched;
        let records: HashMap<ClaimId, RepairRecord> = std::mem::take(&mut self.pending.repairs)
            .into_iter()
            .map(|record| (record.claim_id.clone(), record))
            .collect();
        self.carried.clear();
        self.unplaced.clear();
        let mut claim_ids = Vec::with_capacity(extracted.len());
        let mut findings = Vec::new();
        let mut examples = HashMap::new();
        for (statement, prior) in extracted.into_iter().zip(continues) {
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
                    self.start(&successor, claim, class).await?;
                    successor
                }
                Some(id)
                    if self.takes_successor(candidate, &id, records.get(&id))
                        || self.rewritten_unawaited(
                            candidate,
                            &id,
                            records.get(&id),
                            &claim.statement,
                        ) =>
                {
                    // A statement on a sentence another claim's rewrite
                    // changed, or on one that says something else with no
                    // repair awaiting it, is read again as a successor, at the
                    // post-rewrite node of the class it holds.
                    let successor = ClaimId::generate();
                    let held = self
                        .entry(&id)
                        .and_then(ClaimLedgerEntry::classified)
                        .map(|(class, by)| Classed { class, by });
                    self.append(
                        &id,
                        ClaimEvent::Superseded {
                            successor: Some(successor.clone()),
                            by: Actor::Machine,
                        },
                    )
                    .await?;
                    self.start(&successor, claim, held).await?;
                    successor
                }
                Some(id) => {
                    self.carry_on(policy, candidate, &id, claim, class, records.get(&id))
                        .await?;
                    id
                }
                None => {
                    let id = ClaimId::generate();
                    self.start(&id, claim, class).await?;
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
            if !records.contains_key(&id) && self.stands(candidate, &id) {
                self.unplaced.push(id);
                continue;
            }
            let after = rewritten.get(&id).cloned().flatten();
            self.orphan(policy, candidate, &id, records.get(&id), after)
                .await?;
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

    /// Whether the statement `id` still has its sentence in the lesson, in a
    /// section that stays: a code example's claim has none, and a section that
    /// goes takes its claims with it.
    fn stands(&self, candidate: &Value, id: &ClaimId) -> bool {
        let Some(entry) = self.entry(id).filter(|entry| !is_example(entry)) else {
            return false;
        };
        let at = entry.location();
        !self.cuts.going.contains_key(&at.unit_index)
            && !at.quote.trim().is_empty()
            && field_text(candidate, at.unit_index, &at.field)
                .is_some_and(|text| text.contains(&at.quote))
    }

    /// Whether the claim `id`, which a repair of another claim sent for as a
    /// neighbour on its sentence, now stands on a changed sentence: a statement
    /// of the machine's own, or one it has ended, not the user's, which stays
    /// theirs, nor a code example.
    fn takes_successor(
        &self,
        candidate: &Value,
        id: &ClaimId,
        repair: Option<&RepairRecord>,
    ) -> bool {
        let Some(record) = repair.filter(|record| record.rides_on.is_some()) else {
            return false;
        };
        let Some(entry) = self.entry(id) else {
            return false;
        };
        if entry.state().is_settled() || entry.state().awaits_user() || entry.classified().is_none()
        {
            return false;
        }
        let location = entry.location();
        let standing = field_text(candidate, location.unit_index, &location.field)
            .is_some_and(|text| text.contains(&record.before));
        !standing
    }

    /// Whether the claim `id`, which no repair awaited, now stands where its
    /// sentence was written over with another statement, `statement`: a claim
    /// of the machine's own, or one it has ended, not the user's, nor a code
    /// example.
    fn rewritten_unawaited(
        &self,
        candidate: &Value,
        id: &ClaimId,
        repair: Option<&RepairRecord>,
        statement: &str,
    ) -> bool {
        if repair.is_some() {
            return false;
        }
        let Some(entry) = self.entry(id) else {
            return false;
        };
        if entry.state().is_settled() || entry.state().awaits_user() || entry.classified().is_none()
        {
            return false;
        }
        let location = entry.location();
        let standing = field_text(candidate, location.unit_index, &location.field)
            .is_some_and(|text| text.contains(&location.quote));
        !standing && entry.statement() != statement
    }

    /// A claim seen for the first time: extracted, then classified. None is
    /// born of a repair (acceptance design §2.6).
    async fn start(
        &mut self,
        id: &ClaimId,
        claim: ExtractedClaim,
        class: Option<Classed>,
    ) -> Result<()> {
        self.append(
            id,
            ClaimEvent::Extracted {
                location: claim.location,
                statement: claim.statement,
            },
        )
        .await?;
        self.classify_claim(id, class).await
    }

    /// Records the class a claim was sorted into. A claim the ledger holds a
    /// class for is not sorted again: its class changes only when the evidence
    /// moves it (acceptance design §2.9).
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

    /// A claim seen before: record where it stands now, its class when it has
    /// none, and the outcome of a repair that awaited this pass, and keep what
    /// the reopenings and an example's check weigh of it.
    async fn carry_on(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        id: &ClaimId,
        claim: ExtractedClaim,
        class: Option<Classed>,
        repair: Option<&RepairRecord>,
    ) -> Result<()> {
        let Some(before) = self.entry(id).cloned() else {
            return self.start(id, claim, class).await;
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
        if before.classified().is_none() {
            self.classify_claim(id, class).await?;
        }
        // A statement that rode on another's repair, and whose sentence stands,
        // has no outcome of its own to record.
        let decided = match repair {
            Some(record) if record.rides_on.is_none() || before.classified().is_none() => {
                self.record_repair(policy, candidate, id, record, &quote)
                    .await?
            }
            _ => false,
        };
        self.carried.insert(
            id.clone(),
            Carried {
                before,
                decided,
                quote,
            },
        );
        Ok(())
    }

    /// Whether the sentence a repair was sent for is no longer in its field.
    fn left(&self, candidate: &Value, id: &ClaimId, record: &RepairRecord) -> bool {
        self.entry(id).is_none_or(|entry| {
            let location = entry.location();
            field_text(candidate, location.unit_index, &location.field)
                .is_none_or(|text| !text.contains(&record.before))
        })
    }

    /// The outcome of the repair that awaited this pass, for a claim that
    /// continues: what the repair did to its sentence, and for a rewrite the
    /// policy's step. Whether the ledger took the repair's event.
    async fn record_repair(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        id: &ClaimId,
        record: &RepairRecord,
        quote: &str,
    ) -> Result<bool> {
        if record.narrowing {
            return self.record_narrowing(candidate, id, record).await;
        }
        let rewrite = match record.step {
            Some(Step::Rewrite(goal)) => Some(goal),
            Some(_) => return Ok(false),
            None => None,
        };
        let changed = self.left(candidate, id, record);
        let applied = rewrite.is_none() || changed;
        let (after, outcome) = if applied {
            (Some(quote.to_owned()), RepairOutcome::Rewritten)
        } else {
            (
                None,
                RepairOutcome::Rejected {
                    reason: UNCHANGED.into(),
                },
            )
        };
        let taken = self
            .append(
                id,
                ClaimEvent::RepairAttempted {
                    attempt: record.attempt,
                    escalated: record.escalated,
                    before: record.before.clone(),
                    after,
                    outcome,
                    goal: rewrite,
                },
            )
            .await?;
        if let (true, Some(goal)) = (taken, rewrite) {
            let outcome = if applied {
                Outcome::Rewritten
            } else {
                Outcome::NotApplied
            };
            self.record_step(policy, id, Step::Rewrite(goal), outcome)
                .await?;
        }
        Ok(taken)
    }

    /// The outcome of a narrowing that awaited this pass, for a claim a
    /// statement still continues: not applied. The sentence stands as it was,
    /// or it changed and still states the claim. Either way the claim takes no
    /// step: it stays at its cut with its narrowing spent, and the next round
    /// cuts the sentence whole. Whether the ledger took the event.
    async fn record_narrowing(
        &mut self,
        candidate: &Value,
        id: &ClaimId,
        record: &RepairRecord,
    ) -> Result<bool> {
        let reason = if self.left(candidate, id, record) {
            STILL_STATED
        } else {
            UNCHANGED
        };
        self.append(
            id,
            ClaimEvent::RepairAttempted {
                attempt: record.attempt,
                escalated: record.escalated,
                before: record.before.clone(),
                after: None,
                outcome: RepairOutcome::Rejected {
                    reason: reason.into(),
                },
                goal: Some(Goal::KeepTheOthers),
            },
        )
        .await
    }

    /// A claim no statement continues, with the outcome of the repair that
    /// awaited this pass (`rewritten`: the text it wrote).
    async fn orphan(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
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
        if record.narrowing {
            return self
                .orphan_narrowed(policy, candidate, id, record, rewritten)
                .await;
        }
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
                    goal: record.goal(),
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
                    goal: record.goal(),
                };
                // A claim that passed on the sentence has no repair to
                // record, and a decided claim's record moves nothing: either
                // way it went with the sentence.
                self.append(id, event).await?;
                if self.entry(id).is_some_and(|entry| !is_gone(entry.state())) {
                    self.append(id, gone).await?;
                }
            }
        }
        Ok(())
    }

    /// A claim no statement continues, whose narrowing awaited this pass
    /// (`rewritten`: the text it wrote). The sentence changed: the narrowing
    /// took the claim out of it, removed with the text written and the
    /// policy's cut, so the claim ends by its graph. The sentence stands as it
    /// was: not applied. A claim in a section that goes goes with it.
    async fn orphan_narrowed(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        id: &ClaimId,
        record: &RepairRecord,
        rewritten: Option<String>,
    ) -> Result<()> {
        let gone = ClaimEvent::Superseded {
            successor: None,
            by: Actor::Machine,
        };
        if self.cuts.going.contains_key(&record.unit) {
            self.append(id, gone).await?;
            return Ok(());
        }
        let changed = self.left(candidate, id, record);
        let outcome = if changed {
            RepairOutcome::Removed
        } else {
            RepairOutcome::Rejected {
                reason: UNCHANGED.into(),
            }
        };
        let event = ClaimEvent::RepairAttempted {
            attempt: record.attempt,
            escalated: record.escalated,
            before: record.before.clone(),
            after: rewritten.filter(|_| changed),
            outcome,
            goal: Some(Goal::KeepTheOthers),
        };
        let taken = self.append(id, event).await?;
        if !changed {
            return Ok(());
        }
        let stepped = taken
            && self
                .record_step(policy, id, Step::Cut, Outcome::Cut)
                .await?;
        if !stepped && self.entry(id).is_some_and(|entry| !is_gone(entry.state())) {
            self.append(id, gone).await?;
        }
        Ok(())
    }
}
