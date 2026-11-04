//! The readers of the acceptance policy (design §2.6): one adapter per
//! reading, each turning an existing judge path into the policy's outcomes. A
//! reader asks what the pipeline asks of the same statement, restores and
//! records the receipts it does, and leaves the finding it fills; what the
//! answer comes to is interpreted in `lattice-core`, beside the judge. The
//! reading driver asks them along the policy's graphs; the pipeline does not
//! call it yet (build step 6b).
use super::class_checks::finding_of;
use super::classify::{states_a_specific_in, Case};
use super::evidence_checks::{
    alone, applies, bridge_checker, compare, group, labelled_passages, pack, part_wise, AnswerSize,
    CheckedPart, Comparison, Labels, Pass, PendingClaim, NO_PASSAGES,
};
use super::*;
use async_trait::async_trait;
use lattice_core::application::services::claim_verification::{
    page_outcome, BridgeOutcome, BridgePart,
};
use lattice_core::domain::learning::acceptance::{Outcome, Reading};

mod consistency;
pub(super) mod driver;
mod ground;
mod knowledge;
mod page;
mod parts;

/// A statement to read: where it stands, what it says and the class it is
/// checked as.
#[derive(Clone, Copy, Debug)]
pub(super) struct Statement<'s> {
    pub(super) unit: usize,
    pub(super) claim: &'s Claim,
    pub(super) classed: Classed,
}

/// What a reading made of a statement, and the finding it leaves.
#[derive(Debug)]
pub(super) struct Read {
    pub(super) outcome: Outcome,
    pub(super) finding: Finding,
}

/// What the readers read with: the model, the lesson and the course it cites,
/// and the job's saved checks.
pub(super) struct ReadContext<'c> {
    pub(super) llm: &'c dyn LLMPort,
    pub(super) candidate: &'c Value,
    pub(super) references: &'c ReferenceCollection<'c>,
    pub(super) citations: &'c Citations,
    pub(super) executions: &'c [execution::Observation],
    pub(super) retained: &'c ClaimChecks,
    sources: Vec<SourceBinding>,
    labels: Labels,
}

impl<'c> ReadContext<'c> {
    /// The context of a pass over `candidate`, which has the bindings and
    /// labels of the course's sources and the receipts a pass reuses by claim.
    pub(super) async fn new(
        llm: &'c dyn LLMPort,
        candidate: &'c Value,
        references: &'c ReferenceCollection<'_>,
        citations: &'c Citations,
        executions: &'c [execution::Observation],
        retained: &'c ClaimChecks,
    ) -> Result<Self> {
        claim_reuse::load_prior_claims(retained).await?;
        Ok(Self {
            llm,
            candidate,
            references,
            citations,
            executions,
            retained,
            sources: evidence_selection::sources(references),
            labels: provenance_gate::passage_labels(references).await?,
        })
    }
}

/// A kind of reading. It reads each statement as the pipeline reads it today.
#[async_trait]
pub(super) trait Reader: Send + Sync {
    fn reading(&self) -> Reading;
    /// One `Read` per statement, in order; an outage is an error the caller
    /// defers on.
    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>>;
}

/// The reader of each reading, as the reading driver asks for them.
pub(super) trait ReaderSet: Send + Sync {
    fn get(&self, reading: Reading) -> &dyn Reader;
}

/// One reader of each reading.
pub(super) struct Readers {
    parts: parts::Parts,
    ground: ground::Ground,
    page: page::Page,
    consistency: consistency::Consistency,
    knowledge: knowledge::Knowledge,
    veto: knowledge::Knowledge,
}

impl Readers {
    pub(super) fn standard() -> Self {
        Self {
            parts: parts::Parts,
            ground: ground::Ground,
            page: page::Page,
            consistency: consistency::Consistency,
            knowledge: knowledge::Knowledge::knowledge(),
            veto: knowledge::Knowledge::veto(),
        }
    }
}

impl ReaderSet for Readers {
    fn get(&self, reading: Reading) -> &dyn Reader {
        match reading {
            Reading::Parts => &self.parts,
            Reading::Ground => &self.ground,
            Reading::Page => &self.page,
            Reading::Consistency => &self.consistency,
            Reading::Knowledge => &self.knowledge,
            Reading::Veto => &self.veto,
        }
    }
}

/// The rule layer's own test of whether a part of a statement of section
/// `unit` names a specific: the figures its question gives are not, and a
/// hypothetical case's own figures are not (design §2.6).
fn specifics(at: &ReadContext<'_>, unit: usize) -> impl Fn(&str) -> bool {
    let case = Case::of(at.candidate, unit);
    move |part| states_a_specific_in(part, &case)
}

/// Whether the part-wise judge holds a check of `claim` against `passages`
/// beside its answer, as it is shown them: the test of the page `Parts` reads,
/// that `Page` leaves out, and of the section's passages `Ground` reads.
fn parts_fit<'a>(at: &'a ReadContext<'_>) -> impl Fn(&str, &[EvidencePassage]) -> bool + 'a {
    move |claim, passages| {
        bridge_checker(at.llm).bridge_fits(claim, &labelled_passages(passages, &at.labels))
    }
}

/// What a part-wise comparison's key carries beside the claim and its
/// evidence: the guard it asks the specifics test with, the rule layer's own
/// with the section's givens. Pass A asked it with no givens, so a receipt it
/// wrote is never restored for a reading.
const GUARD: &str = "|guard:rule-layer";

/// `statement`'s comparison of `evidence` for `pass`, keyed as the pass keys
/// its own, the part-wise judge's with its guard: `checked` is the statement as
/// the checker reads it.
fn comparison(
    at: &ReadContext<'_>,
    ordinal: usize,
    statement: &Statement<'_>,
    checked: &Claim,
    evidence: Vec<EvidencePassage>,
    pass: Pass,
) -> PendingClaim {
    let key = ClaimChecks::key(statement.unit, checked, &evidence);
    let mut pending = PendingClaim {
        ordinal,
        receipt_key: String::new(),
        key,
        unit: statement.unit,
        claim: statement.claim.clone(),
        checked: checked.statement.clone(),
        evidence,
        pass,
    };
    if applies(&pending) {
        pending.key.push_str(GUARD);
    }
    pending.receipt_key = claim_receipt_key(at.llm, &pending.key);
    pending
}

/// A comparison restored from its receipt, or what is left to ask.
enum Restored {
    Receipt(Finding),
    Ask(PendingClaim),
}

async fn restored(at: &ReadContext<'_>, pending: PendingClaim) -> Result<Restored> {
    match compare(at.llm, at.retained, pending).await? {
        Comparison::Done(finding, _) => Ok(Restored::Receipt(finding)),
        Comparison::Pending(pending) => Ok(Restored::Ask(pending)),
        #[cfg(test)]
        Comparison::Sent => Err(AppError::InternalError(
            "A comparison was sent before it was read".into(),
        )),
    }
}

/// Asks the strict judge about `pending`, packed into requests as the pass
/// packs them, and gives each finding with its comparison's ordinal.
async fn ask_strict(
    at: &ReadContext<'_>,
    pending: Vec<PendingClaim>,
) -> Result<Vec<(usize, Finding)>> {
    let mut asked = Vec::new();
    for request in pack(at.llm, pending, &at.labels) {
        let (results, answered) =
            group(at.llm, request, &|_: Pass, _: &CheckedClaim| {}, &at.labels).await?;
        tally(at.retained, answered);
        asked.extend(
            results
                .into_iter()
                .map(|(_, (ordinal, _, finding, _))| (ordinal, finding)),
        );
    }
    Ok(asked)
}

/// The finding on `evidence` for `statement`, restored from its receipt or
/// asked for, which pass A records under the same key: the part-wise judge's,
/// or the strict judge's of a page that holds no source passage (an example's
/// run is no source).
async fn judge_parts(
    at: &ReadContext<'_>,
    ordinal: usize,
    statement: &Statement<'_>,
    checked: &Claim,
    evidence: Vec<EvidencePassage>,
) -> Result<Finding> {
    let pending = comparison(at, ordinal, statement, checked, evidence, Pass::Attribution);
    let finding = match restored(at, pending).await? {
        Restored::Receipt(finding) => finding,
        Restored::Ask(pending) if applies(&pending) => {
            let guard = specifics(at, statement.unit);
            let ((_, (_, _, finding, _)), answered) =
                part_wise(at.llm, pending, &at.labels, guard).await?;
            tally(at.retained, answered);
            finding
        }
        Restored::Ask(pending) => ask_strict(at, vec![pending])
            .await?
            .into_iter()
            .next()
            .map(|(_, finding)| finding)
            .ok_or_else(|| {
                AppError::InternalError("A comparison was asked without a finding".into())
            })?,
    };
    Ok(alone(finding))
}

/// The finding of a statement with no passage to read, which no judge is
/// asked about.
fn no_passages(statement: &Statement<'_>) -> Finding {
    finding_of(
        (statement.unit, statement.claim, statement.classed),
        ClaimVerdict::Unsupported,
        NO_PASSAGES.into(),
        &[],
        None,
    )
}

/// What the job's judge answers have cost so far, which sizes a page.
fn answers(retained: &ClaimChecks) -> AnswerSize {
    *retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// Adds what a judge's answers cost to the job's tally.
fn tally(retained: &ClaimChecks, answered: AnswerSize) {
    retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .add(answered);
}

/// The part-wise judge's reading of a finding, from the parts it recorded;
/// `None` for a finding that recorded none, such as a strict judge's receipt.
fn bridge_of(finding: &Finding) -> Option<BridgeOutcome> {
    let parts: Vec<BridgePart> = finding
        .two_pass
        .parts
        .iter()
        .map(|part: &CheckedPart| BridgePart {
            part: part.part.clone(),
            kind: part.kind,
            passage: None,
        })
        .collect();
    (!parts.is_empty()).then(|| BridgeOutcome {
        parts,
        reason: finding.reason.clone(),
    })
}

/// What a finding of the part-wise judge on a statement's own citation comes
/// to. A finding with no parts is read by its verdict.
fn parts_outcome(finding: &Finding) -> Outcome {
    match (finding.verdict, bridge_of(finding)) {
        (ClaimVerdict::Unverified, _) => Outcome::Unjudged,
        (_, Some(bridge)) => bridge.outcome(),
        (verdict, None) => page_outcome(verdict, true),
    }
}

/// What a finding of the part-wise judge on a section's passages comes to,
/// as the question whether they ground a statement the model called false.
fn ground_outcome(finding: &Finding) -> Outcome {
    match (finding.verdict, bridge_of(finding)) {
        (ClaimVerdict::Unverified, _) => Outcome::Unjudged,
        (_, Some(bridge)) => bridge.ground_outcome(),
        (ClaimVerdict::Supported, None) => Outcome::Stands,
        (_, None) => Outcome::Refuted,
    }
}
