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
fn a_claims_attempts_never_rise_whatever_evidence_a_later_check_brings() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    // The rewrite costs its attempt whatever passages the rewritten sentence
    // retrieved.
    entry
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "compared v2".into(),
            machine_attempts_left: MACHINE_ATTEMPTS - 1
        }
    );
    // Research found other passages: the verdict may change, the count never
    // goes back up (claim classes design §3.2).
    entry
        .append(machine(5), checked(Some(VerdictKind::Unsupported), "v3"))
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "compared v3".into(),
            machine_attempts_left: MACHINE_ATTEMPTS - 1
        }
    );
    // A recheck on unchanged evidence with no repair in between costs nothing.
    entry
        .append(machine(6), checked(Some(VerdictKind::Unsupported), "v3"))
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Open {
            machine_attempts_left: 1,
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
}

#[test]
fn a_repair_rejected_uses_its_attempt_and_the_last_one_takes_the_claim_out() {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    entry
        .append(
            machine(3),
            rewrite(
                1,
                false,
                RepairOutcome::Rejected {
                    reason: "the answer could not be used".into(),
                },
            ),
        )
        .unwrap();
    assert_eq!(entry.state(), &open(VerdictKind::Unsupported, 1));
    assert!(entry.may_repair() && entry.next_repair_escalates());
    entry
        .append(machine(4), rewrite(2, true, RepairOutcome::Removed))
        .unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
}

#[test]
fn every_failure_the_checker_gives_stays_with_the_machine_which_researches_once_rewrites_and_cuts()
{
    // No passage on the statement's terms is a failure like any other: no
    // source supports it.
    let mut entry = claim("c2");
    entry
        .append(machine(2), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    assert_eq!(entry.state(), &open(VerdictKind::NoSource, 2));
    assert!(entry.may_repair());

    // Research runs once per claim; a round that finds nothing leaves the
    // claim to the machine, which rewrites its sentence and then takes it out.
    let research = |found| ClaimEvent::Researched {
        queries: vec!["boiling point sea level".into()],
        sources_added: vec!["v2".into()],
        passages_found: found,
        round_ms_per_claim: None,
    };
    let mut entry = claim("c3");
    entry
        .append(machine(2), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    assert!(entry.may_research());
    entry.append(machine(3), research(1)).unwrap();
    assert!(!entry.may_research(), "once per claim");
    entry
        .append(machine(4), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    assert_eq!(
        entry.state(),
        &open(VerdictKind::NoSource, 2),
        "same evidence after research"
    );
    assert!(entry.may_repair());

    let mut entry = claim("c4");
    entry
        .append(machine(2), checked(Some(VerdictKind::NoSource), "v1"))
        .unwrap();
    entry.append(machine(3), research(0)).unwrap();
    assert_eq!(entry.state(), &open(VerdictKind::NoSource, 2));
    assert!(entry.may_repair() && !entry.may_research());
}

#[test]
fn evidence_a_round_saved_never_earns_a_claim_another_round() {
    use crate::domain::provenance::{Bar, ClusterId, CountedSupport, TrustLevel};
    let research = || ClaimEvent::Researched {
        queries: vec!["boiling point sea level".into()],
        sources_added: vec!["v2".into()],
        passages_found: 1,
        round_ms_per_claim: None,
    };
    let counted = |version: &str, cluster: &str, level| CountedSupport {
        source_version_id: version.into(),
        origin_cluster: ClusterId(cluster.into()),
        counted: Some(level),
    };
    let short = |have: Vec<CountedSupport>| {
        Some(VerdictKind::InsufficientStanding {
            bar: Bar::Primary,
            have,
        })
    };
    let blog = || counted("v1", "v1", TrustLevel::Established);

    // Whatever the next check finds, on other evidence, in a stronger or an
    // independent source, the claim's one round is spent and its attempts do
    // not rise.
    for (id, kind) in [
        ("c1", VerdictKind::Unsupported),
        ("c2", VerdictKind::NoSource),
    ] {
        let mut entry = claim(id);
        entry
            .append(machine(2), checked(Some(kind.clone()), "v1"))
            .unwrap();
        entry.append(machine(3), research()).unwrap();
        entry.append(machine(4), checked(Some(kind), "v2")).unwrap();
        assert!(!entry.may_research(), "{id}: other evidence is not a round");
        assert!(matches!(
            entry.state(),
            ClaimState::Open {
                machine_attempts_left: MACHINE_ATTEMPTS,
                ..
            }
        ));
    }
    for (id, gained) in [
        ("c3", counted("v2", "v2", TrustLevel::Established)),
        ("c4", counted("v2", "v1", TrustLevel::Authoritative)),
    ] {
        let mut entry = claim(id);
        entry
            .append(machine(2), checked(short(vec![blog()]), "v1"))
            .unwrap();
        entry.append(machine(3), research()).unwrap();
        entry
            .append(machine(4), checked(short(vec![blog(), gained]), "v2"))
            .unwrap();
        assert!(
            !entry.may_research(),
            "{id}: a stronger or an independent source is not a round"
        );
    }
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
                by: Actor::Machine,
            },
        )
        .unwrap();
    assert_eq!(
        entry.state(),
        &ClaimState::Edited {
            at: 7,
            successor: Some(ClaimId("c2".into())),
            by: Actor::User,
        },
        "the user's edit says who changed the text"
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
    unsupported
        .append(user(7), act(confirm(), Some("Checked by hand.")))
        .unwrap();
    assert_eq!(
        unsupported.available_actions(),
        vec![
            ClaimActionKind::Edit,
            ClaimActionKind::Remove,
            ClaimActionKind::AttachEvidence,
            ClaimActionKind::Unconfirm,
            ClaimActionKind::Undo
        ]
    );
    let mut fresh = claim("c3");
    assert!(fresh.append(user(2), act(confirm(), None)).is_err());
}

#[test]
fn an_insufficient_standing_claim_is_never_a_repair_target_and_waits_after_research() {
    use crate::domain::provenance::{Bar, ClusterId, CountedSupport, TrustLevel};
    let short = Some(VerdictKind::InsufficientStanding {
        bar: Bar::Primary,
        have: vec![CountedSupport {
            source_version_id: "v1".into(),
            origin_cluster: ClusterId("v1".into()),
            counted: Some(TrustLevel::Established),
        }],
    });
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(short.clone(), "v1"))
        .unwrap();
    assert!(!entry.may_repair(), "no rewrite for lack of standing");
    assert!(entry.may_research(), "research may find a stronger source");
    entry
        .append(
            machine(3),
            ClaimEvent::Researched {
                queries: vec!["q".into()],
                sources_added: vec![],
                passages_found: 0,
                round_ms_per_claim: None,
            },
        )
        .unwrap();
    assert!(
        entry.held_for_standing(),
        "research found nothing: it waits for the user ({:?})",
        entry.state()
    );
    assert!(!entry.may_repair());
    assert_eq!(ClaimCounts::tally([&entry]).queued, 1);
    assert_eq!(end_state([&entry]), LessonReviewState::NeedsReview);

    // A repair recorded before standing stopped being repaired still
    // replays as it was: stored ledgers load.
    let mut stored = claim("c2");
    stored.append(machine(2), checked(short, "v1")).unwrap();
    assert!(stored
        .append(machine(3), rewrite(1, false, RepairOutcome::Removed))
        .is_ok());
}

#[test]
fn a_claim_born_of_a_repair_starts_with_its_parents_remaining_attempts() {
    let born = |attempts_left: u8| {
        let mut entry = claim("child");
        entry
            .append(
                machine(2),
                ClaimEvent::BornOfRepair {
                    parent: ClaimId("parent".into()),
                    attempts_left,
                },
            )
            .unwrap();
        entry
            .append(machine(3), checked(Some(VerdictKind::Unsupported), "v1"))
            .unwrap();
        entry
    };
    assert!(matches!(
        born(1).state(),
        ClaimState::Open {
            machine_attempts_left: 1,
            ..
        }
    ));
    assert!(
        born(0).state().awaits_user(),
        "a split of a claim with nothing left adds no attempt"
    );
    let mut checked_first = verified_claim("c2");
    assert!(checked_first
        .append(
            machine(3),
            ClaimEvent::BornOfRepair {
                parent: ClaimId("parent".into()),
                attempts_left: 1,
            },
        )
        .is_err());
}

/// Events as code before sentence locations wrote them: the claim located
/// by its whole passage, and no repair outcome but the four it knew.
const PASSAGE_HISTORIES: &str = r#"{
  "c1": [
    {"ordinal": 0, "provenance": {"actor": "machine", "at": 1, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "extracted", "location": {"unit_index": 0, "unit_kind": "block", "field": "body", "quote": "Heat it. Water boils at 100 °C at sea level. Then cool it.", "span": [0, 59]}, "statement": "Water boils at 100 °C at sea level."}},
    {"ordinal": 1, "provenance": {"actor": "machine", "at": 2, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "checked", "checker": {"local": {"model": "qwen3.5-27b"}}, "result": {"failed": {"kind": "unsupported"}}, "reason": "compared v1", "fingerprint": "fp-v1", "evidence": [{"source_version_id": "v1", "start_byte": 0, "end_byte": 40, "text_sha256": "sha-v1"}], "window_tokens": 32768}},
    {"ordinal": 2, "provenance": {"actor": "machine", "at": 3, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "repair_attempted", "attempt": 1, "escalated": false, "before": "Heat it. Water boils at 100 °C at sea level. Then cool it.", "after": "Heat it. Water boils at 100 °C. Then cool it.", "outcome": "rewritten"}},
    {"ordinal": 3, "provenance": {"actor": "machine", "at": 4, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "checked", "checker": {"local": {"model": "qwen3.5-27b"}}, "result": {"failed": {"kind": "unsupported"}}, "reason": "compared v1", "fingerprint": "fp-v1", "evidence": [{"source_version_id": "v1", "start_byte": 0, "end_byte": 40, "text_sha256": "sha-v1"}], "window_tokens": 32768}},
    {"ordinal": 4, "provenance": {"actor": "machine", "at": 5, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "repair_attempted", "attempt": 2, "escalated": true, "before": "Heat it. Water boils at 100 °C at sea level. Then cool it.", "after": null, "outcome": {"rejected": {"reason": "changed a protected sentence"}}}},
    {"ordinal": 5, "provenance": {"actor": "machine", "at": 6, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "checked", "checker": {"local": {"model": "qwen3.5-27b"}}, "result": {"failed": {"kind": "unsupported"}}, "reason": "compared v1", "fingerprint": "fp-v1", "evidence": [{"source_version_id": "v1", "start_byte": 0, "end_byte": 40, "text_sha256": "sha-v1"}], "window_tokens": 32768}}
  ],
  "c2": [
    {"ordinal": 0, "provenance": {"actor": "machine", "at": 1, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "extracted", "location": {"unit_index": 0, "unit_kind": "block", "field": "body", "quote": "Heat it. Water boils at 100 °C at sea level. Then cool it.", "span": [0, 59]}, "statement": "Heat it."}},
    {"ordinal": 1, "provenance": {"actor": "machine", "at": 2, "job_id": "job-1", "app_version": "1.0.0", "checker_model": "qwen3.5-27b"},
     "event": {"kind": "checked", "checker": {"local": {"model": "qwen3.5-27b"}}, "result": {"supported": {"source_version_id": "v1", "supporting_quote": "heat it"}}, "reason": "compared v1", "fingerprint": "fp-v1", "evidence": [{"source_version_id": "v1", "start_byte": 0, "end_byte": 40, "text_sha256": "sha-v1"}], "window_tokens": 32768}}
  ]
}"#;

#[test]
fn histories_written_before_sentence_locations_replay_unchanged() {
    let histories: std::collections::BTreeMap<String, Vec<RecordedEvent>> =
        serde_json::from_str(PASSAGE_HISTORIES).unwrap();
    let replay = |id: &str| {
        ClaimLedgerEntry::from_events(ClaimId(id.into()), histories[id].clone()).unwrap()
    };

    // Two failures on the same evidence, a rewrite and a rejected repair:
    // the user's, as before.
    let queued = replay("c1");
    assert_eq!(
        queued.state(),
        &ClaimState::Queued {
            kind: VerdictKind::Unsupported,
            reason: "compared v1".into(),
        }
    );
    assert!(!queued.may_repair());
    assert_eq!(queued.location().quote, FIELD_TEXT);
    assert_eq!(queued.location().span, Some((0, FIELD_TEXT.len())));

    let verified = replay("c2");
    assert_eq!(
        verified.state(),
        &ClaimState::ResolvedByEvidence {
            source_version_id: "v1".into(),
            supporting_quote: "heat it".into(),
        }
    );
    assert_eq!(verified.location().quote, FIELD_TEXT);

    // What replays is what was stored: written back, the events read the same.
    for id in ["c1", "c2"] {
        let entry = replay(id);
        let written = serde_json::to_string(entry.events()).unwrap();
        let read: Vec<RecordedEvent> = serde_json::from_str(&written).unwrap();
        assert_eq!(read, histories[id]);
    }
}

#[test]
fn a_claims_class_is_the_strongest_authoritys_whatever_the_order() {
    // The model only splits what the rule layer leaves, so its class never
    // lightens the rule layer's (claim classes design §3.1).
    let mut entry = claim("c1");
    assert_eq!(entry.class(), None);
    entry
        .append(
            machine(2),
            classified(Class::Reasoning, ClassifiedBy::Model),
        )
        .unwrap();
    assert_eq!(entry.class(), Some(Class::Reasoning));
    entry
        .append(machine(3), classified(Class::Sourced, ClassifiedBy::Rule))
        .unwrap();
    assert_eq!(
        entry.class(),
        Some(Class::Sourced),
        "a rule outranks the model"
    );
    assert_eq!(
        entry.classified(),
        Some((Class::Sourced, ClassifiedBy::Rule))
    );
    entry
        .append(machine(4), classified(Class::Common, ClassifiedBy::Model))
        .unwrap();
    assert_eq!(
        entry.class(),
        Some(Class::Sourced),
        "a model never lightens a rule"
    );

    // A sentence the writer cited is Sourced whatever else is said of it.
    let mut cited = claim("c3");
    cited
        .append(machine(2), classified(Class::Internal, ClassifiedBy::Rule))
        .unwrap();
    cited
        .append(machine(3), classified(Class::Sourced, ClassifiedBy::Writer))
        .unwrap();
    assert_eq!(
        cited.class(),
        Some(Class::Sourced),
        "the writer outranks a rule"
    );
    assert_eq!(
        cited.classified(),
        Some((Class::Sourced, ClassifiedBy::Writer))
    );
    cited
        .append(machine(4), classified(Class::Internal, ClassifiedBy::Rule))
        .unwrap();
    cited
        .append(
            machine(5),
            classified(Class::Reasoning, ClassifiedBy::Model),
        )
        .unwrap();
    assert_eq!(
        cited.class(),
        Some(Class::Sourced),
        "and nothing later lightens it"
    );

    // A judge reclassing what the model classed is the model deciding again.
    let mut reclassed = claim("c2");
    reclassed
        .append(machine(2), classified(Class::Common, ClassifiedBy::Model))
        .unwrap();
    reclassed
        .append(machine(3), classified(Class::Sourced, ClassifiedBy::Model))
        .unwrap();
    assert_eq!(reclassed.class(), Some(Class::Sourced));

    // Every classification stays in the log, and the log replays to the class.
    let classifications = |entry: &ClaimLedgerEntry| {
        entry
            .events()
            .iter()
            .filter(|e| e.event.kind_name() == "classified")
            .count()
    };
    assert_eq!(classifications(&entry), 3);
    let replayed =
        ClaimLedgerEntry::from_events(entry.claim_id().clone(), entry.events().to_vec()).unwrap();
    assert_eq!(replayed.class(), Some(Class::Sourced));
    assert_eq!(replayed, entry);
}

#[test]
fn the_same_authority_deciding_again_only_makes_a_class_stricter() {
    // A later pass that sorts a statement again is no reason to check it more
    // lightly (claim classes design §3.1).
    let mut entry = claim("c1");
    entry
        .append(machine(2), classified(Class::Sourced, ClassifiedBy::Model))
        .unwrap();
    entry
        .append(
            machine(3),
            classified(Class::Reasoning, ClassifiedBy::Model),
        )
        .unwrap();

    assert_eq!(
        entry.classified(),
        Some((Class::Sourced, ClassifiedBy::Model))
    );
    let replayed =
        ClaimLedgerEntry::from_events(entry.claim_id().clone(), entry.events().to_vec()).unwrap();
    assert_eq!(replayed.class(), Some(Class::Sourced));
}

#[test]
fn a_claim_that_was_verified_and_fails_again_resumes_from_what_it_had_left() {
    // One attempt spent on a rewrite, then verified: failing again it has the
    // one it had left, not a full budget (claim classes design §3.2).
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
    entry.append(machine(5), checked(None, "v3")).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    entry
        .append(machine(6), checked(Some(VerdictKind::Unsupported), "v4"))
        .unwrap();
    assert!(
        matches!(
            entry.state(),
            ClaimState::Open {
                machine_attempts_left: 1,
                ..
            }
        ),
        "{:?}",
        entry.state()
    );

    // With none left it waits for the user the moment it fails again.
    let mut spent = queued_claim("c2", VerdictKind::Unsupported);
    spent.append(machine(7), checked(None, "v2")).unwrap();
    assert!(matches!(
        spent.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    spent
        .append(machine(8), checked(Some(VerdictKind::Unsupported), "v3"))
        .unwrap();
    assert!(spent.state().awaits_user(), "{:?}", spent.state());

    // A claim born of a repair resumes from its parent's attempts.
    let mut child = claim("c3");
    child
        .append(
            machine(2),
            ClaimEvent::BornOfRepair {
                parent: ClaimId("c0".into()),
                attempts_left: 1,
            },
        )
        .unwrap();
    child.append(machine(3), checked(None, "v1")).unwrap();
    child
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert!(matches!(
        child.state(),
        ClaimState::Open {
            machine_attempts_left: 1,
            ..
        }
    ));
}

#[test]
fn a_reasoning_pass_ends_consistent_and_a_common_pass_ends_as_the_machines_general_knowledge() {
    let passed = |result: CheckResult, evidence: Vec<EvidenceSeen>| {
        ClaimEvent::Checked(Check {
            result,
            evidence,
            ..local_check(None, "v1")
        })
    };
    let mut reasoning = claim("c1");
    reasoning
        .append(
            machine(2),
            passed(CheckResult::Consistent, vec![seen("v1")]),
        )
        .unwrap();
    assert_eq!(reasoning.state(), &ClaimState::Consistent { at: 2 });
    assert_eq!(reasoning.latest_failure(), None);
    assert!(
        reasoning.allows(ClaimActionKind::Remove) && !reasoning.allows(ClaimActionKind::Confirm)
    );

    let mut common = claim("c2");
    common
        .append(machine(2), passed(CheckResult::KnownCommon, vec![]))
        .unwrap();
    assert_eq!(common.state(), &ClaimState::GeneralKnowledge { at: 2 });

    // Both passes resolve the lesson, and neither is the user's doing.
    let verified = verified_claim("c3");
    assert_eq!(
        end_state([&verified, &reasoning, &common]),
        LessonReviewState::Ready
    );
    let counts = ClaimCounts::tally([&verified, &reasoning, &common]);
    assert_eq!((counts.verified, counts.resolved_by_you()), (3, 0));

    // A later failing check demotes a consistent claim as it does a verified
    // one, with the attempts it has.
    reasoning
        .append(machine(3), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    assert!(matches!(
        reasoning.state(),
        ClaimState::Open {
            kind: VerdictKind::Contradicted,
            machine_attempts_left: MACHINE_ATTEMPTS,
            ..
        }
    ));
}

#[test]
fn a_passed_veto_that_no_decision_follows_ends_consistent_and_clears_the_failure() {
    let mut vetoed = claim("c1");
    vetoed
        .append(machine(2), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    assert_eq!(vetoed.latest_failure(), Some(&VerdictKind::Contradicted));
    vetoed
        .append(
            machine(3),
            ClaimEvent::Checked(Check {
                result: CheckResult::NotFalse,
                evidence: vec![],
                ..local_check(None, "v1")
            }),
        )
        .unwrap();
    assert_eq!(vetoed.state(), &ClaimState::Consistent { at: 3 });
    assert_eq!(vetoed.latest_failure(), None);
    assert_eq!(CheckResult::NotFalse.support(), None);
}

#[test]
fn a_claim_whose_sentence_goes_with_a_cut_is_removed_and_one_with_a_successor_is_edited() {
    let mut cut_with_another = verified_claim("c1");
    cut_with_another
        .append(
            machine(3),
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine,
            },
        )
        .unwrap();
    assert!(
        matches!(
            cut_with_another.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        cut_with_another.state()
    );
    assert!(cut_with_another.state().is_resolved());

    let mut continued = verified_claim("c2");
    continued
        .append(
            machine(3),
            ClaimEvent::Superseded {
                successor: Some(ClaimId("c3".into())),
                by: Actor::Machine,
            },
        )
        .unwrap();
    assert!(matches!(
        continued.state(),
        ClaimState::Edited {
            successor: Some(_),
            by: Actor::Machine,
            ..
        }
    ));
}

#[test]
fn a_passed_claim_on_a_rewritten_sentence_is_checked_again_with_the_lower_attempts() {
    let mut passed = verified_claim("c1");
    passed
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    assert!(
        matches!(passed.state(), ClaimState::ResolvedByEvidence { .. }),
        "the check moves it, not the rewrite"
    );
    assert!(!passed.may_repair(), "the rewrite waits for its check");
    passed
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert_eq!(
        passed.state(),
        &ClaimState::Open {
            kind: VerdictKind::Unsupported,
            reason: "compared v2".into(),
            machine_attempts_left: MACHINE_ATTEMPTS - 1,
        },
        "it resumes from the attempts the claim whose repair it was has left"
    );
    assert!(passed.next_repair_escalates());

    // A rewrite that still passes leaves it verified.
    let mut still = verified_claim("c2");
    still
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    still.append(machine(4), checked(None, "v2")).unwrap();
    assert!(matches!(
        still.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    // Only a rewrite rides on a sentence a claim shares.
    assert!(verified_claim("c3")
        .append(machine(3), rewrite(1, false, RepairOutcome::Removed))
        .is_err());
}

#[test]
fn a_common_pass_is_checked_again_when_its_sentence_is_rewritten() {
    let mut common = claim("c1");
    common
        .append(
            machine(2),
            ClaimEvent::Checked(Check {
                result: CheckResult::KnownCommon,
                evidence: vec![],
                ..local_check(None, "v1")
            }),
        )
        .unwrap();
    assert!(matches!(
        common.state(),
        ClaimState::GeneralKnowledge { .. }
    ));
    common
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    common
        .append(machine(4), checked(Some(VerdictKind::Contradicted), "v2"))
        .unwrap();
    assert!(
        matches!(
            common.state(),
            ClaimState::Open {
                kind: VerdictKind::Contradicted,
                machine_attempts_left: 1,
                ..
            }
        ),
        "{:?}",
        common.state()
    );
}

#[test]
fn a_check_that_gave_no_answer_changes_nothing_about_the_claim() {
    let unjudged = |version: &str| checked(Some(VerdictKind::CouldNotJudge), version);

    // A new claim stays unchecked, and its attempts stay whole.
    let mut fresh = claim("c1");
    fresh.append(machine(2), unjudged("v1")).unwrap();
    assert_eq!(fresh.state(), &ClaimState::Unchecked);
    assert!(!fresh.may_repair());
    assert_eq!(fresh.unjudged(), 1);
    fresh.append(machine(3), unjudged("v1")).unwrap();
    assert_eq!(fresh.unjudged(), 2);
    // An answer ends the streak, and the claim is judged as any other.
    fresh
        .append(machine(4), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    assert_eq!(fresh.unjudged(), 0);
    assert_eq!(
        fresh.state(),
        &open(VerdictKind::Unsupported, MACHINE_ATTEMPTS)
    );

    // A rewrite keeps waiting for the check that decides it, and costs
    // nothing more than the rewrite itself.
    let mut rewritten = claim("c2");
    rewritten
        .append(machine(2), checked(Some(VerdictKind::Unsupported), "v1"))
        .unwrap();
    rewritten
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    rewritten.append(machine(4), unjudged("v2")).unwrap();
    assert_eq!(rewritten.state(), &open(VerdictKind::Unsupported, 2));
    assert!(
        !rewritten.may_repair(),
        "the rewrite still waits for its check"
    );
    rewritten
        .append(machine(5), checked(Some(VerdictKind::Unsupported), "v2"))
        .unwrap();
    assert!(matches!(
        rewritten.state(),
        ClaimState::Open {
            kind: VerdictKind::Unsupported,
            machine_attempts_left: 1,
            ..
        }
    ));

    // A claim that passed on text since changed has no check now.
    let mut passed = verified_claim("c3");
    passed.append(machine(3), unjudged("v2")).unwrap();
    assert_eq!(passed.state(), &ClaimState::Unchecked);
    assert!(!passed.state().is_resolved());
    passed.append(machine(4), checked(None, "v2")).unwrap();
    assert!(matches!(
        passed.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    // A claim waiting for the user stays the user's.
    let mut waiting = queued_claim("c4", VerdictKind::Unsupported);
    let before = waiting.state().clone();
    waiting.append(machine(7), unjudged("v3")).unwrap();
    assert_eq!(waiting.state(), &before);
}
