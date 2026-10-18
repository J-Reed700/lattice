//! The ledger's invariants: append-only verdicts, the attempt budget, undo,
//! the user's verdicts against the machine and the Verifier.

use super::*;

fn open(kind: VerdictKind, attempts: u8) -> ClaimState {
    ClaimState::Open {
        kind,
        reason: "compared v1".into(),
        machine_attempts_left: attempts,
    }
}

#[test]
fn a_claim_that_fails_twice_on_the_same_evidence_is_queued_after_two_repairs() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    assert_eq!(entry.state(), &open(VerdictKind::Contradicted, 2));
    assert!(entry.may_repair() && !entry.next_repair_escalates());

    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    assert!(!entry.may_repair(), "a rewrite waits for its check");
    entry
        .append(machine(4), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    assert_eq!(entry.state(), &open(VerdictKind::Contradicted, 1));
    assert!(entry.next_repair_escalates());

    entry
        .append(machine(5), rewrite(2, true, RepairOutcome::Rewritten))
        .unwrap();
    entry
        .append(machine(6), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Queued {
            kind: VerdictKind::Unsupported,
            reason: "compared v1".into()
        }
    );
    assert!(!entry.may_repair() && !entry.may_research());
    let repairs = entry
        .events()
        .iter()
        .filter(|e| matches!(e.event, ClaimEvent::RepairAttempted { .. }))
        .count();
    assert_eq!(repairs, 2);
}

#[test]
fn new_evidence_restores_the_budget_and_the_same_evidence_does_not() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    entry
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "compared v2".into(),
            machine_attempts_left: MACHINE_ATTEMPTS
        }
    );
    // A recheck on unchanged evidence with no repair in between costs nothing.
    entry
        .append(machine(5), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Open {
            machine_attempts_left: MACHINE_ATTEMPTS,
            ..
        }
    ));
}

#[test]
fn a_recheck_on_new_evidence_verifies_a_queued_claim_or_leaves_it_with_the_user() {
    let mut entry = queued_claim("c1", VerdictKind::Unsupported);
    entry
        .append(machine(7), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Queued {
            kind: VerdictKind::Contradicted,
            reason: "compared v2".into()
        }
    );
    assert!(!entry.may_repair());
    entry.append(machine(8), checked(None, "v3")).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
}

#[test]
fn a_rejected_repair_uses_an_attempt_without_a_check() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    let rejected = || RepairOutcome::Rejected {
        reason: "the rewrite dropped a citation".into(),
    };
    entry
        .append(machine(3), rewrite(1, false, rejected()))
        .unwrap();
    assert_eq!(entry.state(), &open(VerdictKind::Unsupported, 1));
    entry
        .append(machine(4), rewrite(2, true, rejected()))
        .unwrap();
    assert!(matches!(entry.state(), ClaimState::Queued { .. }));
}

#[test]
fn repairs_the_rules_do_not_allow_are_refused() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    assert!(matches!(
        entry.append(machine(3), rewrite(1, true, RepairOutcome::Rewritten)),
        Err(LedgerError::RepairMismatch { .. })
    ));
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    assert!(matches!(
        entry.append(machine(4), rewrite(2, true, RepairOutcome::Rewritten)),
        Err(LedgerError::NotAllowed { .. })
    ));
    // A repair the user's edit superseded is recorded and not charged.
    entry
        .append(
            machine(4),
            rewrite(2, true, RepairOutcome::SupersededByUser),
        )
        .unwrap();
    let mut no_source = claim("c2");
    no_source
        .append(machine(2), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    assert!(!no_source.may_repair());
    assert!(no_source
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .is_err());
}

#[test]
fn kinds_the_machine_cannot_repair_research_once_or_go_to_the_user() {
    for kind in [
        VerdictKind::CouldNotJudge,
        VerdictKind::GeneralKnowledgeProposed,
    ] {
        let mut entry = claim("c1");
        entry.append(machine(2), checked(Some(kind), "v1")).unwrap();
        assert!(entry.state().awaits_user(), "{:?}", entry.state());
    }

    let mut entry = claim("c2");
    entry
        .append(machine(2), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    assert!(entry.may_research());
    let research = |found| ClaimEvent::Researched {
        queries: vec!["boiling point sea level".into()],
        sources_added: vec!["v2".into()],
        passages_found: found,
    };
    entry.append(machine(3), research(1)).unwrap();
    assert!(!entry.may_research(), "once per fingerprint");
    entry
        .append(machine(4), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    assert!(entry.state().awaits_user(), "same evidence after research");

    let mut entry = claim("c3");
    entry
        .append(machine(2), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    entry.append(machine(3), research(0)).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Queued {
            kind: VerdictKind::NoSource,
            ..
        }
    ));
}

#[test]
fn the_log_is_append_only_and_replays_to_the_same_state() {
    let entry = queued_claim("c1", VerdictKind::Unsupported);
    let ordinals: Vec<u32> = entry.events().iter().map(|e| e.ordinal).collect();
    assert_eq!(ordinals, (0..6).collect::<Vec<_>>());
    let replayed =
        ClaimLedgerEntry::from_events(entry.claim_id().clone(), entry.events().to_vec()).unwrap();
    assert_eq!(replayed, entry);

    let mut out_of_order = entry.events().to_vec();
    out_of_order.swap(2, 3);
    assert!(matches!(
        ClaimLedgerEntry::from_events(ClaimId("c1".into()), out_of_order),
        Err(LedgerError::OutOfOrder { .. })
    ));
    let headless = entry.events().get(1..).unwrap().to_vec();
    assert!(matches!(
        ClaimLedgerEntry::from_events(ClaimId("c1".into()), headless),
        Err(LedgerError::NotExtractedFirst { .. })
    ));
    let mut entry = entry;
    assert!(matches!(
        entry.append(machine(7), act(confirm(), None)),
        Err(LedgerError::WrongActor { .. })
    ));
    assert!(matches!(
        entry.append(user(7), checked(None, "v1")),
        Err(LedgerError::WrongActor { .. })
    ));
}

#[test]
fn a_verdict_is_added_never_rewritten() {
    let mut entry = queued_claim("c1", VerdictKind::Unsupported);
    let before = entry.events().to_vec();
    entry.append(user(7), act(confirm(), None)).unwrap();
    assert_eq!(entry.events().get(..before.len()).unwrap(), before);
    entry
        .append(user(8), act(UserAction::Unconfirm, None))
        .unwrap();
    entry
        .append(user(9), act(confirm(), Some("Checked the CRC handbook.")))
        .unwrap();
    // Every verdict stays in the history, the note included.
    let notes: Vec<_> = entry
        .events()
        .iter()
        .filter_map(|e| match &e.event {
            ClaimEvent::UserAction { note, .. } => Some(note.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        notes,
        vec![None, None, Some("Checked the CRC handbook.".into())]
    );
}

#[test]
fn the_machine_never_overrides_the_users_verdict() {
    let mut entry = queued_claim("c1", VerdictKind::Unsupported);
    entry.append(user(7), act(confirm(), None)).unwrap();
    let confirmed = entry.state().clone();
    entry
        .append(machine(8), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    entry.append(machine(9), checked(None, "v3")).unwrap();
    assert_eq!(entry.state(), &confirmed);
    assert!(entry
        .append(machine(10), rewrite(1, false, RepairOutcome::Rewritten))
        .is_err());
}

#[test]
fn confirming_a_contradiction_needs_a_note() {
    let mut entry = queued_claim("c1", VerdictKind::Contradicted);
    assert!(entry.confirm_requires_note());
    assert!(matches!(
        entry.append(user(7), act(confirm(), Some("  "))),
        Err(LedgerError::NoteRequired { .. })
    ));
    entry
        .append(user(7), act(confirm(), Some("The source is outdated.")))
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Confirmed { note: Some(note), .. } if note == "The source is outdated."
    ));
}

#[test]
fn undo_restores_the_state_before_the_last_user_event_and_undo_again_redoes_it() {
    let mut entry = queued_claim("c1", VerdictKind::Unsupported);
    let queued = entry.state().clone();
    let confirm_at = entry.append(user(7), act(confirm(), None)).unwrap();
    let confirmed = entry.state().clone();
    assert_eq!(entry.undo_target(), Some(confirm_at));

    let undo_at = entry
        .append(
            user(8),
            ClaimEvent::Undone {
                event_ordinal: confirm_at,
            },
        )
        .unwrap();
    assert_eq!(entry.state(), &queued);
    entry
        .append(
            user(9),
            ClaimEvent::Undone {
                event_ordinal: undo_at,
            },
        )
        .unwrap();
    assert_eq!(entry.state(), &confirmed);

    // Only the last user event can be undone.
    assert!(matches!(
        entry.append(
            user(10),
            ClaimEvent::Undone {
                event_ordinal: confirm_at
            }
        ),
        Err(LedgerError::NotUndoable { .. })
    ));
    let replayed =
        ClaimLedgerEntry::from_events(entry.claim_id().clone(), entry.events().to_vec()).unwrap();
    assert_eq!(replayed.state(), &confirmed);
}

#[test]
fn undoing_a_machine_removal_hands_the_claim_to_the_user() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Removed))
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    let removal = entry.undo_target().unwrap();
    entry
        .append(
            user(4),
            ClaimEvent::Undone {
                event_ordinal: removal,
            },
        )
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Queued {
            kind: VerdictKind::Contradicted,
            reason: "compared v1".into()
        }
    );
}

#[test]
fn an_edit_retires_the_claim_and_names_its_successor() {
    let mut entry = queued_claim("c1", VerdictKind::Unsupported);
    let edit = UserAction::Edit {
        before: QUOTE.into(),
        after: "Water boils at 100 °C at one atmosphere.".into(),
    };
    entry.append(user(7), act(edit, None)).unwrap();
    entry
        .append(
            machine(8),
            ClaimEvent::Superseded {
                successor: Some(ClaimId("c2".into())),
            },
        )
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Edited {
            at: 7,
            successor: Some(ClaimId("c2".into()))
        }
    );
    assert!(entry.state().is_resolved());
    assert!(!entry.allows(ClaimActionKind::Edit));
    assert!(entry
        .append(
            machine(9),
            ClaimEvent::Extracted {
                location: location(QUOTE),
                statement: "x".into()
            }
        )
        .is_err());
}

#[test]
fn user_driven_research_and_evidence_never_lead_to_an_automatic_repair() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    entry
        .append(
            user(3),
            act(
                UserAction::ResearchWithHint {
                    hint: "NIST steam tables".into(),
                },
                None,
            ),
        )
        .unwrap();
    assert!(entry.state().awaits_user());
    entry
        .append(machine(4), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Queued { kind: VerdictKind::Contradicted, reason } if reason == "compared v2"
    ));
    assert!(!entry.may_repair());

    let attach = |result| ClaimEvent::EvidenceAttached {
        source_version_id: "v9".into(),
        provenance: Provenance::PastedText,
        result,
    };
    entry
        .append(user(5), attach(AttachOutcome::NoPassage))
        .unwrap();
    assert!(entry.state().awaits_user());
    entry
        .append(
            user(6),
            attach(AttachOutcome::Checked(local_check(None, "v9"))),
        )
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { source_version_id, .. } if source_version_id == "v9"
    ));
}

#[test]
fn the_verifier_queues_a_machine_verdict_and_reopens_a_human_one() {
    let mut entry = verified_claim("c1");
    entry.append(machine(3), verifier(None)).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    entry
        .append(machine(4), verifier(Some(VerdictKind::CouldNotJudge)))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Queued {
            kind: VerdictKind::Disputed {
                verifier: Box::new(VerdictKind::CouldNotJudge)
            },
            reason: "compared v1".into()
        }
    );

    let mut entry = queued_claim("c2", VerdictKind::Unsupported);
    entry
        .append(user(7), act(confirm(), Some("Seen in the lab.")))
        .unwrap();
    let confirmed = entry.state().clone();
    entry
        .append(machine(8), verifier(Some(VerdictKind::CouldNotJudge)))
        .unwrap();
    assert_eq!(entry.state(), &confirmed, "could-not-verify is no dispute");
    entry
        .append(machine(9), verifier(Some(VerdictKind::Contradicted)))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Reopened {
            cause: ReopenCause::VerifierDispute,
            previous: Box::new(confirmed)
        }
    );
    assert!(entry.confirm_requires_note(), "the Verifier contradicts it");
}

#[test]
fn reopening_applies_only_to_closed_claims_and_unconfirm_reopens() {
    let mut entry = queued_claim("c1", VerdictKind::Unsupported);
    let reopen = ClaimEvent::Reopened {
        cause: ReopenCause::SourceDeleted("v1".into()),
    };
    assert!(entry.append(machine(7), reopen.clone()).is_err());
    entry.append(user(7), act(confirm(), None)).unwrap();
    let confirmed = entry.state().clone();
    entry
        .append(user(8), act(UserAction::Unconfirm, None))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Reopened {
            cause: ReopenCause::Unconfirmed,
            previous: Box::new(confirmed)
        }
    );

    let mut entry = verified_claim("c2");
    entry.append(machine(3), reopen).unwrap();
    assert!(entry.state().awaits_user());
}

#[test]
fn the_card_offers_only_what_the_state_allows() {
    let contradicted = queued_claim("c1", VerdictKind::Contradicted);
    assert_eq!(
        contradicted.available_actions(),
        vec![
            ClaimActionKind::Confirm,
            ClaimActionKind::Edit,
            ClaimActionKind::Remove,
            ClaimActionKind::AttachEvidence,
            ClaimActionKind::ResearchWithHint,
        ]
    );
    let mut unsupported = queued_claim("c2", VerdictKind::Unsupported);
    assert!(unsupported.allows(ClaimActionKind::GeneralKnowledge));
    unsupported
        .append(user(7), act(UserAction::GeneralKnowledge, None))
        .unwrap();
    assert_eq!(
        unsupported.available_actions(),
        vec![
            ClaimActionKind::Edit,
            ClaimActionKind::Remove,
            ClaimActionKind::Undo
        ]
    );
    let mut fresh = claim("c3");
    assert!(fresh
        .append(user(2), act(UserAction::GeneralKnowledge, None))
        .is_err());
    assert!(fresh.append(user(2), act(confirm(), None)).is_err());
}
