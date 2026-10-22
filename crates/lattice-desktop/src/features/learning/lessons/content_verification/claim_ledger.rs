//! The claim ledger as the verify-and-repair loop keeps it: which claim each
//! finding is, what the machine did about it, and what happens next.
//!
//! Every pass is observed claim by claim. A check is recorded only when the
//! claim is new or something new about it is known (§3 `new_information`);
//! research runs once per claim and evidence; a repair targets only claims
//! the rules still allow, two per piece of evidence. Once nothing is left for
//! the machine, the lesson publishes if every claim is resolved and otherwise
//! waits for the user's review, with every open claim kept (§4, §6.1).
//! Inside a preparation job the ledger is written through
//! `ClaimReviewRepository`; elsewhere (tests, fixtures) it lives in memory.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §3, §4, §6, §9.

mod audit;
mod observe;
mod receipts;
mod successor;

pub(super) use audit::Audited;

use super::general_knowledge::Resolution;
use super::provenance_gate::ProvenanceGate;
use super::support_gate::SupportGate;
use super::*;
use crate::features::learning::curriculum::LearningClaimTally;
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::lessons::claim_reviews::repository::{
    ClaimReviewRepository, ShownPassage,
};
use crate::features::learning::persistence::now;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    end_state, Actor, ClaimCounts, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState,
    EventProvenance, LessonReview, LessonReviewState, RepairOutcome, Revision, StopReason,
    UserAction, VerdictKind, MACHINE_ATTEMPTS,
};

pub(in crate::features::learning) use observe::{field_text, unit_texts};

/// The lesson checkpoint holding repairs awaiting the pass that decides them.
const PENDING_KEY: &str = "claim-ledger-v1:pending";
/// The prefix of a job's result reference when lessons wait for review.
const NEEDS_REVIEW_REF: &str = "needs_review:";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A repair sent for a claim, awaiting the pass that shows what became of
/// its sentence: rewritten (the check decides) or removed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct RepairRecord {
    pub(super) claim_id: ClaimId,
    pub(super) unit: usize,
    attempt: u8,
    escalated: bool,
    before: String,
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

/// What the machine does next, once a pass is observed.
pub(super) enum Decision {
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
    /// The claim of each finding of the latest observed report, in order.
    findings: Vec<ClaimId>,
    /// The claim of each tracked example, by observation id.
    examples: HashMap<String, ClaimId>,
    /// Claims whose user edit was just applied: re-extraction names their
    /// successors (§5.1 Edit).
    pub(super) edited_now: HashSet<ClaimId>,
    gate: Box<dyn SupportGate>,
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
            edited_now: HashSet::new(),
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

    /// The claims of the latest pass, findings and examples.
    fn observed(&self) -> impl Iterator<Item = &ClaimId> {
        self.findings.iter().chain(self.examples.values())
    }

    /// §4: repair what the rules allow; otherwise publish if every claim is
    /// resolved, else hand the lesson to the user (BLOCKED, §2.1).
    pub(super) fn decide(&self) -> Decision {
        let repairable: Vec<ClaimId> = self
            .observed()
            .filter(|id| self.entry(id).is_some_and(ClaimLedgerEntry::may_repair))
            .cloned()
            .collect();
        if !repairable.is_empty() {
            return Decision::Repair(repairable);
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

    /// The repair sent for `claims` (§4): which attempt it is for each claim,
    /// saved with the lesson as it stood so the pass after it is charged even
    /// after a restart.
    pub(super) async fn begin_repairs(
        &mut self,
        candidate: &Value,
        claims: &[ClaimId],
    ) -> Result<()> {
        let records: Vec<RepairRecord> = claims
            .iter()
            .filter_map(|id| {
                let entry = self.entry(id)?;
                let ClaimState::Open {
                    machine_attempts_left,
                    ..
                } = entry.state()
                else {
                    return None;
                };
                Some(RepairRecord {
                    claim_id: id.clone(),
                    unit: entry.location().unit_index,
                    attempt: (MACHINE_ATTEMPTS + 1).saturating_sub(*machine_attempts_left),
                    escalated: entry.next_repair_escalates(),
                    before: entry.location().quote.clone(),
                })
            })
            .collect();
        self.pending.repairs.extend(records);
        self.pending
            .previous
            .get_or_insert_with(|| candidate.clone());
        self.save_pending().await
    }

    /// The repair of these claims produced nothing usable; each uses its
    /// attempt at once.
    pub(super) async fn reject(&mut self, claims: &[ClaimId], reason: &str) -> Result<()> {
        self.close_records(
            claims,
            RepairOutcome::Rejected {
                reason: reason.into(),
            },
        )
        .await
    }

    /// The user resolved these claims while their repair waited: it is not
    /// sent and not charged (§9).
    pub(super) async fn supersede(&mut self, claims: &[ClaimId]) -> Result<()> {
        self.close_records(claims, RepairOutcome::SupersededByUser)
            .await
    }

    async fn close_records(&mut self, claims: &[ClaimId], outcome: RepairOutcome) -> Result<()> {
        let (closed, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending.repairs)
            .into_iter()
            .partition(|record| claims.contains(&record.claim_id));
        self.pending.repairs = kept;
        for record in closed {
            self.append(
                &record.claim_id,
                ClaimEvent::RepairAttempted {
                    attempt: record.attempt,
                    escalated: record.escalated,
                    before: record.before,
                    after: None,
                    outcome: outcome.clone(),
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

    /// Whether the machine may still repair this claim (a sync point's test).
    pub(super) fn may_repair(&self, claim_id: &ClaimId) -> bool {
        self.entry(claim_id)
            .is_some_and(ClaimLedgerEntry::may_repair)
    }

    /// The report the lesson publishes with: each finding carries the user's
    /// resolution, and the issues are what the ledger leaves open.
    pub(super) fn bind(&self, mut report: LessonVerificationReport) -> LessonVerificationReport {
        let resolution = |id: &ClaimId| match self.entry(id).map(ClaimLedgerEntry::state) {
            Some(ClaimState::Confirmed { at, note, .. }) => Some(Resolution::Confirmed {
                at: *at,
                note: note.clone(),
            }),
            Some(ClaimState::GeneralKnowledge { at, .. }) => {
                Some(Resolution::GeneralKnowledge { at: *at })
            }
            _ => None,
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
                        .is_some_and(|id| resolution(id).is_some())
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
/// what is done to them next: a first check, research then a recheck (the
/// source check's apart from the checker's), or a rewrite; only the last is
/// a repair.
pub(super) fn claim_tally(entries: &[ClaimLedgerEntry]) -> LearningClaimTally {
    let counts = ClaimCounts::tally(entries.iter());
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    let (mut unchecked, mut searching, mut sent_back) = (0, 0, 0);
    for entry in entries {
        match entry.state() {
            ClaimState::Unchecked => unchecked += 1,
            ClaimState::Open {
                kind: VerdictKind::InsufficientStanding { .. },
                ..
            } if entry.may_research() => sent_back += 1,
            ClaimState::Open { .. } if entry.may_research() => searching += 1,
            _ => {}
        }
    }
    LearningClaimTally {
        unchecked: count(unchecked),
        searching: count(searching),
        sent_back: count(sent_back),
        repairing: count(
            counts
                .open
                .saturating_sub(unchecked + searching + sent_back),
        ),
        waiting: count(counts.queued),
        verified: count(counts.verified),
        resolved_by_you: count(counts.resolved_by_you()),
        rewritten: count(counts.rewritten),
    }
}

/// What researching one claim on the web cost when a research round last
/// measured it, from any lesson of any program: it describes the backend and
/// the network, not the course (course evidence index D5). `None` outside a
/// preparation job, and before any round was measured.
pub(super) async fn research_cost() -> Result<Option<std::time::Duration>> {
    let Some(scope) = crate::features::learning::lesson_drafts::scope() else {
        return Ok(None);
    };
    Ok(ClaimReviewRepository::new(scope.pool)
        .research_ms_per_claim()
        .await?
        .map(std::time::Duration::from_millis))
}

/// The error that unwinds a lesson's preparation when it waits for the user's
/// review. It is not a failure: the job reads the review row and completes
/// (§6.1, `generation_jobs`). Its text is the hold's own words, with no
/// failure category in front of them, wherever it is shown or logged.
pub(in crate::features::learning) fn needs_review(claims: usize) -> AppError {
    AppError::Other(format!(
        "Needs your review: {claims} claim{} could not be verified. Resolve each one, then resume verification.",
        if claims == 1 { "" } else { "s" }
    ))
}

/// The lessons a job's result reference names as waiting for review: the
/// inverse of [`review_outcome`].
pub(in crate::features::learning) fn held_lessons(result_ref: &str) -> Option<Vec<String>> {
    let ids = result_ref.strip_prefix(NEEDS_REVIEW_REF)?;
    Some(
        ids.split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// A lesson of a preparation job that waits for the user's review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features::learning) struct HeldForReview {
    pub lesson_id: String,
    /// Claims waiting for the user.
    pub claims: usize,
}

/// Whether `lesson_id`'s preparation in `job_id` stopped because the lesson
/// waits for the user's review (§6.1), and for how many claims: the review
/// row, not the error that unwound the preparation, is what says so.
pub(in crate::features::learning) async fn held_for_review(
    pool: &sqlx::SqlitePool,
    lesson_id: &str,
    job_id: &str,
) -> Result<Option<HeldForReview>> {
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(lesson_id)
        .await?;
    let held = ledger.review.as_ref().is_some_and(|review| {
        review.state == LessonReviewState::NeedsReview && review.job_id == job_id
    });
    Ok(held.then(|| {
        let counts = ClaimCounts::tally(ledger.claims.iter());
        HeldForReview {
            lesson_id: lesson_id.to_owned(),
            claims: counts.queued + counts.open,
        }
    }))
}

/// The completed job's result reference and message when lessons wait for
/// review (§6.1): `needs_review:<lesson id>[,<lesson id>…]`.
pub(in crate::features::learning) fn review_outcome(held: &[HeldForReview]) -> (String, String) {
    let ids: Vec<&str> = held.iter().map(|hold| hold.lesson_id.as_str()).collect();
    let claims: usize = held.iter().map(|hold| hold.claims).sum();
    let plural =
        |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    let message = if held.len() == 1 {
        format!("Needs your review: {}", plural(claims, "claim"))
    } else {
        format!(
            "Needs your review: {} in {}",
            plural(claims, "claim"),
            plural(held.len(), "lesson")
        )
    };
    (format!("{NEEDS_REVIEW_REF}{}", ids.join(",")), message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_change_found_at_publication_is_verified_again_not_an_error() {
        assert!(sources_changed(&AppError::InvalidInput(
            "The reference collection changed during verification. Prepare the lesson again."
                .into()
        )));
        assert!(sources_changed(&AppError::InvalidInput(
            "Verification source content changed; verify this lesson again.".into()
        )));
        assert!(!sources_changed(&AppError::InvalidInput(
            "A complete verification report for this exact lesson revision is required before publication.".into()
        )));
    }

    #[test]
    fn several_held_lessons_complete_the_job_as_needs_review_for_each() {
        let held = |lesson: &str, claims| HeldForReview {
            lesson_id: lesson.into(),
            claims,
        };
        assert_eq!(
            review_outcome(&[held("a", 2), held("b", 3)]),
            (
                "needs_review:a,b".to_owned(),
                "Needs your review: 5 claims in 2 lessons".to_owned()
            )
        );
    }
}
