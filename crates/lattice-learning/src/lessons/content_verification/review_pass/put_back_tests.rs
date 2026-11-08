#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The review pass writes the user's put back and restore into the draft.

use super::*;
use lattice_core::domain::learning::claim_review::{
    ClaimLocation, CutPlace, EventProvenance, PutPlace, UnitKind,
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
        cuts: Vec::new(),
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

const ALPHA: &str = "Alpha comes first.";
const BRAVO: &str = "Bravo comes second.";
const CHARLIE: &str = "Charlie comes third.";
const DELTA: &str = "Delta comes fourth.";
const ECHO: &str = "Echo comes last.";

/// A claim whose sentence the machine cut, recorded between `before` and
/// `after`, that the user put back to `stored`.
fn put_back_claim(
    id: &str,
    sentence: &str,
    before: &str,
    after: &str,
    stored: Anchor,
) -> ClaimLedgerEntry {
    put_back_claim_in(0, id, sentence, before, after, stored)
}

fn put_back_claim_in(
    unit: usize,
    id: &str,
    sentence: &str,
    before: &str,
    after: &str,
    stored: Anchor,
) -> ClaimLedgerEntry {
    let mut entry = ClaimLedgerEntry::extracted(
        ClaimId(id.into()),
        provenance(Actor::Machine),
        ClaimLocation::locate(unit, UnitKind::Block, "body", sentence, sentence),
        sentence.into(),
    )
    .unwrap();
    entry
        .append(
            provenance(Actor::Machine),
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine,
            },
        )
        .unwrap();
    let cut = CutPlace {
        sentence: sentence.into(),
        before: Some(before.into()),
        after: Some(after.into()),
    };
    entry
        .append(provenance(Actor::Machine), ClaimEvent::SentenceCut { cut })
        .unwrap();
    entry
        .append(
            provenance(Actor::User),
            ClaimEvent::UserAction {
                action: UserAction::PutBack {
                    sentence: sentence.into(),
                    edited: false,
                    place: PutPlace {
                        unit_index: unit,
                        field: "body".into(),
                        anchor: stored,
                    },
                },
                note: None,
            },
        )
        .unwrap();
    entry
}

/// Bravo, Charlie and Delta cut one after the other, each recorded against
/// the text the cuts before it left, and put back to where the first cut's
/// record puts them all.
fn adjacent_put_backs() -> [ClaimLedgerEntry; 3] {
    let after_alpha = || Anchor::After(ALPHA.into());
    [
        put_back_claim("bravo", BRAVO, ALPHA, CHARLIE, after_alpha()),
        put_back_claim("charlie", CHARLIE, ALPHA, DELTA, after_alpha()),
        put_back_claim("delta", DELTA, ALPHA, ECHO, after_alpha()),
    ]
}

fn pass(entries: &[ClaimLedgerEntry], text: &str) -> String {
    let mut draft = json!({"blocks": [{"title": "A", "body": text}], "questions": []});
    let found = changes(entries, &draft);
    apply(None, &mut draft, found);
    body(&draft).to_owned()
}

#[test]
fn adjacent_cut_sentences_the_user_put_back_are_written_in_their_original_order_whatever_the_order_of_the_claims(
) {
    let [bravo, charlie, delta] = adjacent_put_backs();
    let original = [ALPHA, BRAVO, CHARLIE, DELTA, ECHO].join(" ");
    for order in [
        [&bravo, &charlie, &delta],
        [&delta, &bravo, &charlie],
        [&charlie, &delta, &bravo],
        [&delta, &charlie, &bravo],
    ] {
        let entries: Vec<ClaimLedgerEntry> = order.iter().map(|entry| (*entry).clone()).collect();
        assert_eq!(
            pass(&entries, &[ALPHA, ECHO].join(" ")),
            original,
            "{:?}",
            entries
                .iter()
                .map(|entry| entry.claim_id().0.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_put_back_a_pass_already_wrote_is_not_written_again_beside_the_others() {
    let entries = adjacent_put_backs();
    let once = pass(&entries, &[ALPHA, ECHO].join(" "));
    assert_eq!(pass(&entries, &once), once);
}

#[test]
fn a_sentence_the_user_wrote_between_the_recorded_neighbours_sends_the_put_back_to_its_stored_place(
) {
    let entries = [put_back_claim(
        "charlie",
        CHARLIE,
        ALPHA,
        ECHO,
        Anchor::Before(ECHO.into()),
    )];
    let mine = "The user wrote this one.";
    assert_eq!(
        pass(&entries, &[ALPHA, mine, ECHO].join(" ")),
        [ALPHA, mine, CHARLIE, ECHO].join(" ")
    );
}

#[test]
fn a_section_end_the_user_chose_is_kept_over_the_cut_record() {
    let entries = [put_back_claim("bravo", BRAVO, ALPHA, CHARLIE, Anchor::End)];
    assert_eq!(
        pass(&entries, &[ALPHA, CHARLIE, ECHO].join(" ")),
        [ALPHA, CHARLIE, ECHO, BRAVO].join(" ")
    );
}

#[test]
fn a_cut_recorded_in_another_block_does_not_make_this_blocks_place_ambiguous() {
    let entries = [
        put_back_claim("bravo", BRAVO, ALPHA, ECHO, Anchor::After(ECHO.into())),
        put_back_claim_in(1, "zulu", "Zulu comes elsewhere.", ECHO, ALPHA, Anchor::End),
    ];
    let body_a = [ALPHA, ECHO].join(" ");
    let mut draft = json!({"blocks": [
        {"title": "A", "body": body_a},
        {"title": "B", "body": "Other."},
    ], "questions": []});
    let found = changes(&entries, &draft);
    apply(None, &mut draft, found);
    assert_eq!(body(&draft), [ALPHA, BRAVO, ECHO].join(" "));
}
