#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The review pass writes the user's put back and restore into the draft.

use super::*;
use lattice_core::domain::learning::claim_review::{
    ClaimLocation, EventProvenance, PutPlace, UnitKind,
};
use serde_json::json;

fn provenance(actor: Actor) -> EventProvenance {
    EventProvenance {
        actor,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: None,
    }
}

fn candidate() -> Value {
    json!({"blocks": [{"title": "A", "body": "Heat it. Cool it."}], "questions": []})
}

fn put_back(sentence: &str, anchor: Anchor) -> Change {
    Change::PutBack {
        pointer: "/blocks/0/body".into(),
        anchor,
        sentence: sentence.into(),
    }
}

fn body(candidate: &Value) -> &str {
    candidate
        .pointer("/blocks/0/body")
        .unwrap()
        .as_str()
        .unwrap()
}

#[test]
fn a_put_back_goes_into_the_draft_beside_its_neighbour() {
    let mut draft = candidate();
    apply(
        None,
        &mut draft,
        vec![put_back("Boil it.", Anchor::After("Heat it.".into()))],
    );
    assert_eq!(body(&draft), "Heat it. Boil it. Cool it.");
    let mut draft = candidate();
    apply(
        None,
        &mut draft,
        vec![put_back("Boil it.", Anchor::Before("Cool it.".into()))],
    );
    assert_eq!(body(&draft), "Heat it. Boil it. Cool it.");
}

#[test]
fn a_put_back_whose_neighbour_is_gone_goes_to_the_end_and_is_never_written_twice() {
    let mut draft = candidate();
    let change = || put_back("Boil it.", Anchor::After("Chill it.".into()));
    apply(None, &mut draft, vec![change()]);
    assert_eq!(body(&draft), "Heat it. Cool it. Boil it.");
    apply(None, &mut draft, vec![change()]);
    assert_eq!(body(&draft), "Heat it. Cool it. Boil it.");
}

#[test]
fn a_restore_replaces_the_rewrite_with_the_original_once() {
    let mut draft = candidate();
    let change = || Change::Replace {
        pointer: "/blocks/0/body".into(),
        from: "Cool it.".into(),
        to: "Chill it a little.".into(),
    };
    apply(None, &mut draft, vec![change()]);
    assert_eq!(body(&draft), "Heat it. Chill it a little.");
    apply(None, &mut draft, vec![change()]);
    assert_eq!(body(&draft), "Heat it. Chill it a little.");
}

fn extracted() -> ClaimLedgerEntry {
    ClaimLedgerEntry::extracted(
        ClaimId("c1".into()),
        provenance(Actor::Machine),
        ClaimLocation::locate(0, UnitKind::Block, "body", "Boil it.", "Boil it."),
        "Boil it.".into(),
    )
    .unwrap()
}

fn user_put_back() -> ClaimEvent {
    ClaimEvent::UserAction {
        action: UserAction::PutBack {
            sentence: "Boil it.".into(),
            edited: false,
            place: PutPlace {
                unit_index: 0,
                field: "body".into(),
                anchor: Anchor::End,
            },
        },
        note: None,
    }
}

fn is_put_back(action: &UserAction) -> bool {
    matches!(action, UserAction::PutBack { .. })
}

#[test]
fn a_put_back_is_live_until_the_user_takes_it_back_or_acts_on_the_claim_again() {
    let mut entry = extracted();
    entry
        .append(
            provenance(Actor::Machine),
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine,
            },
        )
        .unwrap();
    assert!(live(&entry, is_put_back).is_none());
    entry
        .append(provenance(Actor::User), user_put_back())
        .unwrap();
    assert!(live(&entry, is_put_back).is_some());

    let mut taken_back = entry.clone();
    let ordinal = taken_back.undo_target().unwrap();
    taken_back
        .append(
            provenance(Actor::User),
            ClaimEvent::Undone {
                event_ordinal: ordinal,
            },
        )
        .unwrap();
    assert!(live(&taken_back, is_put_back).is_none());

    let mut removed = entry;
    removed
        .append(
            provenance(Actor::User),
            ClaimEvent::UserAction {
                action: UserAction::Remove,
                note: None,
            },
        )
        .unwrap();
    assert!(live(&removed, is_put_back).is_none());
}
