//! How each event moves a claim: the replayed position after an event, and
//! the transitions the ledger applies once it has validated the event.

use super::{
    Check, CheckBasis, CheckResult, Checker, ClaimState, GateVerdict, ReopenCause, TrustAssessment,
    TrustCause, VerdictKind, MACHINE_ATTEMPTS,
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
    /// What the provenance gate weighed for the latest local check, when
    /// the gate decided it; cleared by every other local check.
    pub(super) trust: Option<TrustAssessment>,
    /// A claim born of a repair starts with its parent's remaining attempts
    /// instead of a full budget (claim sources design §9.1).
    pub(super) inherited_budget: Option<u8>,
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
/// budget for the kinds a rewrite or research can address, or, for a claim
/// born of a repair, what its parent had left. A repairable claim with no
/// attempts left waits for the user.
fn first_failure(kind: VerdictKind, reason: String, inherited: Option<u8>) -> ClaimState {
    let budget = inherited.unwrap_or(MACHINE_ATTEMPTS);
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
        VerdictKind::CouldNotJudge
        | VerdictKind::GeneralKnowledgeProposed
        | VerdictKind::Disputed { .. } => queued(kind, reason),
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
        return first_failure(kind, reason, cursor.inherited_budget);
    };
    if new_evidence {
        return first_failure(kind, reason, cursor.inherited_budget);
    }
    match kind {
        kind if budgeted(&kind) && charged => charge(kind, reason, machine_attempts_left),
        kind if budgeted(&kind) && machine_attempts_left > 0 => ClaimState::Open {
            kind,
            reason,
            machine_attempts_left,
        },
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
    let previous_failure = std::mem::replace(
        &mut cursor.latest_failure,
        match &check.result {
            CheckResult::Failed { kind } => Some(kind.clone()),
            CheckResult::Supported { .. } | CheckResult::SupportedElsewhere { .. } => None,
        },
    );
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
    let new_evidence = !charged
        && cursor
            .basis
            .as_ref()
            .is_none_or(|basis| basis.fingerprint != check.fingerprint)
        && !support_unchanged(previous_failure.as_ref(), &check.result);
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
        }
        | CheckResult::SupportedElsewhere {
            source_version_id,
            supporting_quote,
            ..
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
                _ => first_failure(kind, reason, cursor.inherited_budget),
            }
        }
    };
}

/// Whether a failing check on other evidence leaves the claim's support
/// where it was (source provenance §6.2, §8): its supports still fail the
/// same bar, and none it gained comes from an origin cluster it lacked or
/// stands above the best it had. Such evidence cannot make the claim
/// sufficient, so it is not new information: it restores no budget and
/// earns no further research round, and the claim goes on to its repairs
/// and then the user, as a citation hop that adds no new cluster ends its
/// chain. Without this, every research round that saved anything at all
/// would restart every scrutinised claim it touched.
fn support_unchanged(previous: Option<&VerdictKind>, result: &CheckResult) -> bool {
    let (
        Some(VerdictKind::InsufficientStanding {
            bar: bar_before,
            have: before,
        }),
        CheckResult::Failed {
            kind: VerdictKind::InsufficientStanding { bar, have: after },
        },
    ) = (previous, result)
    else {
        return false;
    };
    let had = |version: &str| {
        before
            .iter()
            .any(|support| support.source_version_id == version)
    };
    let best_before = before.iter().filter_map(|support| support.counted).max();
    bar == bar_before
        && !after.iter().any(|gained| {
            let Some(level) = gained.counted else {
                return false;
            };
            // Cluster ids name a cluster's smallest member, so a cluster is
            // new when none of its members was a support before.
            let new_cluster = !after
                .iter()
                .filter(|member| member.origin_cluster == gained.origin_cluster)
                .any(|member| had(&member.source_version_id));
            !had(&gained.source_version_id)
                && (new_cluster || best_before.is_none_or(|best| level > best))
        })
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
