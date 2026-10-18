//! How each event moves a claim: the replayed position after an event, and
//! the transitions the ledger applies once it has validated the event.

use super::{
    Check, CheckBasis, CheckResult, Checker, ClaimState, ReopenCause, VerdictKind, MACHINE_ATTEMPTS,
};

/// The replayed position after an event: the state, plus what the rules
/// need to remember about how the claim got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cursor {
    pub(super) state: ClaimState,
    pub(super) basis: Option<CheckBasis>,
    /// A rewrite awaits the check that decides it.
    pub(super) repair_pending: bool,
    /// Research ran under the current fingerprint.
    pub(super) researched: bool,
    /// The latest failing verdict from either checker; cleared by Supported.
    pub(super) latest_failure: Option<VerdictKind>,
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

/// A first failure, or a failure on new evidence: the machine gets its full
/// budget for the kinds a rewrite or research can address.
fn first_failure(kind: VerdictKind, reason: String) -> ClaimState {
    match kind {
        VerdictKind::Unsupported | VerdictKind::Contradicted | VerdictKind::NoSource => {
            ClaimState::Open {
                kind,
                reason,
                machine_attempts_left: MACHINE_ATTEMPTS,
            }
        }
        VerdictKind::CouldNotJudge
        | VerdictKind::GeneralKnowledgeProposed
        | VerdictKind::Disputed { .. } => queued(kind, reason),
    }
}

/// One machine attempt used up on the same evidence; none left queues it.
pub(super) fn charge(kind: VerdictKind, reason: String, attempts_left: u8) -> ClaimState {
    match attempts_left.saturating_sub(1) {
        0 => queued(kind, reason),
        left => ClaimState::Open {
            kind,
            reason,
            machine_attempts_left: left,
        },
    }
}

/// §3 `after_machine_failure`, applied to a failing check of an Open claim:
/// new evidence restores the budget; the check that decides a repair uses an
/// attempt; a kind the machine cannot repair, or no source after research,
/// goes to the user. `new_evidence` is false for the check that decides a
/// rewrite even when the rewritten sentence retrieved other passages: what
/// the machine's own rewrite brings is not new information, or a claim the
/// machine keeps rewriting would never reach the user (§4 "Bounded").
fn after_machine_failure(
    cursor: &Cursor,
    kind: VerdictKind,
    reason: String,
    new_evidence: bool,
    charged: bool,
) -> ClaimState {
    let ClaimState::Open {
        machine_attempts_left,
        ..
    } = cursor.state
    else {
        return first_failure(kind, reason);
    };
    if new_evidence {
        return first_failure(kind, reason);
    }
    match kind {
        VerdictKind::Unsupported | VerdictKind::Contradicted if charged => {
            charge(kind, reason, machine_attempts_left)
        }
        VerdictKind::Unsupported | VerdictKind::Contradicted if machine_attempts_left > 0 => {
            ClaimState::Open {
                kind,
                reason,
                machine_attempts_left,
            }
        }
        VerdictKind::NoSource if !cursor.researched => ClaimState::Open {
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
/// evidence the user attached, so an Open claim becomes the user's too.
pub(super) fn apply_check(cursor: &mut Cursor, check: &Check, attached: bool) {
    cursor.latest_failure = match &check.result {
        CheckResult::Supported { .. } => None,
        CheckResult::Failed { kind } => Some(kind.clone()),
    };
    let model = match &check.checker {
        Checker::Verifier { .. } => {
            apply_verifier(&mut cursor.state, &check.result, &check.reason);
            return;
        }
        Checker::Local { model } => model.clone(),
    };
    let charged = std::mem::take(&mut cursor.repair_pending);
    let new_evidence = !charged
        && cursor
            .basis
            .as_ref()
            .is_none_or(|basis| basis.fingerprint != check.fingerprint);
    if new_evidence {
        cursor.researched = false;
    }
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
        } => ClaimState::ResolvedByEvidence {
            source_version_id: source_version_id.clone(),
            supporting_quote: supporting_quote.clone(),
        },
        CheckResult::Failed { kind } => {
            let (kind, reason) = (kind.clone(), check.reason.clone());
            match &cursor.state {
                ClaimState::Open { .. } if attached => queued(kind, reason),
                ClaimState::Open { .. } => {
                    after_machine_failure(cursor, kind, reason, new_evidence, charged)
                }
                ClaimState::Queued { .. } | ClaimState::Reopened { .. } => queued(kind, reason),
                _ => first_failure(kind, reason),
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
