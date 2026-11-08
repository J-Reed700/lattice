//! A sentence the machine took out may be put back, and the original of a
//! rewrite restored; both are the user's, and Undo takes them back.

use super::*;

fn taken_out() -> ClaimLedgerEntry {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Removed))
        .unwrap();
    entry
}

fn rewritten() -> ClaimLedgerEntry {
    let mut entry = claim("c1");
    entry
        .append(machine(2), checked(Some(VerdictKind::Contradicted), "v1"))
        .unwrap();
    entry
        .append(machine(3), rewrite(1, false, RepairOutcome::Rewritten))
        .unwrap();
    entry
        .append(
            machine(4),
            ClaimEvent::Superseded {
                successor: Some(ClaimId("c2".into())),
                by: Actor::Machine,
            },
        )
        .unwrap();
    entry
}

fn put_back() -> UserAction {
    UserAction::PutBack {
        sentence: QUOTE.into(),
        edited: false,
        place: PutPlace {
            unit_index: 0,
            field: "body".into(),
            anchor: Anchor::End,
        },
    }
}

#[test]
fn a_sentence_the_machine_took_out_may_be_put_back_and_then_it_is_confirmed_by_the_user() {
    let mut entry = taken_out();
    assert!(entry.taken_out());
    assert!(entry.allows(ClaimActionKind::PutBack));
    assert!(!entry.allows(ClaimActionKind::RestoreOriginal));
    entry.append(user(4), act(put_back(), None)).unwrap();
    assert!(matches!(
        entry.state(),
        ClaimState::Confirmed { bound, .. } if bound == &HumanBinding::new(QUOTE, Vec::new())
    ));
    assert!(!entry.taken_out());
}

#[test]
fn undo_after_a_put_back_leaves_the_sentence_taken_out_again() {
    let mut entry = taken_out();
    entry.append(user(4), act(put_back(), None)).unwrap();
    let ordinal = entry.undo_target().unwrap();
    entry
        .append(
            user(5),
            ClaimEvent::Undone {
                event_ordinal: ordinal,
            },
        )
        .unwrap();
    assert!(entry.taken_out());
    assert!(entry.allows(ClaimActionKind::PutBack));
}

#[test]
fn a_claim_that_is_not_taken_out_cannot_be_put_back() {
    let mut open = claim("c1");
    assert!(!open.allows(ClaimActionKind::PutBack));
    assert!(open.append(user(2), act(put_back(), None)).is_err());
    let mut removed_by_user = claim("c2");
    removed_by_user
        .append(user(2), act(UserAction::Remove, None))
        .unwrap();
    assert!(!removed_by_user.allows(ClaimActionKind::PutBack));
}

#[test]
fn a_sentence_the_machine_rewrote_offers_its_original_and_not_a_put_back() {
    let mut entry = rewritten();
    assert_eq!(entry.rewrite(), Some((QUOTE, "Water boils at 100 °C.")));
    assert!(entry.allows(ClaimActionKind::RestoreOriginal));
    assert!(!entry.allows(ClaimActionKind::PutBack));
    assert!(!entry.taken_out());
    entry
        .append(
            user(5),
            act(
                UserAction::RestoreOriginal {
                    original: QUOTE.into(),
                    rewrite: "Water boils at 100 °C.".into(),
                },
                None,
            ),
        )
        .unwrap();
    assert!(matches!(entry.state(), ClaimState::Confirmed { .. }));
}

#[test]
fn a_sentence_taken_out_has_no_original_to_restore() {
    let mut entry = taken_out();
    assert_eq!(entry.rewrite(), None);
    let restore = UserAction::RestoreOriginal {
        original: QUOTE.into(),
        rewrite: "x".into(),
    };
    assert!(entry.append(user(4), act(restore, None)).is_err());
}

#[test]
fn a_source_does_not_bring_a_sentence_back_by_itself() {
    let mut entry = taken_out();
    let attached = ClaimEvent::EvidenceAttached {
        source_version_id: "v2".into(),
        provenance: Provenance::PastedText,
        result: AttachOutcome::Checked(local_check(None, "v2")),
    };
    assert!(entry.allows(ClaimActionKind::AttachEvidence));
    entry.append(user(4), attached).unwrap();
    assert!(
        entry.taken_out(),
        "the text is put back first, by the user's put back"
    );
}

#[test]
fn the_place_a_cut_recorded_is_the_claims_and_old_claims_have_none() {
    let mut entry = taken_out();
    assert_eq!(entry.cut_place(), None);
    let cut = CutPlace {
        sentence: QUOTE.into(),
        before: Some("Heat it.".into()),
        after: Some("Then cool it.".into()),
    };
    entry
        .append(machine(4), ClaimEvent::SentenceCut { cut: cut.clone() })
        .unwrap();
    assert_eq!(entry.cut_place(), Some(&cut));
    assert!(entry.taken_out(), "recording the place changes no state");
}

#[test]
fn an_event_stored_before_put_back_existed_still_reads() {
    let stored = r#"{"kind":"user_action","action":"remove","note":null}"#;
    let event: ClaimEvent = serde_json::from_str(stored).unwrap();
    assert!(matches!(
        event,
        ClaimEvent::UserAction {
            action: UserAction::Remove,
            ..
        }
    ));
    let cut = ClaimEvent::SentenceCut {
        cut: CutPlace {
            sentence: "A.".into(),
            before: None,
            after: None,
        },
    };
    let round: ClaimEvent = serde_json::from_str(&serde_json::to_string(&cut).unwrap()).unwrap();
    assert_eq!(round, cut);
}
