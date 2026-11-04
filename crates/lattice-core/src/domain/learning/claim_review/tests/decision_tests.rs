//! The ledger records the acceptance policy's decisions, and a claim that has
//! one is read from it: its state follows each decision, a check after it is
//! what was observed, and a record that does not walk is a fault.

use super::*;
use crate::domain::learning::acceptance::{
    check_record, decide, rewrite_ahead, AcceptancePolicy, End, Goal, Outcome, Position, Reading,
    StandardPolicy, Step, Taken, Then,
};
use serde_json::json;

const POLICY: &str = "acceptance-v1";
const READ: Step = Step::Read(Reading::Parts);
const REWRITE: Step = Step::Rewrite(Goal::WhatThePassageStates);

fn node(name: &str, step: Step) -> Then {
    Then::Node {
        name: name.into(),
        step,
    }
}

fn stepped_by(
    policy: &str,
    step: Step,
    outcome: Outcome,
    then: Then,
    reason: &str,
    rewrite_ahead: bool,
) -> ClaimEvent {
    ClaimEvent::Stepped {
        policy: policy.into(),
        step,
        outcome,
        then,
        reason: reason.into(),
        rewrite_ahead,
    }
}

fn stepped_with(
    step: Step,
    outcome: Outcome,
    then: Then,
    reason: &str,
    rewrite_ahead: bool,
) -> ClaimEvent {
    stepped_by(POLICY, step, outcome, then, reason, rewrite_ahead)
}

fn stepped(step: Step, outcome: Outcome, then: Then) -> ClaimEvent {
    stepped_with(step, outcome, then, "Because.", false)
}

fn passes(result: CheckResult) -> ClaimEvent {
    ClaimEvent::Checked(Check {
        result,
        ..local_check(None, "v1")
    })
}

/// A claim the policy has sent to the rewrite of a Sourced statement.
fn decided_open(id: &str) -> ClaimLedgerEntry {
    let mut entry = claim(id);
    entry
        .append(
            machine(2),
            stepped_with(
                READ,
                Outcome::Unproven,
                node("s.rewrite", REWRITE),
                "No passage states it.",
                true,
            ),
        )
        .unwrap();
    entry
}

#[test]
fn a_decision_to_a_remedy_leaves_the_claim_open_with_the_rewrites_ahead() {
    let mut entry = claim("c1");
    entry
        .append(
            machine(2),
            stepped_with(
                READ,
                Outcome::Unproven,
                node("s.research", Step::Research),
                "No passage states it.",
                true,
            ),
        )
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "No passage states it.".into(),
            machine_attempts_left: 1,
        }
    );
    assert_eq!(entry.decided(), Some(&node("s.research", Step::Research)));

    // At the cut no rewrite is ahead, and the failure is the one it had.
    entry
        .append(
            machine(3),
            stepped_with(
                REWRITE,
                Outcome::NotApplied,
                node("s.cut", Step::Cut),
                "The edit was refused.",
                false,
            ),
        )
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "The edit was refused.".into(),
            machine_attempts_left: 0,
        }
    );
}

#[test]
fn the_outcome_gives_the_failure_a_claim_is_open_with() {
    for (outcome, kind) in [
        (Outcome::Refuted, VerdictKind::Contradicted),
        (Outcome::Unproven, VerdictKind::Unsupported),
        (Outcome::NeedsSource, VerdictKind::Unsupported),
        (Outcome::AllCommon, VerdictKind::Unsupported),
    ] {
        let mut entry = claim("c1");
        entry
            .append(
                machine(2),
                stepped(READ, outcome, node("s.rewrite", REWRITE)),
            )
            .unwrap();
        assert_eq!(entry.state().verdict(), Some(&kind), "{outcome:?}");
    }

    // A remedy's outcome keeps the failure the claim was open with, and
    // starts from Unsupported when it was not.
    let mut contradicted = claim("c2");
    contradicted
        .append(
            machine(2),
            stepped(
                Step::Read(Reading::Consistency),
                Outcome::Refuted,
                node("r.rewrite", REWRITE),
            ),
        )
        .unwrap();
    contradicted
        .append(
            machine(3),
            stepped(
                REWRITE,
                Outcome::NotApplied,
                node("r.rewrite.again", REWRITE),
            ),
        )
        .unwrap();
    assert_eq!(
        contradicted.state().verdict(),
        Some(&VerdictKind::Contradicted)
    );
    let mut fresh = claim("c3");
    fresh
        .append(
            machine(2),
            stepped(Step::Research, Outcome::Saved, node("s.rewrite", REWRITE)),
        )
        .unwrap();
    assert_eq!(fresh.state().verdict(), Some(&VerdictKind::Unsupported));
}

#[test]
fn a_decision_to_a_reading_a_recite_or_an_uncite_leaves_the_claim_unchecked() {
    for then in [
        node("s.reread", Step::Read(Reading::Page)),
        node("s.citeread", READ),
        node("c.ground", Step::Read(Reading::Ground)),
        node("s.recite", Step::Recite),
        node("s.uncite", Step::Uncite),
    ] {
        let mut entry = decided_open("c1");
        assert!(matches!(entry.state(), ClaimState::Open { .. }));
        entry
            .append(
                machine(3),
                stepped(Step::Research, Outcome::Saved, then.clone()),
            )
            .unwrap();
        assert_eq!(entry.state(), &ClaimState::Unchecked, "{then:?}");
        assert_eq!(entry.decided(), Some(&then));
    }
}

#[test]
fn a_decision_to_an_end_gives_that_ends_state() {
    let veto = Step::Read(Reading::Veto);
    for (end, state) in [
        (End::Consistent, ClaimState::Consistent { at: 7 }),
        (
            End::GeneralKnowledge,
            ClaimState::GeneralKnowledge { at: 7 },
        ),
        (
            End::Removed {
                section_goes: false,
            },
            ClaimState::Removed {
                at: 7,
                by: Actor::Machine,
            },
        ),
        (
            End::Removed { section_goes: true },
            ClaimState::Removed {
                at: 7,
                by: Actor::Machine,
            },
        ),
    ] {
        let mut entry = decided_open("c1");
        entry
            .append(machine(7), stepped(veto, Outcome::Stands, Then::End(end)))
            .unwrap();
        assert_eq!(entry.state(), &state, "{end:?}");
    }
}

#[test]
fn resolved_by_evidence_takes_the_latest_support() {
    let mut entry = claim("c1");
    entry.append(machine(2), checked(None, "v1")).unwrap();
    entry
        .append(
            machine(3),
            stepped(READ, Outcome::Unproven, node("s.research", Step::Research)),
        )
        .unwrap();
    // Observed after the decision: it moves nothing, and is the latest support.
    entry
        .append(
            machine(4),
            ClaimEvent::Checked(Check {
                result: CheckResult::Supported {
                    source_version_id: "v2".into(),
                    supporting_quote: "a second quotation".into(),
                },
                ..local_check(None, "v2")
            }),
        )
        .unwrap();
    assert!(matches!(entry.state(), ClaimState::Open { .. }));
    entry
        .append(
            machine(5),
            stepped(
                Step::Read(Reading::Page),
                Outcome::Stands,
                Then::End(End::Resolved),
            ),
        )
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::ResolvedByEvidence {
            source_version_id: "v2".into(),
            supporting_quote: "a second quotation".into(),
        }
    );
}

#[test]
fn a_decision_to_resolve_without_a_supported_check_is_refused() {
    let resolve = || stepped(READ, Outcome::Stands, Then::End(End::Resolved));
    let mut entry = claim("c1");
    assert_eq!(
        entry.append(machine(2), resolve()).unwrap_err(),
        LedgerError::ResolvedWithoutSupport { claim: "c1".into() }
    );
    // A check that failed names no support either.
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    assert_eq!(
        entry.append(machine(3), resolve()).unwrap_err(),
        LedgerError::ResolvedWithoutSupport { claim: "c1".into() }
    );
    assert_eq!(entry.decided(), None);
    assert_eq!(entry.events().len(), 2);
}

#[test]
fn the_end_of_a_statement_that_is_not_a_claim_is_a_pass_the_machine_made() {
    let veto = Step::Read(Reading::Veto);
    let mut entry = decided_open("c1");

    let recorded = entry.append(
        machine(7),
        stepped(veto, Outcome::Stands, Then::End(End::NotAClaim)),
    );

    assert!(recorded.is_ok(), "{recorded:?}");
    assert_eq!(entry.state(), &ClaimState::Consistent { at: 7 });
    assert_eq!(entry.decided(), Some(&Then::End(End::NotAClaim)));
}

#[test]
fn the_end_of_a_statement_that_is_not_a_claim_has_no_state_yet() {
    let mut entry = claim("c1");
    assert_eq!(
        entry
            .append(
                machine(2),
                stepped(
                    Step::Read(Reading::Veto),
                    Outcome::Stands,
                    Then::End(End::NotAClaim)
                )
            )
            .unwrap_err(),
        LedgerError::NoStateForEnd {
            claim: "c1".into(),
            end: "not a claim"
        }
    );
    assert_eq!(entry.state(), &ClaimState::Unchecked);
}

#[test]
fn a_stop_changes_no_state_and_is_recorded() {
    let mut entry = decided_open("c1");
    let open = entry.state().clone();
    entry
        .append(machine(3), stepped(READ, Outcome::Unjudged, Then::Stop))
        .unwrap();
    assert_eq!(entry.state(), &open);
    assert_eq!(entry.decided(), Some(&Then::Stop));
}

#[test]
fn a_check_after_a_decision_is_what_was_observed_and_moves_nothing() {
    let mut entry = decided_open("c1");
    let open = entry.state().clone();
    for result in [
        CheckResult::Supported {
            source_version_id: "v1".into(),
            supporting_quote: "boils at 100 °C".into(),
        },
        CheckResult::Failed {
            kind: VerdictKind::Contradicted,
        },
        CheckResult::Consistent,
        CheckResult::KnownCommon,
        CheckResult::NotFalse,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge,
        },
    ] {
        entry.append(machine(3), passes(result)).unwrap();
        assert_eq!(entry.state(), &open);
    }

    // What it observed is still kept.
    let mut entry = decided_open("c2");
    assert!(entry.basis().is_none());
    entry
        .append(machine(3), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    assert_eq!(entry.latest_failure(), Some(&VerdictKind::Contradicted));
    assert!(entry.basis().unwrap().source_version_ids.contains("v2"));
    entry.append(machine(4), checked(None, "v3")).unwrap();
    assert_eq!(entry.latest_failure(), None);
}

#[test]
fn a_decided_claim_that_passed_stays_passed_whatever_a_check_finds() {
    let mut entry = claim("c1");
    entry.append(machine(2), checked(None, "v1")).unwrap();
    entry
        .append(
            machine(3),
            stepped(
                Step::Read(Reading::Veto),
                Outcome::Stands,
                Then::End(End::Consistent),
            ),
        )
        .unwrap();
    let consistent = ClaimState::Consistent { at: 3 };
    assert_eq!(entry.state(), &consistent);
    entry
        .append(machine(4), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    assert_eq!(entry.state(), &consistent);
    // And a check that gave no answer does not take its pass away.
    entry
        .append(
            machine(5),
            passes(CheckResult::Failed {
                kind: VerdictKind::CouldNotJudge,
            }),
        )
        .unwrap();
    assert_eq!(entry.state(), &consistent);

    // The rules for a claim with no decision are as they were.
    let mut undecided = verified_claim("c2");
    undecided
        .append(
            machine(3),
            passes(CheckResult::Failed {
                kind: VerdictKind::CouldNotJudge,
            }),
        )
        .unwrap();
    assert_eq!(undecided.state(), &ClaimState::Unchecked);
}

/// A decided claim the user took over with a request for a search.
fn hinted(id: &str) -> ClaimLedgerEntry {
    let mut entry = decided_open(id);
    let hint = UserAction::ResearchWithHint {
        hint: "the monarch tagging study".into(),
    };
    entry.append(user(3), act(hint, None)).unwrap();
    entry
}

#[test]
fn a_decided_claim_the_user_holds_with_a_hint_is_verified_by_a_passing_local_check() {
    let mut entry = hinted("c1");
    assert!(matches!(entry.state(), ClaimState::Queued { .. }));

    entry.append(machine(4), checked(None, "v2")).unwrap();

    assert_eq!(
        entry.state(),
        &ClaimState::ResolvedByEvidence {
            source_version_id: "v2".into(),
            supporting_quote: "boils at 100 °C".into(),
        }
    );
    // The check answered the hint; the decision the claim was held at is the
    // machine's, and stays where it was.
    assert_eq!(entry.decided(), Some(&node("s.rewrite", REWRITE)));
}

#[test]
fn a_failing_check_after_a_hint_leaves_the_claim_queued_with_the_checks_reason() {
    let mut entry = hinted("c1");

    entry
        .append(machine(4), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();

    assert_eq!(
        entry.state(),
        &ClaimState::Queued {
            kind: VerdictKind::Contradicted,
            reason: "compared v2".into(),
        }
    );
    assert_eq!(entry.latest_failure(), Some(&VerdictKind::Contradicted));
}

#[test]
fn a_later_local_check_with_no_new_hint_is_an_observation() {
    // The hint's answer failed; a passing check after it verifies nothing.
    let mut entry = hinted("c1");
    entry
        .append(machine(4), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    let queued = entry.state().clone();
    entry.append(machine(5), checked(None, "v3")).unwrap();
    assert_eq!(entry.state(), &queued);

    // A new hint asks for the next one.
    let hint = UserAction::ResearchWithHint {
        hint: "the 2019 survey".into(),
    };
    entry.append(user(6), act(hint, None)).unwrap();
    entry.append(machine(7), checked(None, "v4")).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    // A claim the hint verified stays verified whatever a check finds after.
    let verified = entry.state().clone();
    entry
        .append(machine(8), checked(Some(VerdictKind::Contradicted), "v5"))
        .unwrap();
    assert_eq!(entry.state(), &verified);
}

#[test]
fn an_unjudged_check_leaves_the_hint_to_the_next() {
    let mut entry = hinted("c1");
    let queued = entry.state().clone();

    entry
        .append(
            machine(4),
            passes(CheckResult::Failed {
                kind: VerdictKind::CouldNotJudge,
            }),
        )
        .unwrap();
    assert_eq!(entry.state(), &queued);

    entry.append(machine(5), checked(None, "v2")).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
}

#[test]
fn another_action_of_the_user_ends_the_ask_of_a_hint() {
    let mut entry = hinted("c1");
    entry
        .append(user(4), act(confirm(), Some("Seen.")))
        .unwrap();
    entry
        .append(user(5), act(UserAction::Unconfirm, None))
        .unwrap();
    let reopened = entry.state().clone();
    assert!(matches!(reopened, ClaimState::Reopened { .. }));

    entry.append(machine(6), checked(None, "v2")).unwrap();

    assert_eq!(entry.state(), &reopened);
}

#[test]
fn a_verifier_dispute_still_queues_a_decided_claim() {
    let mut entry = claim("c1");
    entry.append(machine(2), checked(None, "v1")).unwrap();
    entry
        .append(
            machine(3),
            stepped(READ, Outcome::Stands, Then::End(End::Resolved)),
        )
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    entry
        .append(machine(4), verifier(Some(VerdictKind::Unsupported)))
        .unwrap();
    let queued = ClaimState::Queued {
        kind: VerdictKind::Disputed {
            verifier: Box::new(VerdictKind::Unsupported),
        },
        reason: "compared v1".into(),
    };
    assert_eq!(entry.state(), &queued);

    // A decision made now is recorded, and the claim stays the user's.
    entry
        .append(
            machine(5),
            stepped(READ, Outcome::Unproven, node("s.research", Step::Research)),
        )
        .unwrap();
    assert_eq!(entry.state(), &queued);
    assert_eq!(entry.decided(), Some(&node("s.research", Step::Research)));
}

#[test]
fn a_decision_on_a_claim_waiting_for_the_user_is_recorded_without_moving_it() {
    let mut entry = queued_claim("c1", VerdictKind::Contradicted);
    let waiting = entry.state().clone();
    entry
        .append(
            machine(7),
            stepped(READ, Outcome::Refuted, node("s.rewrite", REWRITE)),
        )
        .unwrap();
    assert_eq!(entry.state(), &waiting);
    assert_eq!(entry.decided(), Some(&node("s.rewrite", REWRITE)));
}

#[test]
fn a_users_action_overrides_a_decision_and_settled_claims_take_none() {
    let mut entry = decided_open("c1");
    entry.append(user(3), act(confirm(), None)).unwrap();
    assert!(matches!(entry.state(), ClaimState::Confirmed { .. }));
    let refused = entry
        .append(
            machine(4),
            stepped(REWRITE, Outcome::Rewritten, node("s.final", READ)),
        )
        .unwrap_err();
    assert_eq!(
        refused,
        LedgerError::NotAllowed {
            claim: "c1".into(),
            kind: "stepped",
            state: "confirmed",
        }
    );

    // Undoing the confirmation gives back the claim the decision left.
    entry
        .append(user(5), ClaimEvent::Undone { event_ordinal: 2 })
        .unwrap();
    assert!(matches!(entry.state(), ClaimState::Open { .. }));
    assert_eq!(entry.decided(), Some(&node("s.rewrite", REWRITE)));

    for (action, state) in [
        (UserAction::Remove, "removed"),
        (
            UserAction::Edit {
                before: "a".into(),
                after: "b".into(),
            },
            "edited",
        ),
    ] {
        let mut entry = decided_open("c2");
        entry.append(user(3), act(action, None)).unwrap();
        let refused = entry
            .append(
                machine(4),
                stepped(REWRITE, Outcome::Rewritten, node("s.final", READ)),
            )
            .unwrap_err();
        assert_eq!(
            refused,
            LedgerError::NotAllowed {
                claim: "c2".into(),
                kind: "stepped",
                state,
            }
        );
    }
}

#[test]
fn a_decision_belongs_to_the_machine() {
    let mut entry = claim("c1");
    assert_eq!(
        entry
            .append(
                user(2),
                stepped(READ, Outcome::Unproven, node("s.research", Step::Research))
            )
            .unwrap_err(),
        LedgerError::WrongActor {
            claim: "c1".into(),
            kind: "stepped",
            actor: "user",
        }
    );
}

#[test]
fn a_decision_under_another_policy_never_continues_a_claim() {
    let mut entry = decided_open("c1");
    let other = stepped_by(
        "acceptance-v2",
        REWRITE,
        Outcome::Rewritten,
        node("s.final", READ),
        "Because.",
        false,
    );
    assert_eq!(
        entry.append(machine(3), other).unwrap_err(),
        LedgerError::PolicyChanged {
            claim: "c1".into(),
            held: "acceptance-v1".into(),
            recorded: "acceptance-v2".into(),
        }
    );
    assert_eq!(entry.events().len(), 2);
    // The first decision of a claim names the policy it is under.
    let mut fresh = claim("c2");
    fresh
        .append(
            machine(2),
            stepped_by(
                "acceptance-v2",
                READ,
                Outcome::Unproven,
                node("s.research", Step::Research),
                "Because.",
                true,
            ),
        )
        .unwrap();
}

#[test]
fn a_repair_of_a_decided_claim_is_what_happened_and_is_not_counted() {
    let mut entry = decided_open("c1");
    let open = entry.state().clone();
    // The claim has one attempt left, so the old rules would want the second,
    // escalated one here.
    entry
        .append(
            machine(3),
            ClaimEvent::RepairAttempted {
                attempt: 1,
                escalated: false,
                before: QUOTE.into(),
                after: Some("Water boils at 100 °C.".into()),
                outcome: RepairOutcome::Rewritten,
                goal: Some(Goal::WhatThePassageStates),
            },
        )
        .unwrap();
    assert_eq!(entry.state(), &open);
    entry
        .append(
            machine(4),
            stepped(REWRITE, Outcome::Rewritten, node("s.final", READ)),
        )
        .unwrap();
    assert_eq!(entry.state(), &ClaimState::Unchecked);

    // A claim waiting for the user takes none.
    let mut waiting = queued_claim("c2", VerdictKind::Contradicted);
    waiting
        .append(
            machine(7),
            stepped(READ, Outcome::Refuted, node("s.rewrite", REWRITE)),
        )
        .unwrap();
    assert!(waiting
        .append(machine(8), rewrite(1, false, RepairOutcome::Rewritten))
        .is_err());
}

#[test]
fn a_dropped_citation_charges_nothing_and_awaits_its_check() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    let open = entry.state().clone();
    let dropped = || ClaimEvent::CitationDropped {
        cited: vec!["v1".into()],
    };
    assert_eq!(dropped().kind_name(), "citation_dropped");
    entry.append(machine(3), dropped()).unwrap();
    assert_eq!(entry.state(), &open);
    assert!(entry.citation_awaits_check());

    // The check that reads it charges no attempt either.
    entry
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert!(!entry.citation_awaits_check());
    assert!(matches!(
        entry.state(),
        ClaimState::Open {
            machine_attempts_left: 2,
            ..
        }
    ));
    assert_eq!(
        entry.append(user(5), dropped()).unwrap_err(),
        LedgerError::WrongActor {
            claim: "c1".into(),
            kind: "citation_dropped",
            actor: "user",
        }
    );
}

#[test]
fn a_repair_written_without_a_goal_still_reads() {
    let stored = json!({
        "kind": "repair_attempted", "attempt": 1, "escalated": false,
        "before": "a", "after": null, "outcome": "rewritten"
    });
    let event: ClaimEvent = serde_json::from_value(stored.clone()).unwrap();
    let without = ClaimEvent::RepairAttempted {
        attempt: 1,
        escalated: false,
        before: "a".into(),
        after: None,
        outcome: RepairOutcome::Rewritten,
        goal: None,
    };
    assert_eq!(event, without);
    assert_eq!(serde_json::to_value(&event).unwrap(), stored);

    let with_goal: ClaimEvent = serde_json::from_value(json!({
        "kind": "repair_attempted", "attempt": 1, "escalated": false,
        "before": "a", "after": null, "outcome": "rewritten",
        "goal": "agree_with_the_passages"
    }))
    .unwrap();
    assert!(matches!(
        with_goal,
        ClaimEvent::RepairAttempted {
            goal: Some(Goal::AgreeWithThePassages),
            ..
        }
    ));
}

#[test]
fn what_a_check_found_decides_a_class_over_the_writers() {
    assert!(ClassifiedBy::Evidence > ClassifiedBy::Writer);
    let mut entry = claim("c1");
    entry
        .append(machine(2), classified(Class::Sourced, ClassifiedBy::Writer))
        .unwrap();
    entry
        .append(
            machine(3),
            classified(Class::Common, ClassifiedBy::Evidence),
        )
        .unwrap();
    assert_eq!(
        entry.classified(),
        Some((Class::Common, ClassifiedBy::Evidence))
    );
    // The writer's authority does not take it back.
    entry
        .append(machine(4), classified(Class::Sourced, ClassifiedBy::Writer))
        .unwrap();
    assert_eq!(entry.class(), Some(Class::Common));
}

#[test]
fn what_a_check_found_moves_a_class_in_either_direction_and_its_latest_move_holds() {
    let mut entry = claim("c1");
    entry
        .append(
            machine(2),
            classified(Class::Reasoning, ClassifiedBy::Model),
        )
        .unwrap();
    entry
        .append(
            machine(3),
            classified(Class::Sourced, ClassifiedBy::Evidence),
        )
        .unwrap();
    assert_eq!(
        entry.classified(),
        Some((Class::Sourced, ClassifiedBy::Evidence))
    );
    // The same authority, a lighter class: the check read the rewrite.
    entry
        .append(
            machine(4),
            classified(Class::Common, ClassifiedBy::Evidence),
        )
        .unwrap();
    assert_eq!(
        entry.classified(),
        Some((Class::Common, ClassifiedBy::Evidence))
    );
    assert_eq!(entry.class(), Some(Class::Common));

    // The log replays to the class its latest move gave.
    let replayed =
        ClaimLedgerEntry::from_events(entry.claim_id().clone(), entry.events().to_vec()).unwrap();
    assert_eq!(replayed.class(), Some(Class::Common));
    assert_eq!(replayed, entry);
}

#[test]
fn the_decisions_and_the_steps_of_a_claim_are_read_from_its_record() {
    let mut entry = claim("c1");
    assert_eq!(entry.decided(), None);
    assert_eq!(entry.steps(), Vec::<(Taken, Then)>::new());
    entry
        .append(
            machine(2),
            stepped(READ, Outcome::Unproven, node("s.research", Step::Research)),
        )
        .unwrap();
    entry.append(machine(3), checked(None, "v1")).unwrap();
    entry
        .append(
            machine(4),
            stepped(
                Step::Research,
                Outcome::Saved,
                node("s.reread", Step::Read(Reading::Page)),
            ),
        )
        .unwrap();
    assert_eq!(
        entry.steps(),
        vec![
            (
                Taken {
                    step: READ,
                    outcome: Outcome::Unproven
                },
                node("s.research", Step::Research)
            ),
            (
                Taken {
                    step: Step::Research,
                    outcome: Outcome::Saved
                },
                node("s.reread", Step::Read(Reading::Page))
            ),
        ]
    );
    assert_eq!(
        entry.decided(),
        Some(&node("s.reread", Step::Read(Reading::Page)))
    );
}

#[test]
fn a_claim_follows_the_policy_step_by_step_and_its_record_walks() {
    let policy = StandardPolicy::new();
    let start = Position {
        class: Class::Common,
        changed_class: false,
        reasked: false,
        node: policy.graph(Class::Common).entry(),
    };
    let rewrite_step = Step::Rewrite(Goal::AsTheFieldHasIt);
    let path = [
        (
            Step::Read(Reading::Knowledge),
            Outcome::Refuted,
            ClaimState::Unchecked,
        ),
        (
            Step::Read(Reading::Ground),
            Outcome::Refuted,
            ClaimState::Open {
                kind: VerdictKind::Contradicted,
                reason: "Because.".into(),
                machine_attempts_left: 1,
            },
        ),
        (rewrite_step, Outcome::Rewritten, ClaimState::Unchecked),
        (
            Step::Read(Reading::Knowledge),
            Outcome::Stands,
            ClaimState::GeneralKnowledge { at: 5 },
        ),
    ];
    let mut entry = claim("c1");
    let mut at = start;
    for (index, (step, outcome, state)) in path.into_iter().enumerate() {
        let taken = Taken { step, outcome };
        let (then, next) = decide(&policy, at, taken).unwrap();
        let ahead = next.is_some_and(|next| rewrite_ahead(&policy, next));
        entry
            .append(
                machine(2 + i64::try_from(index).unwrap()),
                stepped_by(policy.version(), step, outcome, then, "Because.", ahead),
            )
            .unwrap();
        assert_eq!(entry.state(), &state, "after step {index}");
        if let Some(next) = next {
            at = next;
        }
    }
    assert_eq!(check_record(&policy, start, &entry.steps()), Ok(()));
    assert_eq!(entry.decided(), Some(&Then::End(End::GeneralKnowledge)));

    // Stored and read back, the record replays to the same claim.
    let stored = serde_json::to_string(entry.events()).unwrap();
    let read: Vec<RecordedEvent> = serde_json::from_str(&stored).unwrap();
    let replayed = ClaimLedgerEntry::from_events(ClaimId("c1".into()), read).unwrap();
    assert_eq!(replayed, entry);
}

#[test]
fn the_stored_forms_of_a_decision_are_pinned() {
    fn pin<T>(value: &T, stored: &str)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        assert_eq!(serde_json::to_string(value).unwrap(), stored);
        assert_eq!(&serde_json::from_str::<T>(stored).unwrap(), value);
    }
    pin(&Step::Read(Reading::Parts), r#"{"read":"parts"}"#);
    pin(&Step::Read(Reading::Ground), r#"{"read":"ground"}"#);
    pin(&Step::Research, r#""research""#);
    pin(
        &Step::Rewrite(Goal::WhatThePassageStates),
        r#"{"rewrite":"what_the_passage_states"}"#,
    );
    pin(&Step::Cut, r#""cut""#);
    pin(&End::Resolved, r#""resolved""#);
    pin(&End::GeneralKnowledge, r#""general_knowledge""#);
    pin(&End::NotAClaim, r#""not_a_claim""#);
    pin(
        &End::Removed { section_goes: true },
        r#"{"removed":{"section_goes":true}}"#,
    );
    pin(
        &node("s.rewrite", REWRITE),
        r#"{"node":{"name":"s.rewrite","step":{"rewrite":"what_the_passage_states"}}}"#,
    );
    pin(&Then::End(End::Consistent), r#"{"end":"consistent"}"#);
    pin(&Then::Stop, r#""stop""#);
    pin(
        &stepped_with(
            READ,
            Outcome::Unproven,
            node("s.research", Step::Research),
            "No passage states it.",
            true,
        ),
        concat!(
            r#"{"kind":"stepped","policy":"acceptance-v1","step":{"read":"parts"},"#,
            r#""outcome":"unproven","then":{"node":{"name":"s.research","step":"research"}},"#,
            r#""reason":"No passage states it.","rewrite_ahead":true}"#
        ),
    );
    pin(
        &ClaimEvent::CitationDropped {
            cited: vec!["v1".into()],
        },
        r#"{"kind":"citation_dropped","cited":["v1"]}"#,
    );
    pin(
        &ClaimEvent::RepairAttempted {
            attempt: 1,
            escalated: false,
            before: "a".into(),
            after: None,
            outcome: RepairOutcome::Rewritten,
            goal: Some(Goal::AsTheFieldHasIt),
        },
        concat!(
            r#"{"kind":"repair_attempted","attempt":1,"escalated":false,"before":"a","#,
            r#""after":null,"outcome":"rewritten","goal":"as_the_field_has_it"}"#
        ),
    );
    pin(
        &classified(Class::Common, ClassifiedBy::Evidence),
        r#"{"kind":"classified","class":"common","by":"evidence"}"#,
    );
}
