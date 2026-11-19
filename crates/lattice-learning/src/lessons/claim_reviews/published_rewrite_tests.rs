//! What a rewrite of a section does to its sentences and to the claims on them.
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, EventProvenance, UnitKind,
    UserAction,
};

use super::published_rewrite::{plan, sentence_diff, Hunk};

fn hunk(old: std::ops::Range<usize>, new: std::ops::Range<usize>) -> Hunk {
    Hunk { old, new }
}

#[test]
fn unchanged_text_has_no_hunks() {
    let text = "A one is first. B two is second. C three is third.";
    assert_eq!(sentence_diff(text, text), vec![]);
}

#[test]
fn one_changed_sentence_is_one_hunk_with_both_sides() {
    assert_eq!(
        sentence_diff(
            "A one is first. B two is second. C three is third.",
            "A one is first. B is now different. C three is third."
        ),
        vec![hunk(1..2, 1..2)]
    );
}

#[test]
fn an_added_sentence_is_a_hunk_with_only_a_new_side() {
    assert_eq!(
        sentence_diff(
            "A one is first. C three is third.",
            "A one is first. B two is second. C three is third."
        ),
        vec![hunk(1..1, 1..2)]
    );
}

#[test]
fn a_deleted_sentence_is_a_hunk_with_only_an_old_side() {
    assert_eq!(
        sentence_diff(
            "A one is first. B two is second. C three is third.",
            "A one is first. C three is third."
        ),
        vec![hunk(1..2, 1..1)]
    );
}

#[test]
fn a_moved_sentence_is_deleted_where_it_was_and_added_where_it_went() {
    assert_eq!(
        sentence_diff(
            "A one is first. B two is second. C three is third.",
            "B two is second. C three is third. A one is first."
        ),
        vec![hunk(0..1, 0..0), hunk(3..3, 2..3)]
    );
}

#[test]
fn rewrapping_the_lines_between_sentences_gives_no_hunk() {
    assert_eq!(
        sentence_diff(
            "A one is first. B two is second.\n\nC three is third.",
            "A one is first.\nB two is second. C three is third."
        ),
        vec![]
    );
}

#[test]
fn windows_line_breaks_give_the_same_hunks_as_unix_ones() {
    let unix = sentence_diff(
        "A one is first.\n\nB two is second.\n\nC three is third.",
        "A one is first.\n\nB is now different.\n\nC three is third.\n\nD four is added.",
    );
    let windows = sentence_diff(
        "A one is first.\r\n\r\nB two is second.\r\n\r\nC three is third.",
        "A one is first.\r\n\r\nB is now different.\r\n\r\nC three is third.\r\n\r\nD four is added.",
    );
    assert_eq!(unix, vec![hunk(1..2, 1..2), hunk(3..3, 3..4)]);
    assert_eq!(windows, unix);
}

fn provenance(actor: Actor) -> EventProvenance {
    EventProvenance {
        actor,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: None,
    }
}

/// A claim on `sentence` in `text`, as a pass would have left it.
fn claim(id: &str, field: &str, unit: usize, sentence: &str, text: &str) -> ClaimLedgerEntry {
    stating(id, field, unit, sentence, sentence, text)
}

/// A claim whose quote is `quote` and whose statement is `statement`.
fn stating(
    id: &str,
    field: &str,
    unit: usize,
    quote: &str,
    statement: &str,
    text: &str,
) -> ClaimLedgerEntry {
    ClaimLedgerEntry::extracted(
        ClaimId(id.into()),
        provenance(Actor::Machine),
        ClaimLocation::locate(unit, UnitKind::Block, field, quote, text),
        statement.into(),
    )
    .unwrap()
}

fn body_claim(id: &str, sentence: &str, text: &str) -> ClaimLedgerEntry {
    claim(id, "body", 0, sentence, text)
}

fn ids(list: &[ClaimId]) -> Vec<&str> {
    list.iter().map(|id| id.0.as_str()).collect()
}

const A: &str = "A one is first.";
const B: &str = "B two is second.";
const C: &str = "C three is third.";

fn old() -> String {
    format!("{A} {B} {C}")
}

#[test]
fn a_claim_on_an_unchanged_sentence_is_kept() {
    let old = old();
    let new = format!("{A} {B} {C} D four is added.");
    let plan = plan(&[body_claim("b", B, &old)], 0, true, "body", &old, &new);
    assert_eq!(ids(&plan.kept), ["b"]);
    assert!(plan.edited.is_empty() && plan.removed.is_empty());
}

#[test]
fn a_claim_on_a_changed_sentence_is_edited_with_the_new_sentence_as_its_successor() {
    let old = old();
    let new = format!("{A} B is now different. {C}");
    let plan = plan(&[body_claim("b", B, &old)], 0, true, "body", &old, &new);
    assert_eq!(plan.written.len(), 1);
    assert_eq!(plan.written[0].text, "B is now different.");
    assert_eq!(plan.edited.len(), 1);
    assert_eq!(plan.edited[0].0, ClaimId("b".into()));
    assert_eq!(plan.edited[0].1, 0);
    assert!(plan.kept.is_empty() && plan.removed.is_empty());
}

#[test]
fn two_claims_on_one_changed_sentence_both_go_to_it() {
    let old = old();
    let new = format!("{A} B is now different. {C}");
    let claims = [
        body_claim("one", B, &old),
        body_claim("two", "two is second", &old),
    ];
    let plan = plan(&claims, 0, true, "body", &old, &new);
    assert_eq!(plan.written.len(), 1);
    let edited: Vec<(&str, usize)> = plan
        .edited
        .iter()
        .map(|(id, at)| (id.0.as_str(), *at))
        .collect();
    assert_eq!(edited, [("one", 0), ("two", 0)]);
}

#[test]
fn a_claim_on_a_deleted_sentence_is_removed() {
    let old = old();
    let new = format!("{A} {C}");
    let plan = plan(&[body_claim("b", B, &old)], 0, true, "body", &old, &new);
    assert_eq!(ids(&plan.removed), ["b"]);
    assert!(plan.kept.is_empty() && plan.edited.is_empty() && plan.written.is_empty());
}

#[test]
fn a_claim_over_a_kept_and_a_deleted_sentence_is_removed() {
    let old = old();
    let new = format!("{A} {C}");
    let across = format!("{A} {B}");
    // A statement sharing no word with either sentence keeps the whole quote.
    let both = stating("both", "body", 0, &across, "", &old);
    let plan = plan(&[both], 0, true, "body", &old, &new);
    assert_eq!(ids(&plan.removed), ["both"]);
    assert!(plan.kept.is_empty() && plan.edited.is_empty());
}

#[test]
fn every_new_sentence_is_written_once_at_its_place_in_the_new_text() {
    let old = old();
    let new = format!("{A} X is new. Y is also new. {B} {C} Z comes last.");
    let plan = plan(&[], 0, true, "body", &old, &new);
    let written: Vec<(&str, &str)> = plan
        .written
        .iter()
        .map(|item| {
            (
                item.text.as_str(),
                new.get(item.span.0..item.span.1).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        written,
        [
            ("X is new.", "X is new."),
            ("Y is also new.", "Y is also new."),
            ("Z comes last.", "Z comes last.")
        ]
    );
    let successors: std::collections::HashSet<&str> = plan
        .written
        .iter()
        .map(|item| item.successor.0.as_str())
        .collect();
    assert_eq!(successors.len(), 3);
}

#[test]
fn a_claim_already_removed_or_edited_or_in_another_section_or_field_is_left_out() {
    let old = old();
    let new = format!("{A} {C}");
    let mut removed = body_claim("removed", B, &old);
    removed
        .append(
            provenance(Actor::User),
            ClaimEvent::UserAction {
                action: UserAction::Remove,
                note: None,
            },
        )
        .unwrap();
    let claims = [
        removed,
        claim("other-section", "body", 1, B, &old),
        claim("other-field", "title", 0, B, &old),
    ];
    let plan = plan(&claims, 0, true, "body", &old, &new);
    assert!(plan.kept.is_empty(), "{plan:?}");
    assert!(plan.edited.is_empty(), "{plan:?}");
    assert!(plan.removed.is_empty(), "{plan:?}");
}

#[test]
fn a_claim_whose_sentence_is_not_in_the_text_is_kept() {
    let old = old();
    let new = format!("{A} {C}");
    let missing = "Not in the text.";
    let elsewhere = claim("lost", "body", 0, missing, missing);
    let plan = plan(&[elsewhere], 0, true, "body", &old, &new);
    assert_eq!(ids(&plan.kept), ["lost"]);
}

#[test]
fn a_claim_on_the_title_field_follows_the_title_text() {
    let title = "Boiling is a phase change.";
    let plan = plan(
        &[claim("t", "title", 0, title, title)],
        0,
        true,
        "title",
        title,
        "Boiling is not a phase change.",
    );
    assert_eq!(plan.edited.len(), 1);
    assert_eq!(plan.written[0].text, "Boiling is not a phase change.");
}

#[test]
fn a_claim_on_a_choices_reason_is_an_answer_key_claim_edited_when_that_reason_is_rewritten() {
    const FIRST: &str = "B fails because it reverses the order.";
    let old = format!("{FIRST} It also drops the cache.");
    let new = "B fails because it swaps the order. It also drops the cache.";
    let claim = ClaimLedgerEntry::extracted(
        ClaimId("reason".into()),
        provenance(Actor::Machine),
        ClaimLocation::locate(2, UnitKind::AnswerKey, "rationales/1", FIRST, &old),
        FIRST.into(),
    )
    .unwrap();

    let plan = plan(&[claim], 2, false, "rationales/1", &old, new);

    assert_eq!(plan.edited.len(), 1);
    assert_eq!(plan.edited[0].0, ClaimId("reason".into()));
    assert_eq!(plan.written.len(), 1);
    assert!(plan.kept.is_empty() && plan.removed.is_empty());
}
