//! What the notes of the check's findings say.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::confirm::{Settled, Unmet};
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
        reports: Vec::new(),
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
        clashes: Vec::new(),
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

fn used_note(protected: &[Protected]) -> String {
    let parts = [part(0)];
    let hit = first_hit(&parts, "Alpha holds the line.").unwrap();
    let answer = Answer {
        planned: Vec::new(),
        used_before_taught: Vec::new(),
        contradictions: Vec::new(),
        unseen: Vec::new(),
        reports: Vec::new(),
    };
    let settled = Settled {
        taught: Vec::new(),
        missing: Vec::new(),
        uses: vec![Unmet {
            hit,
            what: "the line",
            elsewhere: Vec::new(),
            later: None,
        }],
        clashes: Vec::new(),
    };
    let candidate = json!({"blocks":[{"body":"Alpha holds the line."}],"questions":[]});
    let found = findings(&answer, &settled, &parts, &candidate, protected);
    assert_eq!(found.len(), 1);
    found[0].note.clone()
}

#[test]
fn a_use_in_a_protected_passage_is_told_to_teach_beside_the_quoted_words() {
    let protected = [Protected {
        unit: 0,
        field: "body".into(),
        quote: "Alpha holds the line.".into(),
        released: false,
    }];
    let note = used_note(&protected);
    assert!(
        note.ends_with(concat!("Teach it in this step, where it is first used (", each_shown!(), "), in sentences added before or after the quoted words; the quoted words stay exactly as they are.")),
        "{note}"
    );
    assert!(!note.contains("change the step"), "{note}");
}

#[test]
fn a_use_in_a_passage_nothing_protects_keeps_its_note() {
    let note = used_note(&[]);
    assert!(
        note.ends_with(concat!("Teach it in this step, where it is first used (", each_shown!(), "), or change the step to use only what the lesson has taught (a record asked for in the learner's own notes needs nothing taught).")),
        "{note}"
    );
}

#[test]
fn the_answer_names_each_thing_once_with_every_use_and_where_it_is_taught_later() {
    let schema = schema(&[], 0);
    let entry = &schema["properties"]["usedBeforeTaught"]["items"];
    assert_eq!(entry["required"], json!(["what", "uses", "taughtLater"]));
    assert_eq!(entry["additionalProperties"], json!(false));
    assert_eq!(entry["properties"]["uses"]["type"], json!("array"));
    assert_eq!(entry["properties"]["uses"]["minItems"], json!(1));
    assert_eq!(entry["properties"]["uses"]["items"]["minLength"], json!(1));
    assert_eq!(entry["properties"]["what"]["minLength"], json!(1));
    assert_eq!(
        entry["properties"]["taughtLater"],
        json!({"type": "string"})
    );
    assert!(entry["properties"].get("quote").is_none());
}
