//! Where a sentence the machine took out goes back: beside a neighbour the
//! cut recorded, beside the claims around it, through a claim that survived
//! the same section, or where the user chooses.

use lattice_core::domain::learning::claim_review::{
    Actor, Anchor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, CutPlace, EventProvenance,
    PutPlace, UnitKind,
};
use lattice_core::shared::error::AppError;
use serde_json::{json, Value};

use super::put_back::{place, resolve, Lesson, CHOOSE};

fn machine() -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: None,
    }
}

fn location(unit: usize, quote: &str, span: Option<(usize, usize)>) -> ClaimLocation {
    ClaimLocation {
        unit_index: unit,
        unit_kind: UnitKind::Block,
        field: "body".into(),
        quote: quote.into(),
        span,
    }
}

fn claim(id: &str, unit: usize, quote: &str, span: Option<(usize, usize)>) -> ClaimLedgerEntry {
    ClaimLedgerEntry::extracted(
        ClaimId(id.into()),
        machine(),
        location(unit, quote, span),
        quote.into(),
    )
    .unwrap()
}

fn moved(mut entry: ClaimLedgerEntry, to: usize, quote: &str) -> ClaimLedgerEntry {
    entry
        .append(
            machine(),
            ClaimEvent::Extracted {
                location: location(to, quote, None),
                statement: quote.into(),
            },
        )
        .unwrap();
    entry
}

fn cut(mut entry: ClaimLedgerEntry, before: Option<&str>, after: Option<&str>) -> ClaimLedgerEntry {
    let sentence = entry.location().quote.clone();
    entry
        .append(
            machine(),
            ClaimEvent::SentenceCut {
                cut: CutPlace {
                    sentence,
                    before: before.map(str::to_owned),
                    after: after.map(str::to_owned),
                },
            },
        )
        .unwrap();
    entry
}

fn lesson(bodies: &[&str]) -> Value {
    let blocks: Vec<Value> = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| json!({"title": format!("Part {index}"), "body": body}))
        .collect();
    json!({ "blocks": blocks, "questions": [] })
}

fn here(units: &Value) -> Lesson<'_> {
    Lesson {
        units,
        published: false,
    }
}

fn put(unit_index: usize, anchor: Anchor) -> PutPlace {
    PutPlace {
        unit_index,
        field: "body".into(),
        anchor,
    }
}

#[test]
fn a_sentence_goes_after_the_sentence_that_stood_before_it_found_by_text_not_by_its_old_unit() {
    let units = lesson(&["Loops repeat.", "Heat it. Cool it."]);
    let gone = cut(
        claim("c1", 7, "Boil it.", Some((9, 17))),
        Some("Heat it."),
        Some("Cool it."),
    );
    assert_eq!(
        place(&here(&units), std::slice::from_ref(&gone), &gone),
        Some(put(1, Anchor::After("Heat it.".into())))
    );
}

#[test]
fn a_sentence_goes_before_the_one_after_it_when_the_one_before_is_gone_too() {
    let units = lesson(&["Loops repeat.", "Cool it."]);
    let gone = cut(
        claim("c1", 0, "Boil it.", None),
        Some("Heat it."),
        Some("Cool it."),
    );
    assert_eq!(
        place(&here(&units), std::slice::from_ref(&gone), &gone),
        Some(put(1, Anchor::Before("Cool it.".into())))
    );
}

#[test]
fn a_neighbour_that_stands_in_two_sections_is_not_a_place() {
    let units = lesson(&["Heat it.", "Heat it. Cool it."]);
    let gone = cut(claim("c1", 0, "Boil it.", None), Some("Heat it."), None);
    let placed = place(&here(&units), std::slice::from_ref(&gone), &gone);
    assert_eq!(
        placed.map(|place| place.unit_index),
        Some(0),
        "a recorded neighbour no one can place falls to the claims around it"
    );
}

#[test]
fn an_old_cut_goes_beside_the_claims_recorded_spans_in_its_unit() {
    let units = lesson(&["Heat it. Cool it. Serve it."]);
    let gone = claim("c1", 0, "Boil it.", Some((9, 17)));
    let before = claim("c2", 0, "Heat it.", Some((0, 8)));
    let after = claim("c3", 0, "Serve it.", Some((18, 27)));
    let claims = [before, gone.clone(), after];
    assert_eq!(
        place(&here(&units), &claims, &gone),
        Some(put(0, Anchor::After("Heat it.".into())))
    );
}

#[test]
fn an_old_cut_with_no_claim_beside_it_goes_to_the_end_of_its_field() {
    let units = lesson(&["Heat it."]);
    let gone = claim("c1", 0, "Boil it.", Some((9, 17)));
    assert_eq!(
        place(&here(&units), std::slice::from_ref(&gone), &gone),
        Some(put(0, Anchor::End))
    );
}

#[test]
fn after_a_section_went_out_the_place_is_found_through_a_claim_of_the_same_original_section() {
    let units = lesson(&["Loops repeat.", "Heat it. Cool it."]);
    let gone = claim("c1", 3, "Boil it.", Some((9, 17)));
    let survivor = moved(claim("c2", 3, "Cool it.", Some((18, 26))), 1, "Cool it.");
    let other = moved(
        claim("c3", 1, "Loops repeat.", Some((0, 13))),
        0,
        "Loops repeat.",
    );
    let claims = [gone.clone(), survivor, other];
    let placed = place(&here(&units), &claims, &gone).unwrap();
    assert_eq!(placed.unit_index, 1, "not the stale unit 3");
}

#[test]
fn after_a_section_went_out_a_sentence_with_no_survivor_of_its_section_has_no_known_place() {
    let units = lesson(&["Loops repeat.", "Heat it."]);
    let gone = claim("c1", 3, "Boil it.", Some((9, 17)));
    let moved_one = moved(claim("c2", 2, "Heat it.", Some((0, 8))), 1, "Heat it.");
    let claims = [gone.clone(), moved_one];
    assert_eq!(place(&here(&units), &claims, &gone), None);
    let refused = resolve(&here(&units), &claims, &gone, None).unwrap_err();
    assert!(matches!(refused, AppError::InvalidInput(message) if message == CHOOSE));
}

#[test]
fn the_user_may_choose_the_section_even_when_the_place_is_known() {
    let units = lesson(&["Loops repeat.", "Heat it. Cool it."]);
    let gone = cut(claim("c1", 1, "Boil it.", None), Some("Heat it."), None);
    let chosen = resolve(&here(&units), std::slice::from_ref(&gone), &gone, Some(0)).unwrap();
    assert_eq!(chosen, put(0, Anchor::End));
    let known = resolve(&here(&units), std::slice::from_ref(&gone), &gone, None).unwrap();
    assert_eq!(known, put(1, Anchor::After("Heat it.".into())));
    let missing = resolve(&here(&units), std::slice::from_ref(&gone), &gone, Some(9)).unwrap_err();
    assert!(matches!(missing, AppError::InvalidInput(_)));
}
