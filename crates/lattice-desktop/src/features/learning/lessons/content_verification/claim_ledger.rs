//! The claim ledger as the verify-and-repair loop keeps it: which claim each
//! finding is, what the machine did about it, and what happens next. The
//! acceptance policy decides every statement's next step; the ledger records
//! each step with the decision made of it, and answers the loop's questions
//! from the node each statement stands at (§3, §4, §6.1). Design:
//! docs/design/2026-10-16-lesson-claim-review.md,
//! docs/design/2026-11-03-lesson-acceptance-policy.md.

mod audit;
mod checks;
mod cotenancy;
mod cuts;
mod fields;
mod held;
mod history;
mod identify;
mod located;
mod observe;
mod position;
mod readings;
mod receipts;
mod stops;
mod successor;
mod unverified;
mod written;

pub(super) use audit::Audited;
pub(super) use cotenancy::{sentences, CutOf};
pub(super) use position::StepKind;
pub(super) use stops::UNREADABLE;
pub(super) use successor::Rewrites;

use super::provenance_gate::ProvenanceGate;
use super::resolution::Resolution;
use super::sections::Cuts;
use super::support_gate::SupportGate;
use super::*;
use crate::features::learning::curriculum::LearningClaimTally;
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::lessons::claim_reviews::repository::{
    ClaimReviewRepository, ShownPassage,
};
use crate::features::learning::persistence::now;
use identify::Carried;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal, Outcome, Step, Then};
use lattice_core::domain::learning::claim_review::{
    end_state, Actor, ClaimCounts, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation,
    ClaimState, EventProvenance, LessonReview, LessonReviewState, RepairOutcome, Revision,
    StopReason, UserAction, MACHINE_ATTEMPTS,
};

pub(in crate::features::learning) use fields::{field_pointer, field_text, locate, unit_texts};
pub(in crate::features::learning) use held::{
    held_for_review, held_lessons, needs_review, review_outcome, HeldForReview,
};
pub(in crate::features::learning) use located::{
    claim_quote, clip, located, located_quote, narrow,
};

/// The lesson checkpoint holding repairs awaiting the pass that decides them.
const PENDING_KEY: &str = "claim-ledger-v1:pending";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where a statement stands in the lesson: its section, field and quote.
#[derive(Clone, Copy)]
pub(super) struct Place<'a> {
    pub(super) unit: usize,
    pub(super) field: &'a str,
    pub(super) quote: &'a str,
}

impl<'a> Place<'a> {
    fn of(location: &'a ClaimLocation) -> Self {
        Self {
            unit: location.unit_index,
            field: &location.field,
            quote: &location.quote,
        }
    }
}

/// Whether two statements stand on one sentence: in one field of one
/// section, one quote holding the other. A rewrite of the sentence changes
/// them both (claim classes design §3.2).
pub(super) fn stand_together(a: &Place<'_>, b: &Place<'_>) -> bool {
    a.unit == b.unit
        && a.field == b.field
        && !a.quote.trim().is_empty()
        && !b.quote.trim().is_empty()
        && (a.quote.contains(b.quote) || b.quote.contains(a.quote))
}

/// A repair sent for a claim, awaiting the pass that shows what became of
/// its sentence: rewritten or not (the policy decides what follows) or
/// removed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct RepairRecord {
    pub(super) claim_id: ClaimId,
    pub(super) unit: usize,
    pub(super) attempt: u8,
    pub(super) escalated: bool,
    pub(super) before: String,
    /// Set on a claim that stands on a sentence this other claim's repair
    /// rewrites: it takes a successor when the sentence is rewritten, and its
    /// record is dropped with the repair's when the sentence is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) rides_on: Option<ClaimId>,
    /// The step the claim is taking: a rewrite toward its node's goal, or the
    /// cut. None for a code example, which keeps its own rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) step: Option<Step>,
    /// The cut is a narrowing rewrite by the model: the sentence also states
    /// another live statement, or no single sentence states this one
    /// (`ClaimLedger::cut_of`). The statements standing on its sentence ride
    /// on it, as on a rewrite.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) narrowing: bool,
}

impl RepairRecord {
    /// What the repair was asked to achieve.
    pub(super) fn goal(&self) -> Option<Goal> {
        if self.narrowing {
            return Some(Goal::KeepTheOthers);
        }
        match self.step {
            Some(Step::Rewrite(goal)) => Some(goal),
            _ => None,
        }
    }
}

/// What the ledger keeps between a change to the lesson and the pass that
/// observes it, saved so a restart still charges the repair.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Pending {
    /// The lesson as the ledger's claims were last extracted from it, when a
    /// repair or the user's edits changed it since: identity compares a
    /// claim's old place with its new one (§3 `identity`).
    pub(super) previous: Option<Value>,
    pub(super) repairs: Vec<RepairRecord>,
}

/// Where the ledger is written inside a preparation job.
struct Sink {
    store: ClaimReviewRepository,
    lesson_id: String,
    job_id: String,
    program_id: String,
}

/// A statement the machine owns, at the node it waits at.
pub(super) struct Waiting {
    pub(super) claim: ClaimId,
    pub(super) node: &'static str,
    pub(super) step: Step,
}

/// What the machine does next, once a pass is read.
pub(super) enum Decision {
    /// Statements wait at nodes the next pass takes: a reading, or a remedy
    /// the last pass did not make.
    Continue(Vec<Waiting>),
    /// The lesson stops, for this reason (boxed: a stop is far larger than the
    /// other variants).
    Stop(Box<StopReason>),
    /// Repair these claims.
    Repair(Vec<ClaimId>),
    /// Every claim is resolved.
    Publish,
    /// Nothing is left for the machine; this many claims wait for the user.
    NeedsReview(usize),
}

pub(super) struct ClaimLedger {
    sink: Option<Sink>,
    entries: Vec<ClaimLedgerEntry>,
    revision: Revision,
    checker_model: String,
    window_tokens: usize,
    pub(super) pending: Pending,
    /// The claim of each finding of the latest pass, in order.
    findings: Vec<ClaimId>,
    /// The claim of each tracked example, by observation id.
    examples: HashMap<String, ClaimId>,
    /// What `identify` left for the reopenings and an example's check of each
    /// claim it carried on, until they are made.
    carried: HashMap<ClaimId, Carried>,
    /// The claims the last `identify` found no statement for while their
    /// sentences stand, which stop the lesson (`fault`).
    unplaced: Vec<ClaimId>,
    /// Claims whose user edit was just applied: re-extraction names their
    /// successors (§5.1 Edit).
    pub(super) edited_now: HashSet<ClaimId>,
    /// What the lesson's cuts left: the sections that go and the flow repairs
    /// made (claim classes design §3.10).
    pub(super) cuts: Cuts,
    gate: Box<dyn SupportGate>,
}

/// A code example's claim: the ledger holds no class for it, which every
/// statement has.
fn is_example(entry: &ClaimLedgerEntry) -> bool {
    entry.classified().is_none()
}

/// A claim the machine may not act on any more: its sentence is gone.
fn is_gone(state: &ClaimState) -> bool {
    matches!(
        state,
        ClaimState::Removed { .. } | ClaimState::Edited { .. }
    )
}

/// The issue line a pass reports for a claim no passage supports.
fn issue(finding: &Finding) -> String {
    format!(
        "Section {}: {} — {}",
        finding.unit + 1,
        finding.statement,
        finding.reason
    )
}

impl ClaimLedger {
    /// The lesson's ledger: from the database inside a preparation job, whose
    /// review row this marks as verifying `raw`; otherwise empty, in memory.
    pub(super) async fn open(llm: &dyn LLMPort, raw: &str) -> Result<Self> {
        let mut ledger = Self {
            sink: None,
            entries: Vec::new(),
            revision: Revision(0),
            checker_model: llm.model_name().to_owned(),
            window_tokens: llm.max_context_tokens(),
            pending: Pending::default(),
            findings: Vec::new(),
            examples: HashMap::new(),
            carried: HashMap::new(),
            unplaced: Vec::new(),
            edited_now: HashSet::new(),
            cuts: Cuts::default(),
            gate: Box::new(ProvenanceGate),
        };
        let Some(scope) = crate::features::learning::lesson_drafts::scope() else {
            return Ok(ledger);
        };
        let program_id = LearningCurriculumRepository::new(scope.pool.clone())
            .job(&scope.job_id)
            .await?
            .program_id;
        ledger.sink = Some(Sink {
            store: ClaimReviewRepository::new(scope.pool),
            lesson_id: scope.lesson_id,
            job_id: scope.job_id,
            program_id,
        });
        ledger
            .write_review(LessonReviewState::Verifying, raw, None)
            .await?;
        ledger.sync().await?;
        ledger.pending = crate::features::learning::lesson_drafts::checkpoint(PENDING_KEY)
            .await?
            .and_then(|saved| serde_json::from_value(saved).ok())
            .unwrap_or_default();
        ledger.load_cuts().await?;
        Ok(ledger)
    }

    /// A sync point (§9): reload what the user did since the last one.
    pub(super) async fn sync(&mut self) -> Result<()> {
        let Some(sink) = &self.sink else {
            return Ok(());
        };
        let ledger = sink.store.load(&sink.lesson_id).await?;
        self.entries = ledger.claims;
        self.revision = ledger.revision;
        Ok(())
    }

    pub(super) fn entries(&self) -> &[ClaimLedgerEntry] {
        &self.entries
    }

    pub(super) fn entry(&self, claim_id: &ClaimId) -> Option<&ClaimLedgerEntry> {
        self.entries
            .iter()
            .find(|entry| entry.claim_id() == claim_id)
    }

    /// The claims of the latest observed findings, in report order.
    pub(super) fn finding_claims(&self) -> &[ClaimId] {
        &self.findings
    }

    async fn write_review(
        &mut self,
        state: LessonReviewState,
        raw: &str,
        stop_reason: Option<StopReason>,
    ) -> Result<()> {
        let Some(sink) = &self.sink else {
            return Ok(());
        };
        self.revision = sink
            .store
            .set_lesson_review(
                &sink.lesson_id,
                LessonReview {
                    program_id: sink.program_id.clone(),
                    job_id: sink.job_id.clone(),
                    state,
                    draft_sha256: digest(raw),
                    checker_model: self.checker_model.clone(),
                    stop_reason,
                },
            )
            .await?;
        Ok(())
    }

    pub(super) async fn save_pending(&self) -> Result<()> {
        crate::features::learning::lesson_drafts::record_checkpoint(
            PENDING_KEY,
            serde_json::to_value(&self.pending)?,
        )
        .await
    }

    fn provenance(&self) -> EventProvenance {
        EventProvenance {
            actor: Actor::Machine,
            at: now(),
            job_id: self.sink.as_ref().map(|sink| sink.job_id.clone()),
            app_version: APP_VERSION.into(),
            checker_model: Some(self.checker_model.clone()),
        }
    }

    /// The claim's log with `event` appended, or `None` when its state
    /// refuses it (the user acted first: their action wins, §9).
    fn probe(
        &self,
        claim_id: &ClaimId,
        provenance: &EventProvenance,
        event: &ClaimEvent,
    ) -> Result<Option<ClaimLedgerEntry>> {
        let provenance = provenance.clone();
        let probed = match (self.entry(claim_id), event) {
            (Some(entry), _) => {
                let mut entry = entry.clone();
                entry.append(provenance, event.clone()).map(|_| entry)
            }
            (
                None,
                ClaimEvent::Extracted {
                    location,
                    statement,
                },
            ) => ClaimLedgerEntry::extracted(
                claim_id.clone(),
                provenance,
                location.clone(),
                statement.clone(),
            ),
            (None, _) => {
                return Err(AppError::InternalError(format!(
                    "The claim ledger has no claim {}",
                    claim_id.0
                )))
            }
        };
        Ok(match probed {
            Ok(entry) => Some(entry),
            Err(refused) => {
                tracing::info!(claim = %claim_id.0, %refused, "claim.state: the machine's event yields to the claim's state");
                None
            }
        })
    }

    /// Appends a machine event; `false` when the claim's state refused it.
    pub(super) async fn append(&mut self, claim_id: &ClaimId, event: ClaimEvent) -> Result<bool> {
        let provenance = self.provenance();
        let Some(mut entry) = self.probe(claim_id, &provenance, &event)? else {
            return Ok(false);
        };
        if let Some(sink) = &self.sink {
            let written = sink
                .store
                .append(
                    &sink.lesson_id,
                    claim_id,
                    provenance.clone(),
                    event.clone(),
                    self.revision,
                )
                .await;
            self.revision = match written {
                Ok(revision) => revision,
                // A user action landed since the last sync point: reload and
                // record against the fresh log, once.
                Err(AppError::InvalidState(_)) => {
                    self.sync().await?;
                    let Some(fresh) = self.probe(claim_id, &provenance, &event)? else {
                        return Ok(false);
                    };
                    entry = fresh;
                    let sink = self.sink.as_ref().ok_or_else(|| {
                        AppError::InternalError("The claim ledger lost its store".into())
                    })?;
                    sink.store
                        .append(&sink.lesson_id, claim_id, provenance, event, self.revision)
                        .await?
                }
                Err(error) => return Err(error),
            };
        }
        match self
            .entries
            .iter_mut()
            .find(|existing| existing.claim_id() == claim_id)
        {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
        Ok(true)
    }

    async fn show(&self, claim_id: &ClaimId, passages: &[ShownPassage]) -> Result<()> {
        match &self.sink {
            Some(sink) => {
                sink.store
                    .show_passages(&sink.lesson_id, claim_id, passages)
                    .await
            }
            None => Ok(()),
        }
    }

    /// The passages each claim's card shows: what its latest check compared.
    pub(super) async fn shown_passages(&self) -> Result<HashMap<ClaimId, Vec<ShownPassage>>> {
        match &self.sink {
            Some(sink) => sink.store.shown_passages(&sink.lesson_id).await,
            None => Ok(HashMap::new()),
        }
    }

    /// The claims of the latest pass, findings and examples.
    fn observed(&self) -> impl Iterator<Item = &ClaimId> {
        self.findings.iter().chain(self.examples.values())
    }

    /// Why a claim the machine owns waits at the node it does: the step it
    /// takes next.
    fn waiting(&self, policy: &dyn AcceptancePolicy) -> Vec<Waiting> {
        self.at_nodes(policy)
            .into_iter()
            .filter(|(_, _, step)| !matches!(step, Step::Rewrite(_) | Step::Cut))
            .filter_map(|(claim, at, step)| {
                let node = policy.graph(at.class).node(at.node)?.name;
                Some(Waiting { claim, node, step })
            })
            .collect()
    }

    /// What the lesson does next, checked in this order (acceptance design
    /// §2.10): a statement this run stopped on stops the lesson, as does one
    /// whose record does not fit its graph or was decided under another
    /// policy; a statement at a rewrite or a cut is repaired, along with a
    /// failed example by its own rule; a statement at any other node is taken
    /// by the next pass; and once every statement has ended the lesson
    /// publishes, or waits for the user: only claims in their hands are left
    /// then (a dispute, a reopening, a withdrawn confirmation, or what the
    /// user's own change of a source's trust re-decided). A last attempt that
    /// cannot take its sentence out ends its claim removed with its section
    /// (claim classes design §3.2, §3.10).
    pub(super) fn decide(&self, policy: &dyn AcceptancePolicy, stopped: &[ClaimId]) -> Decision {
        if !stopped.is_empty() {
            return Decision::Stop(Box::new(self.unreadable(stopped.len())));
        }
        if let Some(reason) = self.fault(policy) {
            return Decision::Stop(Box::new(reason));
        }
        let repairable: Vec<ClaimId> = self
            .observed()
            .filter(|id| self.may_repair(policy, id))
            .cloned()
            .collect();
        if !repairable.is_empty() {
            return Decision::Repair(repairable);
        }
        let waiting = self.waiting(policy);
        if !waiting.is_empty() {
            return Decision::Continue(waiting);
        }
        match end_state(self.entries.iter()) {
            LessonReviewState::Ready => Decision::Publish,
            state => {
                if state == LessonReviewState::Verifying {
                    tracing::warn!("claim.state: claims are the machine's but nothing is left for it to do; handing the lesson to the user");
                }
                let counts = ClaimCounts::tally(self.entries.iter());
                Decision::NeedsReview(counts.queued + counts.open)
            }
        }
    }

    /// The decisions the ledger holds in all: a pass that records none has
    /// moved no statement.
    pub(super) fn stepped(&self) -> usize {
        self.entries
            .iter()
            .flat_map(ClaimLedgerEntry::events)
            .filter(|record| matches!(record.event, ClaimEvent::Stepped { .. }))
            .count()
    }

    /// What the next pass keeps of the ledger: whether it is a later pass, the
    /// lesson as it was before the repairs or edits, and the place in its
    /// graph of each statement the ledger holds (acceptance design §2.9).
    pub(super) fn rewrites(&self, policy: &dyn AcceptancePolicy) -> Rewrites {
        Rewrites::new(
            !self.entries.is_empty(),
            self.pending.previous.clone(),
            self.walks(policy),
        )
        .holding(self.held())
    }

    /// The claims of the statements neither the user nor the machine has
    /// ended. A code example's claim is none of them: its run is no sentence.
    fn held(&self) -> Vec<ClaimLedgerEntry> {
        self.entries
            .iter()
            .filter(|entry| !is_gone(entry.state()) && !is_example(entry))
            .cloned()
            .collect()
    }

    /// The repair sent for `claims` (§4): the step each is taking, saved with
    /// the lesson as it stood so the pass after it records the outcome even
    /// after a restart. A statement takes the step of the node it stands at:
    /// a rewrite toward its goal (the first, or the retry told why the first
    /// was not applied) or the cut, which is a narrowing rewrite when its
    /// sentence also states another live statement (`cut_of`); a failed
    /// example, its own attempt.
    pub(super) async fn begin_repairs(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        claims: &[ClaimId],
    ) -> Result<()> {
        let mut records: Vec<RepairRecord> = claims
            .iter()
            .filter_map(|id| self.repair_record(policy, candidate, id))
            .collect();
        // The claims standing on a sentence a rewrite or a narrowing changes
        // are read again as successors with it (claim classes design §3.2); a
        // last attempt cuts the sentence and takes them with it.
        let rewritten: Vec<(ClaimId, u8)> = records
            .iter()
            .filter(|record| record.narrowing || !self.cuts_next(policy, &record.claim_id))
            .map(|record| (record.claim_id.clone(), record.attempt))
            .collect();
        for (target, attempt) in rewritten {
            self.ride(&target, attempt, claims, &mut records);
        }
        self.pending.repairs.extend(records);
        self.pending
            .previous
            .get_or_insert_with(|| candidate.clone());
        self.save_pending().await
    }

    /// Pushes to `records` the record of each claim standing on `target`'s
    /// sentence, which rides on its repair: not one sent for repair itself
    /// (`sent`), nor one `records` holds already.
    fn ride(
        &self,
        target: &ClaimId,
        attempt: u8,
        sent: &[ClaimId],
        records: &mut Vec<RepairRecord>,
    ) {
        for mate in self.cotenants(target) {
            if sent.contains(&mate) || records.iter().any(|record| record.claim_id == mate) {
                continue;
            }
            let Some(entry) = self.entry(&mate) else {
                continue;
            };
            records.push(RepairRecord {
                unit: entry.location().unit_index,
                attempt,
                escalated: false,
                before: entry.location().quote.clone(),
                rides_on: Some(target.clone()),
                step: None,
                narrowing: false,
                claim_id: mate,
            });
        }
    }

    /// These statements' cuts are narrowing rewrites (`cut_of`), whatever
    /// `begin_repairs` made of them: their pending records are marked so, and
    /// the claims standing on their sentences ride on them.
    pub(super) async fn mark_narrowing(&mut self, claims: &[ClaimId]) -> Result<()> {
        let mut records = std::mem::take(&mut self.pending.repairs);
        let marked: Vec<(ClaimId, u8)> = records
            .iter_mut()
            .filter(|record| {
                record.rides_on.is_none() && !record.narrowing && claims.contains(&record.claim_id)
            })
            .map(|record| {
                record.narrowing = true;
                (record.claim_id.clone(), record.attempt)
            })
            .collect();
        for (target, attempt) in marked {
            self.ride(&target, attempt, claims, &mut records);
        }
        self.pending.repairs = records;
        self.save_pending().await
    }

    /// The repair of `id` the loop sends next: a statement's is the step of
    /// the node it stands at, a failed example's its own attempt.
    fn repair_record(
        &self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        id: &ClaimId,
    ) -> Option<RepairRecord> {
        let entry = self.entry(id)?;
        let unit = entry.location().unit_index;
        let before = entry.location().quote.clone();
        if is_example(entry) {
            let ClaimState::Open {
                machine_attempts_left,
                ..
            } = entry.state()
            else {
                return None;
            };
            return Some(RepairRecord {
                claim_id: id.clone(),
                unit,
                attempt: (MACHINE_ATTEMPTS + 1).saturating_sub(*machine_attempts_left),
                escalated: entry.next_repair_escalates(),
                before,
                rides_on: None,
                step: None,
                narrowing: false,
            });
        }
        let (at, step) = self.waits_at(policy, id)?;
        let node = policy.graph(at.class).node(at.node)?;
        let (attempt, escalated) = match step {
            Step::Rewrite(_) => (if node.name.ends_with(".again") { 2 } else { 1 }, false),
            Step::Cut => (2, true),
            _ => return None,
        };
        Some(RepairRecord {
            claim_id: id.clone(),
            unit,
            attempt,
            escalated,
            before,
            rides_on: None,
            step: Some(step),
            narrowing: step == Step::Cut && self.narrows(candidate, id),
        })
    }

    /// Whether the next repair of `claim_id` is its cut: a statement at a
    /// `Cut` node, a failed example on its last attempt. What the cut of a
    /// statement takes out is `cut_of`'s.
    pub(super) fn cuts_next(&self, policy: &dyn AcceptancePolicy, claim_id: &ClaimId) -> bool {
        match self.entry(claim_id) {
            Some(entry) if is_example(entry) => entry.next_repair_escalates(),
            Some(_) => matches!(self.waits_at(policy, claim_id), Some((_, Step::Cut))),
            None => false,
        }
    }

    /// The repair of these claims produced nothing usable. A failed example
    /// uses its attempt at once; a statement's rewrite was not applied, and the
    /// policy sends it to its retry or its cut. A narrowing is closed as not
    /// applied with its goal and takes no step: the statement stays at its cut
    /// with its narrowing spent.
    pub(super) async fn reject(
        &mut self,
        policy: &dyn AcceptancePolicy,
        claims: &[ClaimId],
        reason: &str,
    ) -> Result<()> {
        let rewrites: Vec<(ClaimId, Step)> = self
            .pending
            .repairs
            .iter()
            .filter(|record| claims.contains(&record.claim_id))
            .filter_map(|record| {
                let step = record
                    .step
                    .filter(|step| matches!(step, Step::Rewrite(_)))?;
                Some((record.claim_id.clone(), step))
            })
            .collect();
        self.close_records(
            claims,
            RepairOutcome::Rejected {
                reason: reason.into(),
            },
        )
        .await?;
        for (id, step) in rewrites {
            self.record_step(policy, &id, step, Outcome::NotApplied)
                .await?;
        }
        Ok(())
    }

    /// The user resolved these claims while their repair waited: it is not
    /// sent and not charged (§9).
    pub(super) async fn supersede(&mut self, claims: &[ClaimId]) -> Result<()> {
        self.close_records(claims, RepairOutcome::SupersededByUser)
            .await
    }

    /// The last attempt on these statements took their sentence out
    /// (`Outcome::Cut`, also when it was out already) or, refused, took their
    /// section out (`Outcome::CannotCut`): the repair is closed as the removal
    /// it was, and the policy's step ends the claims removed.
    pub(super) async fn cut(
        &mut self,
        policy: &dyn AcceptancePolicy,
        claims: &[ClaimId],
        outcome: Outcome,
    ) -> Result<()> {
        self.close_records(claims, RepairOutcome::Removed).await?;
        for id in claims {
            self.record_step(policy, id, Step::Cut, outcome).await?;
        }
        Ok(())
    }

    async fn close_records(&mut self, claims: &[ClaimId], outcome: RepairOutcome) -> Result<()> {
        let (closed, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending.repairs)
            .into_iter()
            .partition(|record| claims.contains(&record.claim_id));
        // The claims that rode on these repairs were not rewritten.
        self.pending.repairs = kept
            .into_iter()
            .filter(|record| {
                !record
                    .rides_on
                    .as_ref()
                    .is_some_and(|target| claims.contains(target))
            })
            .collect();
        for record in closed {
            let goal = record.goal();
            self.append(
                &record.claim_id,
                ClaimEvent::RepairAttempted {
                    attempt: record.attempt,
                    escalated: record.escalated,
                    before: record.before,
                    after: None,
                    outcome: outcome.clone(),
                    goal,
                },
            )
            .await?;
        }
        self.save_pending().await
    }

    /// The claim tracking a failed example, by observation id.
    pub(super) fn example_claim(&self, observation_id: &str) -> Option<&ClaimId> {
        self.examples.get(observation_id)
    }

    /// Whether `claim_id` is a statement of the lesson, which the acceptance
    /// policy decides, and not a failed code example, which keeps its own rule.
    pub(super) fn is_statement(&self, claim_id: &ClaimId) -> bool {
        self.entry(claim_id).is_some_and(|entry| !is_example(entry))
    }

    /// Whether the machine may still repair this claim (a sync point's test):
    /// a statement at a `Rewrite` or a `Cut` node, a failed example by its own
    /// rule.
    pub(super) fn may_repair(&self, policy: &dyn AcceptancePolicy, claim_id: &ClaimId) -> bool {
        match self.entry(claim_id) {
            Some(entry) if is_example(entry) => entry.may_repair(),
            Some(_) => matches!(
                self.waits_at(policy, claim_id),
                Some((_, Step::Rewrite(_) | Step::Cut))
            ),
            None => false,
        }
    }

    /// The report the lesson publishes with: each finding carries the user's
    /// resolution, and the issues are what the ledger leaves open.
    pub(super) fn bind(&self, mut report: LessonVerificationReport) -> LessonVerificationReport {
        let confirmed = |id: &ClaimId| match self.entry(id).map(ClaimLedgerEntry::state) {
            Some(ClaimState::Confirmed { at, note, .. }) => Some(Resolution::Confirmed {
                at: *at,
                note: note.clone(),
            }),
            _ => None,
        };
        let resolution = |id: &ClaimId| match self.entry(id).map(ClaimLedgerEntry::state) {
            Some(ClaimState::Unverified { reason, .. }) => {
                Some(Resolution::Unverified { reason: *reason })
            }
            _ => confirmed(id),
        };
        for (finding, id) in report.findings.iter_mut().zip(&self.findings) {
            finding.resolution = resolution(id);
        }
        let confirmed: Vec<String> = report
            .executions
            .iter()
            .filter(|observation| {
                !observation.passed()
                    && self
                        .examples
                        .get(&observation.id)
                        .is_some_and(|id| confirmed(id).is_some())
            })
            .map(|observation| observation.id.clone())
            .collect();
        report.confirmed_examples = (!confirmed.is_empty()).then_some(confirmed);
        let mut issues: Vec<String> = report
            .executions
            .iter()
            .filter(|observation| {
                !observation.passed() && !report.example_confirmed(&observation.id)
            })
            .map(|observation| {
                format!(
                    "Example {} did not complete: {}",
                    observation.id,
                    observation.evidence()
                )
            })
            .collect();
        issues.extend(
            report
                .findings
                .iter()
                .filter(|finding| !finding.publishable())
                .map(issue),
        );
        report.issues = issues;
        report
    }

    /// The lesson waits for the user: the draft stays in the job's
    /// checkpoints and the review row says so (§6.1), with the audit's stop
    /// when the Verifier audit could not finish (§14.6).
    pub(super) async fn stop_for_review(
        &mut self,
        raw: &str,
        stop_reason: Option<StopReason>,
    ) -> Result<()> {
        self.write_review(LessonReviewState::NeedsReview, raw, stop_reason)
            .await
    }

    pub(super) fn tally(&self) -> LearningClaimTally {
        claim_tally(&self.entries)
    }

    /// The user's last action on a claim, when it is the one that set its
    /// state: what the review pass applies to the draft (§6.2).
    pub(super) fn user_change(entry: &ClaimLedgerEntry) -> Option<&UserAction> {
        entry
            .events()
            .iter()
            .rev()
            .find(|record| record.provenance.actor == Actor::User)
            .and_then(|record| match &record.event {
                ClaimEvent::UserAction { action, .. } => Some(action),
                _ => None,
            })
    }
}

/// Where the lesson's claims stand (§6.3). The machine's are told apart by
/// what is done to them next: a first check, research then a recheck, or a
/// rewrite; only the last is a repair.
pub(super) fn claim_tally(entries: &[ClaimLedgerEntry]) -> LearningClaimTally {
    let counts = ClaimCounts::tally(entries.iter());
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    let (mut unchecked, mut searching) = (0, 0);
    for entry in entries {
        match (entry.state(), entry.decided()) {
            (ClaimState::Unchecked, _) => unchecked += 1,
            (
                ClaimState::Open { .. },
                Some(Then::Node {
                    step: Step::Research,
                    ..
                }),
            ) => searching += 1,
            (ClaimState::Open { .. }, None) if entry.may_research() => searching += 1,
            _ => {}
        }
    }
    LearningClaimTally {
        unchecked: count(unchecked),
        searching: count(searching),
        repairing: count(counts.open.saturating_sub(unchecked + searching)),
        waiting: count(counts.queued),
        verified: count(counts.verified),
        resolved_by_you: count(counts.resolved_by_you()),
        rewritten: count(counts.rewritten),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_repair_from_before_narrowing_loads_as_not_narrowing() {
        let saved = serde_json::json!({
            "repairs": [{"claim_id": "c", "unit": 0, "attempt": 2, "escalated": true, "before": "A sentence."}],
        });

        let pending: Pending = serde_json::from_value(saved).unwrap();

        let record = &pending.repairs[0];
        assert!(!record.narrowing);
        assert_eq!(record.goal(), None);
        // Only a narrowing writes the flag, and it reads back as one.
        let written = serde_json::to_value(record).unwrap();
        assert!(written.get("narrowing").is_none(), "{written}");
        let narrowing = RepairRecord {
            narrowing: true,
            ..record.clone()
        };
        let written = serde_json::to_value(&narrowing).unwrap();
        assert_eq!(written["narrowing"], true);
        let read: RepairRecord = serde_json::from_value(written).unwrap();
        assert!(read.narrowing);
        assert_eq!(read.goal(), Some(Goal::KeepTheOthers));
    }

    #[test]
    fn a_reference_change_found_at_publication_is_verified_again_not_an_error() {
        assert!(sources_changed(&AppError::InvalidInput(
            "A verified source is no longer active; verify this lesson again.".into()
        )));
        assert!(sources_changed(&AppError::InvalidInput(
            "Verification source content changed; verify this lesson again.".into()
        )));
        assert!(!sources_changed(&AppError::InvalidInput(
            "A complete verification report for this exact lesson revision is required before publication.".into()
        )));
    }
}
