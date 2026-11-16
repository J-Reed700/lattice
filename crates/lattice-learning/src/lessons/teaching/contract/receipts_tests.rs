//! When what a review saw taught still stands.
#![allow(clippy::indexing_slicing)]
use super::super::{items_of, parts_of};
use super::Receipts;
use serde_json::json;

#[test]
fn an_idea_s_receipt_lapses_once_its_sentence_lies_only_in_a_question() {
    let items = items_of(&json!({"ideas": ["Parcels are weighed before they ship."]}));
    let mut receipts = Receipts::default();
    receipts.resume(
        &json!({"receipts": {"idea-0": ["A parcel is weighed before it ships."]}}).to_string(),
    );
    let taught = json!({"blocks": [
        {"kind": "explanation", "title": "Weighing", "body": "A parcel is weighed before it ships."}
    ], "questions": []});
    receipts.lapse(&parts_of(&taught), &items);
    assert!(receipts.holds(&items[0]));
    let moved = json!({"blocks": [
        {"kind": "explanation", "title": "Weighing", "body": "Scales are checked each morning."}
    ], "questions": [
        {"prompt": "Why is a parcel weighed?", "explanation": "A parcel is weighed before it ships."}
    ]});
    receipts.lapse(&parts_of(&moved), &items);
    assert!(!receipts.holds(&items[0]));
}
