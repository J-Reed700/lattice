//! How each event moves a claim: the replayed position after an event, and
//! the transitions the ledger applies once it has validated the event.

use super::{
    Actor, AuditedState, Check, CheckBasis, CheckResult, Checker, ClaimState, Class, ClassifiedBy,
    GateVerdict, ReopenCause, TrustAssessment, TrustCause, VerdictKind,
};
use crate::domain::learning::acceptance::{End, Outcome, Step, Then};

/// The replayed position after an event: the state, plus what the rules
/// need to remember about how the claim got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cursor {
    pub(super) state: ClaimState,
    pub(super) basis: Option<CheckBasis>,
    /// A rewrite awaits the check that decides it.
    pub(super) repair_pending: bool,
    /// The machine repaired the claim's citation and no local check has read
    /// the new one yet (course evidence index §5.1).
    pub(super) citation_pending: bool,
    /// Research ran: once per claim for its life, whatever evidence a later
    /// check brings (claim classes design §3.6).
    pub(super) researched: bool,
    /// The latest failing verdict from either checker; cleared by Supported.
    pub(super) latest_failure: Option<VerdictKind>,
    /// What the provenance gate weighed for the latest local check, when
    /// the gate decided it; cleared by every other local check.
    pub(super) trust: Option<TrustAssessment>,
    /// The machine attempts the claim has left for its life, in whatever
    /// state it is: they only fall, so a claim verified and then failing
    /// again resumes from what it had left. A claim born of a repair starts
    /// with its parent's remaining ones (claim sources design §9.1; claim
    /// classes design §3.2).
    pub(super) attempts_left: u8,
    /// The statement's class and the strongest authority that decided it.
    pub(super) class: Option<(Class, ClassifiedBy)>,
    /// The latest decision of the acceptance policy: its version and where it
    /// sent the statement. A claim that has one is read from it, and the
    /// rules below that decide machine transitions leave it alone.
    pub(super) decided: Option<(String, Then)>,
    /// The source version and quote of the latest supported local check, which
    /// a decision to resolve the claim names.
    pub(super) last_support: Option<(String, String)>,
    /// The user asked for a search with a hint and no local check that gave an
    /// answer has read what it found: that check answers the hint (acceptance
    /// design §2.10, a user's hint).
    pub(super) hinted: bool,
    /// The run of a code example the claim stands for.
    pub(super) code_example: Option<String>,
}

pub(super) fn queued(kind: VerdictKind, reason: String) -> ClaimState {
    ClaimState::Queued { kind, reason }
}

/// A reopened claim keeps the closed state it came from, never a chain.
pub(super) fn reopened(cause: ReopenCause, state: ClaimState) -> ClaimState {
    let previous = match state {
        ClaimState::Reopened { previous, .. } => previous,
        other => Box::new(other),
    };
    ClaimState::Reopened { cause, previous }
}

/// The user took the claim over: an Open claim waits for them now; the
/// machine does not repair it again without new information.
pub(super) fn hand_to_user(state: &mut ClaimState) {
    if let ClaimState::Open { kind, reason, .. } = state {
        *state = queued(kind.clone(), std::mem::take(reason));
    }
}

/// A failure of a claim that is not Open: the machine has the attempts the
/// claim has left, for every kind a rewrite or research can address. A
/// dispute and a checker that gave no answer (only a user's evidence meets
/// one) go to the user. A repairable claim with none left
/// waits for the user.
fn first_failure(cursor: &Cursor, kind: VerdictKind, reason: String) -> ClaimState {
    let budget = cursor.attempts_left;
    match kind {
        kind if budget == 0 && budgeted(&kind) => queued(kind, reason),
        VerdictKind::Unsupported
        | VerdictKind::Contradicted
        | VerdictKind::NoSource
        | VerdictKind::InsufficientStanding { .. } => ClaimState::Open {
            kind,
            reason,
            machine_attempts_left: budget,
        },
        VerdictKind::CouldNotJudge | VerdictKind::Disputed { .. } => queued(kind, reason),
    }
}

/// A failure the machine's budget counts: the kinds a rewrite addresses,
/// and a claim below its bar, whose recorded repairs (made before claim
/// sources design §10.4 stopped them) replay as they were. No new repair of
/// one is made (`ClaimLedgerEntry::may_repair`); an Open one research has
/// had its chance on waits for the user (`held_for_standing`).
pub(super) fn budgeted(kind: &VerdictKind) -> bool {
    kind.is_repairable() || matches!(kind, VerdictKind::InsufficientStanding { .. })
}

/// One machine attempt used up: the claim's count falls, and none left
/// queues it.
pub(super) fn charge(cursor: &mut Cursor, kind: VerdictKind, reason: String) -> ClaimState {
    cursor.attempts_left = cursor.attempts_left.saturating_sub(1);
    match cursor.attempts_left {
        0 => queued(kind, reason),
        left => ClaimState::Open {
            kind,
            reason,
            machine_attempts_left: left,
        },
    }
}

/// §3 `after_machine_failure`, applied to a failing check of an Open claim:
/// the check that decides a repair uses an attempt; a kind the user answers
/// goes to the user. New evidence may change the verdict; it never restores
/// the budget (claim classes design §3.2), so a claim's attempts only ever
/// fall.
fn after_machine_failure(
    cursor: &mut Cursor,
    kind: VerdictKind,
    reason: String,
    charged: bool,
) -> ClaimState {
    let machine_attempts_left = cursor.attempts_left;
    match kind {
        kind if budgeted(&kind) && charged => charge(cursor, kind, reason),
        kind if budgeted(&kind) && machine_attempts_left > 0 => ClaimState::Open {
            kind,
            reason,
            machine_attempts_left,
        },
        _ => queued(kind, reason),
    }
}

/// A check's effect. A claim waiting for the user stays the user's: a
/// recheck on new information may verify it or update its reason, never hand
/// it back to an automatic repair (§4, §5.1). `attached`: the check answers
/// evidence the user attached, so an Open claim becomes the user's too. `at`
/// is when the check was recorded, which a pass that rests on no passage
/// carries into its state.
pub(super) fn apply_check(cursor: &mut Cursor, check: &Check, attached: bool, at: i64) {
    let local = !attached && matches!(check.checker, Checker::Local { .. });
    // The first local check that gave an answer after the user's hint is the
    // claim's own: the hint asked for it, and an unjudged check does not use
    // the hint up.
    let answers_hint = local && cursor.hinted && !check.result.is_unjudged();
    if answers_hint {
        cursor.hinted = false;
    }
    // A decided claim's state is its decision's: a local check that is not the
    // user's evidence or the answer to their hint is what was observed, never
    // what follows.
    let observation = cursor.decided.is_some() && local && !answers_hint;
    // A local checker that gave no answer taught nothing about the claim:
    // no attempt is charged and a rewrite keeps waiting for its check. A
    // claim that passed on text since changed has no check now.
    if local && check.result.is_unjudged() {
        if !observation && cursor.state.is_verified() {
            cursor.state = ClaimState::Unchecked;
            cursor.basis = None;
            cursor.trust = None;
        }
        return;
    }
    cursor.latest_failure = match &check.result {
        CheckResult::Failed { kind } => Some(kind.clone()),
        CheckResult::Supported { .. }
        | CheckResult::SupportedElsewhere { .. }
        | CheckResult::Consistent
        | CheckResult::KnownCommon
        | CheckResult::NotFalse => None,
    };
    let model = match &check.checker {
        Checker::Verifier { .. } => {
            apply_verifier(&mut cursor.state, &check.result, &check.reason);
            return;
        }
        Checker::Local { model } => model.clone(),
    };
    // The gate's assessment, if it decided this check, follows it.
    cursor.trust = None;
    let charged = std::mem::take(&mut cursor.repair_pending);
    cursor.citation_pending = false;
    cursor.basis = Some(CheckBasis {
        fingerprint: check.fingerprint.clone(),
        checker_model: model,
        window_tokens: check.window_tokens,
        source_version_ids: check
            .evidence
            .iter()
            .map(|seen| seen.source_version_id.clone())
            .collect(),
    });
    if let Some((source_version_id, supporting_quote)) = check.result.support() {
        cursor.last_support = Some((source_version_id.to_owned(), supporting_quote.to_owned()));
    }
    if observation || cursor.state.is_settled() {
        return;
    }
    cursor.state = match &check.result {
        CheckResult::Supported {
            source_version_id,
            supporting_quote,
        }
        | CheckResult::SupportedElsewhere {
            source_version_id,
            supporting_quote,
            ..
        } => ClaimState::ResolvedByEvidence {
            source_version_id: source_version_id.clone(),
            supporting_quote: supporting_quote.clone(),
        },
        CheckResult::Consistent | CheckResult::NotFalse => ClaimState::Consistent { at },
        CheckResult::KnownCommon => ClaimState::GeneralKnowledge { at },
        CheckResult::Failed { kind } => {
            let (kind, reason) = (kind.clone(), check.reason.clone());
            match std::mem::replace(&mut cursor.state, ClaimState::Unchecked) {
                ClaimState::Open { .. } if attached => queued(kind, reason),
                ClaimState::Open { .. } => after_machine_failure(cursor, kind, reason, charged),
                ClaimState::Queued { .. } | ClaimState::Reopened { .. } => queued(kind, reason),
                _ => first_failure(cursor, kind, reason),
            }
        }
    };
}

/// §14.5: the Verifier only audits. It never resolves a claim; a dispute
/// queues a machine-verified claim and reopens a human verdict.
fn apply_verifier(state: &mut ClaimState, result: &CheckResult, reason: &str) {
    let CheckResult::Failed { kind } = result else {
        return;
    };
    let Some(audited) = state
        .audited_state()
        .filter(|audited| audited.disputed_by(kind))
    else {
        return;
    };
    *state = match audited {
        AuditedState::Verified => queued(
            VerdictKind::Disputed {
                verifier: Box::new(kind.clone()),
            },
            reason.to_string(),
        ),
        AuditedState::Confirmed | AuditedState::GeneralKnowledge => {
            reopened(ReopenCause::VerifierDispute, state.clone())
        }
    };
}

/// The gate's re-decision after a trust change (source provenance §18.7):
/// an insufficient claim the change made sufficient is verified; a verified
/// one it made insufficient is reopened for the user; a claim reopened by an
/// earlier trust change goes back to what it was once its support suffices
/// again. A failing claim keeps its owner and its attempts, with the new
/// failure and reason. The ledger allows the event only on a claim whose
/// state is the gate's ([`super::ClaimLedgerEntry::redecidable`]).
pub(super) fn redecided(
    cursor: &mut Cursor,
    trust: &TrustAssessment,
    reason: &str,
    cause: &TrustCause,
) {
    let verdict = trust.verdict();
    cursor.latest_failure = match &verdict {
        GateVerdict::Sufficient => None,
        GateVerdict::Fails(kind) => Some(kind.clone()),
    };
    cursor.trust = Some(trust.clone());
    let verified = || ClaimState::ResolvedByEvidence {
        source_version_id: trust.supported.source_version_id.clone(),
        supporting_quote: trust.supported.supporting_quote.clone(),
    };
    let state = std::mem::replace(&mut cursor.state, ClaimState::Unchecked);
    cursor.state = match (state, verdict) {
        (ClaimState::Open { .. } | ClaimState::Queued { .. }, GateVerdict::Sufficient) => {
            verified()
        }
        (
            ClaimState::Open {
                machine_attempts_left,
                ..
            },
            GateVerdict::Fails(kind),
        ) => ClaimState::Open {
            kind,
            reason: reason.to_owned(),
            machine_attempts_left,
        },
        (ClaimState::Queued { .. }, GateVerdict::Fails(kind)) => queued(kind, reason.to_owned()),
        (
            ClaimState::Reopened {
                cause: ReopenCause::TrustChanged(_),
                previous,
            },
            GateVerdict::Sufficient,
        ) => *previous,
        (state @ ClaimState::ResolvedByEvidence { .. }, GateVerdict::Fails(_))
        | (state @ ClaimState::Reopened { .. }, GateVerdict::Fails(_)) => {
            reopened(ReopenCause::TrustChanged(cause.clone()), state)
        }
        (state, _) => state,
    };
}

/// Why a decision has no state to give a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Undecidable {
    /// Resolved by evidence, with no supported local check to name.
    NoSupport,
}

/// The state a decision sends a claim to; `None` leaves it where it is. At a
/// node that is a remedy the claim is Open, with the rewrites still reachable
/// from it; at a reading, a recite or an uncite it waits for the machine's next
/// step; at an end it is that end's state (acceptance design §2.9).
pub(super) fn stepped(
    cursor: &Cursor,
    outcome: Outcome,
    then: &Then,
    reason: &str,
    rewrite_ahead: bool,
    at: i64,
) -> Result<Option<ClaimState>, Undecidable> {
    Ok(match then {
        Then::Stop => None,
        Then::Node { step, .. } => Some(match step {
            Step::Research | Step::Rewrite(_) | Step::Cut => ClaimState::Open {
                kind: failure_kind(outcome, &cursor.state),
                reason: reason.to_owned(),
                machine_attempts_left: u8::from(rewrite_ahead),
            },
            Step::Read(_) | Step::Recite | Step::Uncite => ClaimState::Unchecked,
        }),
        Then::End(End::Resolved) => {
            let (source_version_id, supporting_quote) =
                cursor.last_support.clone().ok_or(Undecidable::NoSupport)?;
            Some(ClaimState::ResolvedByEvidence {
                source_version_id,
                supporting_quote,
            })
        }
        // A statement that is not a claim has passed the falsity veto: the
        // machine's pass, as a Reasoning statement's is.
        Then::End(End::Consistent | End::NotAClaim) => Some(ClaimState::Consistent { at }),
        Then::End(End::GeneralKnowledge) => Some(ClaimState::GeneralKnowledge { at }),
        // The ledger publishes it not verified with the reason of its last
        // reading, which this decision does not carry.
        Then::End(End::Unverified) => None,
        Then::End(End::Removed { .. }) => Some(ClaimState::Removed {
            at,
            by: Actor::Machine,
        }),
    })
}

/// What failed a statement, from what the step found: a remedy's outcome keeps
/// the failure the claim was Open with.
fn failure_kind(outcome: Outcome, held: &ClaimState) -> VerdictKind {
    match (outcome, held) {
        (Outcome::Refuted, _) => VerdictKind::Contradicted,
        (Outcome::Unproven | Outcome::NeedsSource | Outcome::AllCommon, _) => {
            VerdictKind::Unsupported
        }
        (_, ClaimState::Open { kind, .. }) => kind.clone(),
        _ => VerdictKind::Unsupported,
    }
}
