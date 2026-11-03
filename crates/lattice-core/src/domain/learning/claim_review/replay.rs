//! How each event moves a claim: the replayed position after an event, and
//! the transitions the ledger applies once it has validated the event.

use super::{
    Check, CheckBasis, CheckResult, Checker, ClaimState, Class, ClassifiedBy, GateVerdict,
    ReopenCause, TrustAssessment, TrustCause, VerdictKind,
};

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
    // A local checker that gave no answer taught nothing about the claim:
    // no attempt is charged and a rewrite keeps waiting for its check. A
    // claim that passed on text since changed has no check now.
    if !attached && matches!(check.checker, Checker::Local { .. }) && check.result.is_unjudged() {
        if matches!(
            cursor.state,
            ClaimState::ResolvedByEvidence { .. }
                | ClaimState::Consistent { .. }
                | ClaimState::GeneralKnowledge { .. }
        ) {
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
        | CheckResult::KnownCommon => None,
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
    if cursor.state.is_settled() {
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
        CheckResult::Consistent => ClaimState::Consistent { at },
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
    let disputes = match state {
        ClaimState::ResolvedByEvidence { .. } => matches!(
            kind,
            VerdictKind::Unsupported
                | VerdictKind::Contradicted
                | VerdictKind::NoSource
                | VerdictKind::CouldNotJudge
        ),
        ClaimState::Confirmed { .. } => matches!(
            kind,
            VerdictKind::Unsupported | VerdictKind::Contradicted | VerdictKind::NoSource
        ),
        ClaimState::GeneralKnowledge { .. } => *kind == VerdictKind::Contradicted,
        _ => false,
    };
    if !disputes {
        return;
    }
    *state = match &*state {
        ClaimState::ResolvedByEvidence { .. } => queued(
            VerdictKind::Disputed {
                verifier: Box::new(kind.clone()),
            },
            reason.to_string(),
        ),
        closed => reopened(ReopenCause::VerifierDispute, closed.clone()),
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
