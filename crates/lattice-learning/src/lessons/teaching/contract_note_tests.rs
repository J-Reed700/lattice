//! What the notes of the check's findings say.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::confirm::Settled;
use super::*;

fn part(index: usize) -> Part {
    Part {
        id: format!("section-{index}"),
        site: FindingSite::Section(index),
        texts: vec!["Alpha holds the line.".to_owned()],
        shown: Value::Null,
    }
}

fn missing_note(belongs: Option<&Part>) -> String {
    let answer = Answer {
        planned: Vec::new(),
        used_before_taught: Vec::new(),
        contradictions: Vec::new(),
        unseen: Vec::new(),
    };
    let item = Item {
        id: "action-1".to_owned(),
        kind: "action",
        text: "Hold the line".to_owned(),
    };
    let settled = Settled {
        taught: Vec::new(),
        missing: vec![(&item, belongs)],
        uses: Vec::new(),
    };
    let found = findings(&answer, &settled, &[], &Value::Null, &[]);
    assert_eq!(found.len(), 1);
    found[0].note.clone()
}

#[test]
fn a_missing_item_is_told_the_part_it_fits_best_in() {
    let placed = part(1);
    let note = missing_note(Some(&placed));
    assert!(note.ends_with(" It fits best in section 2."), "{note}");
}

#[test]
fn a_missing_item_with_no_part_is_not_told_one() {
    let note = missing_note(None);
    assert!(!note.contains("fits best"), "{note}");
}
